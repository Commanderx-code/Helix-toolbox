import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]

# The release's file names, as the workflows publish them; the HelixForge and
# window files sit beside the command line's and must never be mistaken for it.
RELEASE_FILES = [
    'helix-toolbox-linux-x86_64',
    'helix-toolbox_1.25.18-1_amd64.deb',
    'helix-toolbox_1.25.18-1_arm64.deb',
    'helix-toolbox-1.25.18-1.aarch64.rpm',
    'helix-toolbox-1.25.18-1.x86_64.rpm',
    'helix-toolbox-1.25.18-1-aarch64.pkg.tar.zst',
    'helix-toolbox-1.25.18-1-x86_64.pkg.tar.zst',
    'helix-toolbox-gui_1.25.18_amd64.deb',
    'helix-toolbox-gui_1.25.18_arm64.deb',
    'helix-toolbox-gui-1.25.18-1.aarch64.rpm',
    'helix-toolbox-gui-1.25.18-1.x86_64.rpm',
    'helix-toolbox-gui_1.25.18_amd64.AppImage',
    'helix-toolbox-gui-1.25.18-1-aarch64.pkg.tar.zst',
    'helix-toolbox-gui-1.25.18-1-x86_64.pkg.tar.zst',
    'helixforge_1.25.18-1_amd64.deb',
    'helixforge-1.25.18-1.x86_64.rpm',
    'helixforge-1.25.18-1-x86_64.pkg.tar.zst',
]

# What the script may call from the machine; everything else is a fake, so a
# real package manager on the machine running the tests is never found.
TOOLS = ['sh', 'grep', 'cut', 'sed', 'wc', 'sha256sum', 'mktemp', 'chmod', 'rm', 'cp', 'cat']


class InstallScriptTests(unittest.TestCase):
    """install.sh installs the release's checked packages with the package manager it finds."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.release = self.root / 'release'
        self.release.mkdir()
        for name in RELEASE_FILES:
            (self.release / name).write_text(f'contents of {name}\n')
        self.publish_sums()
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        for tool in TOOLS:
            (self.bin / tool).symlink_to(shutil.which(tool))
        self.fake('curl', 'while [ $# -gt 2 ]; do shift; done\ncp "${2#file://}" "$1"')
        self.fake('uname', 'echo "${ARCH:-x86_64}"')
        self.fake('id', 'echo "${UID_IS:-1000}"')
        self.fake('sudo', 'echo sudo >> "$RAN"\nexec "$@"')
        self.ran = self.root / 'ran'

    def fake(self, name, body):
        path = self.bin / name
        path.write_text('#!/bin/sh\n' + body + '\n')
        path.chmod(0o755)

    def manager(self, name):
        # Records its arguments, with the package files reduced to their names.
        self.fake(name, 'line=' + name + '\nfor a in "$@"; do line="$line ${a##*/}"; done\necho "$line" >> "$RAN"')

    def publish_sums(self, wrong=None):
        lines = []
        for name in RELEASE_FILES:
            digest = '0' * 64 if name == wrong else hashlib.sha256((self.release / name).read_bytes()).hexdigest()
            lines.append(f'{digest}  {name}\n')
        (self.release / 'SHA256SUMS').write_text(''.join(lines))

    def run_install(self, *arguments, piped=False, **env):
        environment = dict(PATH=str(self.bin), HOME=str(self.root), HELIX_TOOLBOX_RELEASE=f'file://{self.release}',
                           RAN=str(self.ran), TMPDIR=str(self.root), **env)
        script = ROOT / 'install.sh'
        if piped:
            command, stdin = ['sh', '-s', '--', *arguments], script.read_text()
        else:
            command, stdin = ['sh', str(script), *arguments], None
        # A session of its own has no terminal, as in CI.
        return subprocess.run(command, input=stdin, cwd=self.root, text=True, capture_output=True,
                              env=environment, start_new_session=True)

    def calls(self):
        return self.ran.read_text().splitlines() if self.ran.exists() else []

    def test_each_package_manager_installs_both_packages(self):
        expected = {
            'pacman': 'pacman -U --needed --noconfirm helix-toolbox-1.25.18-1-x86_64.pkg.tar.zst '
                      'helix-toolbox-gui-1.25.18-1-x86_64.pkg.tar.zst',
            'apt-get': 'apt-get install -y helix-toolbox_1.25.18-1_amd64.deb helix-toolbox-gui_1.25.18_amd64.deb',
            'zypper': 'zypper --non-interactive install --allow-unsigned-rpm helix-toolbox-1.25.18-1.x86_64.rpm '
                      'helix-toolbox-gui-1.25.18-1.x86_64.rpm',
            'dnf': 'dnf install -y helix-toolbox-1.25.18-1.x86_64.rpm helix-toolbox-gui-1.25.18-1.x86_64.rpm',
        }
        for manager, call in expected.items():
            with self.subTest(manager=manager):
                for other in expected:
                    (self.bin / other).unlink(missing_ok=True)
                self.ran.unlink(missing_ok=True)
                self.manager(manager)
                result = self.run_install()
                self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
                self.assertEqual(self.calls(), ['sudo', call])

    def test_aarch64_takes_the_arm_packages(self):
        self.manager('apt-get')
        result = self.run_install(ARCH='aarch64')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.calls()[-1],
                         'apt-get install -y helix-toolbox_1.25.18-1_arm64.deb helix-toolbox-gui_1.25.18_arm64.deb')

    def test_no_window_installs_the_command_line_only(self):
        self.manager('dnf')
        result = self.run_install('--no-window')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.calls()[-1], 'dnf install -y helix-toolbox-1.25.18-1.x86_64.rpm')

    def test_piped_to_sh_with_arguments(self):
        self.manager('pacman')
        result = self.run_install('--no-window', piped=True)
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual(self.calls()[-1], 'pacman -U --needed --noconfirm helix-toolbox-1.25.18-1-x86_64.pkg.tar.zst')

    def test_root_installs_without_sudo(self):
        self.manager('zypper')
        result = self.run_install(UID_IS='0')
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertNotIn('sudo', self.calls())

    def test_mismatched_checksum_installs_nothing(self):
        self.manager('apt-get')
        self.publish_sums(wrong='helix-toolbox-gui_1.25.18_amd64.deb')
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('does not match its published checksum', result.stdout)
        self.assertEqual(self.calls(), [])

    def test_a_package_missing_from_the_release_installs_nothing(self):
        self.manager('pacman')
        (self.release / 'SHA256SUMS').write_text('')
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_unsupported_processor_is_refused(self):
        self.manager('pacman')
        result = self.run_install(ARCH='riscv64')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('x86_64 and aarch64', result.stdout)
        self.assertEqual(self.calls(), [])

    def test_no_known_package_manager_is_refused(self):
        result = self.run_install()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('No pacman, apt, zypper or dnf', result.stdout)

    def test_unknown_option_is_refused(self):
        self.manager('pacman')
        result = self.run_install('--bogus')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])


if __name__ == '__main__':
    unittest.main()
