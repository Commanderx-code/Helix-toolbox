import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SHARED = ROOT / 'scripts/shared/common-script.sh'


def check_env(groups):
    """Run checkEnv from the shared helpers as a user in `groups`, with
    stand-in commands for openSUSE, and return the result."""
    with tempfile.TemporaryDirectory() as temp:
        temp = Path(temp)
        bin_dir = temp / 'bin'
        bin_dir.mkdir()
        stubs = {
            'id': f'case "$1" in -u) echo 1000 ;; -nG) echo "{groups}" ;; esac',
            'sudo': 'exit 0',
            'zypper': 'exit 0',
            'curl': 'exit 0',
            'groups': f'echo {groups}',
            'uname': 'echo x86_64',
        }
        for name, body in stubs.items():
            path = bin_dir / name
            path.write_text('#!/bin/sh\n' + body + '\n')
            path.chmod(0o755)
        for tool in ('dirname', 'realpath', 'grep'):
            (bin_dir / tool).symlink_to(subprocess.run(
                ['sh', '-c', f'command -v {tool}'], capture_output=True, text=True).stdout.strip())
        script = temp / 'entry.sh'
        script.write_text(f'#!/bin/sh -e\n. {SHARED}\ncheckEnv\necho READY\n')
        env = {'PATH': str(bin_dir), 'HOME': str(temp)}
        return subprocess.run(['/bin/sh', str(script)], env=env, cwd=temp,
                              capture_output=True, text=True, timeout=30)


class SuperUserTests(unittest.TestCase):
    def test_a_user_in_no_administrator_group_is_left_to_sudo(self):
        # openSUSE's sudoers lets every user run sudo with the root password
        # (Defaults targetpw); the first user is in no wheel/sudo group.
        result = check_env('users')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn('READY', result.stdout)
        self.assertIn('not in the wheel, sudo or root group; sudo decides', result.stdout)

    def test_the_administrator_group_is_named(self):
        for group in ('wheel', 'sudo'):
            with self.subTest(group=group):
                result = check_env(f'users {group}')
                self.assertIn(f'Administrator group: {group}', result.stdout)
                self.assertNotIn('not in the wheel', result.stdout)
                self.assertIn('READY', result.stdout)


if __name__ == '__main__':
    unittest.main()
