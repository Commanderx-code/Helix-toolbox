import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/installers/commander'))
spec = importlib.util.spec_from_file_location('zellij_config', ROOT / 'scripts/installers/commander/zellij-config.py')
zellij = importlib.util.module_from_spec(spec)
spec.loader.exec_module(zellij)


class ZellijTests(unittest.TestCase):
    def test_installer_package_paths_and_failure_boundaries(self):
        script = ROOT / 'scripts/installers/commander/zellij.sh'
        for manager, existing, candidate, install_ok, answer, success in (
            ('pacman', False, '0.44.3', True, 'APPLY', True),
            ('dnf', False, '0.44.3', True, 'APPLY', True),
            ('apt-get', False, '0.44.3', True, 'APPLY', True),
            ('nala', False, '0.44.3', True, 'APPLY', True),
            ('apt-get', False, '(none)', True, 'APPLY', False),
            ('dnf', False, '0.44.3', False, 'APPLY', False),
            ('pacman', True, '0.44.3', True, 'APPLY', True),
            ('pacman', False, '0.44.3', True, 'no', False),
        ):
            with self.subTest(manager=manager, existing=existing, candidate=candidate,
                              install_ok=install_ok, answer=answer), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                # Stub shared helpers, leaving the actual installer logic intact.
                (root / 'common.sh').write_text('''commander_init() { ESCALATION_TOOL=env; }
checkCommandRequirements() { :; }
confirm() { read -r answer; [ "$answer" = APPLY ] || exit 1; }
command_exists() { [ -f ./installed ]; }
install_packages() { printf '%s\\n' "$*" > ./packages; [ "$INSTALL_OK" = yes ] || return 1; touch ./installed; }
''')
                for name, content in {
                    'apt-get': '#!/bin/sh\nexit 0\n',
                    'nala': '#!/bin/sh\nexit 0\n',
                    'apt-cache': '#!/bin/sh\nprintf "Candidate: %s\\n" "$CANDIDATE"\n',
                    'python3': '#!/bin/sh\ntouch ./configured\n',
                    'zellij': '#!/bin/sh\nexit 0\n',
                }.items():
                    binary = root / name
                    binary.write_text(content)
                    binary.chmod(0o755)
                if existing:
                    (root / 'installed').touch()
                result = subprocess.run(['sh', '-e', str(script)], cwd=root, input=answer + '\n',
                                        text=True, capture_output=True, env=dict(os.environ,
                                        PATH=str(root) + ':' + os.environ['PATH'], PACKAGER=manager,
                                        CANDIDATE=candidate, INSTALL_OK='yes' if install_ok else 'no'))
                self.assertEqual(result.returncode == 0, success, result.stderr)
                self.assertEqual((root / 'configured').exists(), success)
                if existing or answer != 'APPLY' or candidate == '(none)':
                    self.assertFalse((root / 'packages').exists())

    def test_preset_and_repeat_install(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory)
            zellij.install(config)
            target = config / 'zellij/config.kdl'
            self.assertIn('default_mode "locked"', target.read_text())
            self.assertIn('theme "tokyo-night-storm"', target.read_text())
            self.assertNotIn('default_shell', target.read_text())
            target.write_text('personal settings')
            zellij.install(config)
            self.assertEqual(target.read_text(), 'personal settings')

    def test_managed_links_are_preserved(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory)
            (config / 'zellij').mkdir()
            target = config / 'zellij/config.kdl'
            target.symlink_to(config / 'missing-store-path')
            zellij.install(config)
            self.assertTrue(target.is_symlink())
            self.assertFalse(target.exists())

    def test_symlink_parent_is_not_written(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory)
            real = config / 'managed'
            real.mkdir()
            (config / 'zellij').symlink_to(real)
            with self.assertRaises(ValueError):
                zellij.install(config)
            self.assertEqual(list(real.iterdir()), [])
