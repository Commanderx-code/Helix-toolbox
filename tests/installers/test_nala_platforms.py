import os
from pathlib import Path
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
                root = Path(tmp)
                (root / 'os-release').write_text(f'ID={distro}\nID_LIKE="{family}"\n')
                (root / 'common.sh').write_text('''checkEnv() { ESCALATION_TOOL=env; }
checkCommandRequirements() { :; }
command_exists() { [ -f "$HOME/installed" ]; }
id() { printf '1000\\n'; }
''')
                (root / 'script.sh').write_text(script.replace('../../common-script.sh', './common.sh').replace('/etc/os-release', './os-release'))
                mocks = {
                    'apt-get': '#!/bin/sh\necho "$*" >> "$HOME/operations"\n[ "$1" != install ] || touch "$HOME/installed"\n',
                    'apt-cache': '#!/bin/sh\nprintf "Candidate: %s\\n" "$CANDIDATE"\n',
                    'python3': '#!/bin/sh\necho "$*" >> "$HOME/python-calls"\n',
                }
                for name, content in mocks.items():
                    path = root / name
                    path.write_text(content)
                    path.chmod(0o755)
                result = subprocess.run(['sh', '-e', './script.sh'], cwd=root, input='APPLY\n',
                    text=True, capture_output=True,
                    env=dict(os.environ, HOME=tmp, SHELL='/bin/bash', CANDIDATE=candidate,
                             PATH=tmp + ':' + os.environ['PATH']))
                self.assertEqual(result.returncode, expected, result.stderr)
                calls = (root / 'python-calls').read_text() if (root / 'python-calls').exists() else ''
                self.assertEqual('nala-alias' in calls, expected == 0)
                self.assertEqual((root / 'installed').exists(), expected == 0)
                if distro == 'fedora':
                    self.assertFalse((root / 'operations').exists())
