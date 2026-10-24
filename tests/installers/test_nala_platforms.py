import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NalaPlatformTests(unittest.TestCase):
    def test_platforms_and_missing_package(self):
        script = (ROOT / 'scripts/installers/system-setup/debian/nala-setup.sh').read_text()
        for distro, family, candidate, expected in [
            ('zorin', 'ubuntu debian', '0.15.1', 0),
            ('ubuntu', 'debian', '0.15.1', 0),
            ('debian', '', '0.15.1', 0),
            ('fedora', '', '0.15.1', 1),
            ('zorin', 'ubuntu debian', '(none)', 1),
        ]:
            with self.subTest(distro=distro, candidate=candidate), tempfile.TemporaryDirectory() as tmp:
                # The script runs from its own directory inside a copy of the
                # installers tree, as the engine runs it, and reaches the real
                # Commander configure.py through its relative path.
                root = Path(tmp)
                installers = root / 'scripts/installers'
                directory = installers / 'system-setup/debian'
                directory.mkdir(parents=True)
                (installers / 'commander').mkdir()
                shutil.copy(ROOT / 'scripts/installers/commander/configure.py', installers / 'commander')
                (installers / 'common-script.sh').write_text('''checkEnv() { ESCALATION_TOOL=env; }
checkCommandRequirements() { :; }
command_exists() { [ -f "$HOME/installed" ]; }
id() { printf '1000\\n'; }
''')
                (root / 'os-release').write_text(f'ID={distro}\nID_LIKE="{family}"\n')
                (directory / 'nala-setup.sh').write_text(script.replace('/etc/os-release', str(root / 'os-release')))
                home = root / 'home'
                home.mkdir()
                bin_directory = root / 'bin'
                bin_directory.mkdir()
                mocks = {
                    'apt-get': '#!/bin/sh\necho "$*" >> "$HOME/operations"\n[ "$1" != install ] || touch "$HOME/installed"\n',
                    'apt-cache': '#!/bin/sh\nprintf "Candidate: %s\\n" "$CANDIDATE"\n',
                }
                for name, content in mocks.items():
                    path = bin_directory / name
                    path.write_text(content)
                    path.chmod(0o755)
                env = dict(os.environ, HOME=str(home), SHELL='/bin/bash', CANDIDATE=candidate,
                           XDG_CONFIG_HOME=str(home / '.config'), XDG_DATA_HOME=str(home / '.local/share'),
                           PATH=str(bin_directory) + ':' + os.environ['PATH'])
                env.pop('ZDOTDIR', None)
                result = subprocess.run(['sh', '-e', './nala-setup.sh'], cwd=directory, input='APPLY\n',
                    text=True, capture_output=True, env=env)
                self.assertEqual(result.returncode, expected, result.stderr)
                self.assertEqual((home / 'installed').exists(), expected == 0)
                bashrc = home / '.bashrc'
                self.assertEqual(bashrc.exists(), expected == 0)
                if expected == 0:
                    self.assertIn("alias apt='nala'", bashrc.read_text())
                if distro == 'fedora':
                    self.assertFalse((home / 'operations').exists())
                else:
                    self.assertIn(f'Alias destination: {bashrc}', result.stdout)
