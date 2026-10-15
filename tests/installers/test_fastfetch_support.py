from pathlib import Path
import os
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/installers/commander/fastfetch-support.sh'


class ImageSupportTests(unittest.TestCase):
    def run_installer(self, manager, present=True, features='imagemagick7'):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'fastfetch-distro.py').write_text(
                'import sys\n'
                f'if "--version" in sys.argv: sys.exit({0 if present else 1})\n'
                f'print({features!r})\n'
            )
            script = 'install_packages() { printf "PACKAGES: %s\\n" "$*"; }\n. "$1"\ninstall_fastfetch_support\n'
            return subprocess.run(['sh', '-e', '-c', script, 'sh', str(HELPER)],
                                  cwd=root, env=dict(os.environ, PACKAGER=manager),
                                  capture_output=True, text=True)

    def test_distro_package_names_with_existing_fastfetch(self):
        for manager in ('apt-get', 'nala', 'pacman', 'dnf'):
            result = self.run_installer(manager)
            self.assertEqual(result.returncode, 0, result.stderr)
            expected = 'ImageMagick ImageMagick-libs' if manager == 'dnf' else 'imagemagick'
            self.assertIn('PACKAGES: ' + expected + '\n', result.stdout)
            self.assertIn('includes the Kitty/Sixel image backend', result.stdout)

    def test_missing_fastfetch_is_installed_with_image_support(self):
        result = self.run_installer('apt-get', present=False)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('PACKAGES: fastfetch imagemagick', result.stdout)

    def test_missing_compiled_backend_is_reported(self):
        result = self.run_installer('dnf', features='zlib')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('lacks its image backend', result.stdout)
        self.assertNotIn('includes the Kitty/Sixel', result.stdout)

    def test_unsupported_manager_does_not_install(self):
        result = self.run_installer('unknown')
        self.assertNotEqual(result.returncode, 0)
        self.assertNotIn('PACKAGES:', result.stdout)
