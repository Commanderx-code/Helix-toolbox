import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

UTILS = Path(__file__).resolve().parents[2] / 'scripts/installers/utils'
OPENSSL = shutil.which('openssl')

COMMON = '''RED= YELLOW= GREEN= CYAN= RC=
PACKAGER=pacman ESCALATION_TOOL=env
command_exists() { command -v "$1" >/dev/null 2>&1; }
checkEnv() { :; }
checkEscalationTool() { :; }
'''


def stand_in(folder, name, body):
    path = folder / name
    path.write_text('#!/bin/sh\n' + body)
    path.chmod(0o755)


class EncryptionToolTests(unittest.TestCase):
    """The file encryption tool keeps its password off the command line and
    the screen, and still opens what earlier versions encrypted."""

    def setUp(self):
        if OPENSSL is None:
            self.skipTest('openssl is not installed')
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        (self.root / 'utils').mkdir()
        shutil.copy(UTILS / 'encrypt_decrypt_tool.sh', self.root / 'utils/tool.sh')
        (self.root / 'common-script.sh').write_text(COMMON)
        self.bin = self.root / 'bin'
        self.bin.mkdir()
        self.calls = self.root / 'openssl.log'
        # Every openssl command line is written down, then the real one runs.
        stand_in(self.bin, 'openssl', f'printf "%s\\n" "$*" >> {self.calls}\nexec {OPENSSL} "$@"\n')
        stand_in(self.bin, 'clear', '')

    def tearDown(self):
        self.tmp.cleanup()

    def run_tool(self, answers):
        return subprocess.run(
            ['sh', 'tool.sh'], cwd=self.root / 'utils', input='\n'.join(answers) + '\n',
            capture_output=True, text=True, timeout=120,
            env={**os.environ, 'PATH': f'{self.bin}:{os.environ["PATH"]}'})

    def test_a_file_is_encrypted_and_decrypted_without_its_password_on_argv(self):
        plain = self.root / 'notes.txt'
        plain.write_text('the secret notes\n')
        sealed, opened = self.root / 'notes.enc', self.root / 'notes.out'
        result = self.run_tool(['1', str(plain), str(sealed), 'hunter2 two', 'hunter2 two', '',
                                '2', str(sealed), str(opened), 'hunter2 two', '', '3'])
        self.assertIn('Encrypted: ' + str(sealed), result.stdout, result.stderr)
        self.assertIn('Decrypted: ' + str(opened), result.stdout, result.stderr)
        self.assertEqual(opened.read_text(), 'the secret notes\n')
        self.assertTrue(sealed.read_bytes().startswith(b'Helix Toolbox enc pbkdf2 600000\n'))
        calls = self.calls.read_text()
        self.assertNotIn('hunter2', calls)
        self.assertNotIn(' -k ', calls)
        self.assertIn('-iter 600000', calls)

    def test_mismatched_passwords_encrypt_nothing(self):
        plain = self.root / 'notes.txt'
        plain.write_text('x\n')
        result = self.run_tool(['1', str(plain), str(self.root / 'notes.enc'), 'one', 'two', '', '3'])
        self.assertIn('Passwords do not match', result.stdout)
        self.assertFalse((self.root / 'notes.enc').exists())

    def test_a_file_from_the_earlier_version_still_opens(self):
        plain = self.root / 'old.txt'
        plain.write_text('from before\n')
        old = self.root / 'old.enc'
        # What the earlier version wrote: OpenSSL's default work factor, no header.
        subprocess.run([OPENSSL, 'enc', '-aes-256-cbc', '-salt', '-pbkdf2', '-pass', 'stdin',
                        '-in', str(plain), '-out', str(old)], input='sesame\n', text=True, check=True)
        opened = self.root / 'old.out'
        result = self.run_tool(['2', str(old), str(opened), 'sesame', '', '3'])
        self.assertIn('Decrypted: ' + str(opened), result.stdout, result.stderr)
        self.assertEqual(opened.read_text(), 'from before\n')

    def test_a_directory_round_trips(self):
        folder = self.root / 'docs'
        (folder / 'sub').mkdir(parents=True)
        (folder / 'a.txt').write_text('a\n')
        (folder / 'sub/b.txt').write_text('b\n')
        sealed, opened = self.root / 'sealed', self.root / 'opened'
        # A password no temporary path or option can contain by chance.
        secret = 'Plum-Kestrel-42'
        self.run_tool(['1', str(folder), str(sealed), secret, secret, '', '2', str(sealed), str(opened), secret, '', '3'])
        self.assertEqual((opened / 'a.txt').read_text(), 'a\n')
        self.assertEqual((opened / 'sub/b.txt').read_text(), 'b\n')
        self.assertNotIn(secret, self.calls.read_text())


class PasswordPromptTests(unittest.TestCase):
    """Passwords are read with echo off, and no script prints device names as
    escapes."""

    def test_password_reads_turn_echo_off(self):
        lines = [line.strip() for line in (UTILS / 'user-account-manager/add_user.sh').read_text().splitlines()]
        reads = [i for i, line in enumerate(lines) if line.startswith('read -r password')]
        self.assertEqual(len(reads), 2)
        for i in reads:
            self.assertEqual(lines[i - 1], 'stty -echo')
            self.assertEqual(lines[i + 1], 'stty echo')
        tool = (UTILS / 'encrypt_decrypt_tool.sh').read_text()
        self.assertNotIn('read -r PASSWORD', tool)
        self.assertIn('read_secret PASSWORD "Enter the decryption password: "', tool)

    def test_wifi_passphrase_is_not_on_nmcli_argv(self):
        text = (UTILS / 'wifi-control.sh').read_text()
        self.assertNotIn('password "$password"', text)
        self.assertIn('nmcli --ask dev wifi connect', text)

    def test_bluetooth_names_are_not_printed_as_escapes(self):
        text = (UTILS / 'bluetooth-control.sh').read_text()
        self.assertNotIn('printf "%b\\n" "$devices"', text)
        self.assertNotIn('printf "%b" "$devices"', text)


class BluetoothTests(unittest.TestCase):
    def test_control_characters_in_device_names_are_dropped(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'utils').mkdir()
            script = (UTILS / 'bluetooth-control.sh').read_text()
            (root / 'utils/bt.sh').write_text(script)
            (root / 'common-script.sh').write_text(COMMON)
            (root / 'common-service-script.sh').write_text('startAndEnableService() { :; }\n')
            bin_dir = root / 'bin'
            bin_dir.mkdir()
            stand_in(bin_dir, 'clear', '')
            # One device whose advertised name holds escapes, raw and spelled out.
            stand_in(bin_dir, 'bluetoothctl', 'case "$1" in devices) printf "Device AA:BB \\033[2JEvil\\\\033[H\\n" ;; esac\n')
            result = subprocess.run(['sh', 'bt.sh'], cwd=root / 'utils', input=b'1\n\n2\n0\n0\n',
                                    capture_output=True, timeout=60,
                                    env={**os.environ, 'PATH': f'{bin_dir}:{os.environ["PATH"]}'})
            out = result.stdout
            self.assertIn(b'Evil', out, result.stderr)
            self.assertNotIn(b'\x1b[2J', out)
            self.assertNotIn(b'\x1b[H', out)


if __name__ == '__main__':
    unittest.main()
