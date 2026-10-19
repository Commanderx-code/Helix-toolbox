import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class StartScriptTests(unittest.TestCase):
    """start.sh outside a checkout downloads the release and checks its sum."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.release = self.root / 'release'
        self.release.mkdir()
        shutil.copy2(ROOT / 'start.sh', self.root / 'start.sh')
        self.binary = self.release / 'helix-toolbox-linux-x86_64'
        self.binary.write_text('#!/bin/sh\necho "helix-toolbox ran $*" > "$RAN"\n')
        self.publish_sums()
        curl = self.root / 'curl'
        curl.write_text('#!/bin/sh\nwhile [ $# -gt 2 ]; do shift; done\ncp "${2#file://}" "$1"\n')
        curl.chmod(0o755)
        uname = self.root / 'uname'
        uname.write_text('#!/bin/sh\necho x86_64\n')
        uname.chmod(0o755)

    def publish_sums(self, digest=None):
        digest = digest or hashlib.sha256(self.binary.read_bytes()).hexdigest()
        (self.release / 'SHA256SUMS').write_text(f'{digest}  helix-toolbox-linux-x86_64\n')

    def run_start(self):
        return subprocess.run(['sh', str(self.root / 'start.sh'), '--help'], cwd=self.root, text=True, capture_output=True,
                              env=dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'],
                                       HELIX_TOOLBOX_RELEASE=f'file://{self.release}', RAN=str(self.root / 'ran')))

    def test_checked_release_runs_with_arguments(self):
        result = self.run_start()
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertEqual((self.root / 'ran').read_text(), 'helix-toolbox ran --help\n')

    def test_mismatched_checksum_is_not_executed(self):
        self.publish_sums('0' * 64)
        result = self.run_start()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('checksum', result.stdout)
        self.assertFalse((self.root / 'ran').exists())

    def test_release_without_the_asset_sum_is_refused(self):
        (self.release / 'SHA256SUMS').write_text('')
        result = self.run_start()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'ran').exists())


if __name__ == '__main__':
    unittest.main()
