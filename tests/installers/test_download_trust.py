"""How the general catalog's scripts trust what they download: vendor keys
are pinned, repositories are refreshed one at a time, and a remote file name
cannot leave the folder it is saved in."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLERS = ROOT / 'scripts/installers'
SHARED = ROOT / 'scripts/shared/common-script.sh'
GENERAL = [path for path in INSTALLERS.rglob('*.sh') if 'commander' not in path.relative_to(INSTALLERS).parts]


class RepositoryTrustTests(unittest.TestCase):
    def test_no_script_trusts_keys_or_sources_wholesale(self):
        # A global --gpg-auto-import-keys refresh takes any key any repository
        # offers; apt-key trusts a key for every repository; a source list
        # fetched over HTTP lets the network choose what apt trusts.
        patterns = {
            'global key import': re.compile(r'--gpg-auto-import-keys'),
            'apt-key': re.compile(r'\bapt-key\b'),
            'plain HTTP source': re.compile(r'http://[^\s"\']*\.(?:list|repo|sources)\b|addrepo\s+http://'),
        }
        for path in GENERAL:
            text = path.read_text(errors='replace')
            for what, pattern in patterns.items():
                with self.subTest(script=str(path.relative_to(ROOT)), what=what):
                    self.assertIsNone(pattern.search(text))


@unittest.skipUnless(shutil.which('gpg'), 'needs gpg')
class PinnedKeysTests(unittest.TestCase):
    """pinnedKeys with a real GnuPG, every keyring in a scratch folder."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        home = self.root / 'maker'
        home.mkdir(mode=0o700)
        self.gpg = ['gpg', '--homedir', str(home), '--batch', '--quiet', '--passphrase', '', '--pinentry-mode', 'loopback']
        self.keys = [self.make_key(name) for name in ('Vendor', 'Stranger')]
        self.served = self.root / 'served.asc'
        self.served.write_bytes(self.run_gpg('--armor', '--export', *self.keys).stdout)

    def run_gpg(self, *words):
        return subprocess.run(self.gpg + list(words), check=True, capture_output=True)

    def make_key(self, name):
        address = f'{name.lower()}@example.com'
        self.run_gpg('--quick-gen-key', f'{name} <{address}>', 'ed25519', 'sign', 'never')
        listing = self.run_gpg('--with-colons', '--list-keys', address).stdout.decode()
        return next(line.split(':')[9] for line in listing.splitlines() if line.startswith('fpr'))

    def pinned(self, output, *fingerprints):
        script = self.root / 'run.sh'
        script.write_text(f'''#!/bin/sh -e
. {SHARED}
ESCALATION_TOOL=env
curl() {{ while [ "$1" != -o ]; do shift; done; cp {self.served} "$2"; }}
pinnedKeys https://vendor.example/key.asc "$@"
''')
        return subprocess.run(['sh', str(script), str(output)] + list(fingerprints), capture_output=True, text=True)

    def primaries(self, keyring):
        lines = self.run_gpg('--with-colons', '--show-keys', str(keyring)).stdout.decode().splitlines()
        return [line.split(':')[9] for before, line in zip(lines, lines[1:]) if before.startswith('pub') and line.startswith('fpr')]

    def test_only_the_pinned_key_is_kept(self):
        for name in ('vendor.asc', 'keyrings/vendor.gpg'):
            with self.subTest(output=name):
                output = self.root / name
                result = self.pinned(output, self.keys[0])
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(self.primaries(output), [self.keys[0]])
                self.assertEqual(output.read_bytes().startswith(b'-----BEGIN PGP'), name.endswith('.asc'))

    def test_a_missing_pinned_key_adds_nothing(self):
        output = self.root / 'vendor.asc'
        result = self.pinned(output, self.keys[0], 'A' * 40)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not the one expected', result.stderr)
        self.assertFalse(output.exists())


class IsoFileNameTests(unittest.TestCase):
    """create-bootable-usb.sh saves a download under a plain name only."""

    def iso_file_name(self, name):
        text = (INSTALLERS / 'utils/create-bootable-usb.sh').read_text()
        function = re.search(r'^iso_file_name\(\) \{\n.*?^\}\n', text, re.M | re.S).group(0)
        return subprocess.run(['sh', '-c', function + 'iso_file_name "$1"', 'sh', name], capture_output=True, text=True)

    def test_plain_names_are_kept(self):
        for name, kept in (('Fedora-Workstation-Live-x86_64-44.iso', 'Fedora-Workstation-Live-x86_64-44.iso'),
                           ('archlinux-2026.10.01-x86_64.iso.xz', 'archlinux-2026.10.01-x86_64.iso.xz'),
                           ('releases/44/debian-13.1.0-amd64-netinst.iso', 'debian-13.1.0-amd64-netinst.iso')):
            with self.subTest(name=name):
                result = self.iso_file_name(name)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(result.stdout, kept + '\n')

    def test_names_that_reach_elsewhere_are_refused(self):
        for name in ('../.bashrc', '..', '.', '', '.config', '-o', 'a/../..', 'name with space.iso', 'x\n.iso', 'dir/'):
            with self.subTest(name=name):
                self.assertNotEqual(self.iso_file_name(name).returncode, 0)


if __name__ == '__main__':
    unittest.main()
