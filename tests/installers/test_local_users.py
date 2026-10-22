import getpass
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLERS = ROOT / 'scripts/installers'

# Clones the stand-in repository: what $FAKE/repo holds, copied whole.
GIT = '#!/bin/sh\n[ "$1" = clone ] || exit 2\necho "git clone $2 $3" >> "$FAKE/log"\ncp -R "$FAKE/repo" "$3"\n'


class SteamModTests(unittest.TestCase):
    """The Steam mod installers against a stand-in git, in a scratch home."""

    def setUp(self):
        self.fake = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.fake)
        self.home = self.fake / 'home'
        self.tmp = self.fake / 'tmp'
        self.tmp.mkdir()
        self.bin = self.fake / 'bin'
        self.bin.mkdir()
        (self.bin / 'git').write_text(GIT)
        (self.bin / 'git').chmod(0o755)
        (self.fake / 'log').write_text('')
        # The scripts source ../common-script.sh from their own directory.
        gaming = self.fake / 'installers/gaming'
        gaming.mkdir(parents=True)
        shutil.copy(INSTALLERS / 'common-script.sh', self.fake / 'installers')
        for name in ('common-steam-script.sh', 'arc-raiders-mod.sh', 'starfield-titus-mod.sh'):
            text = (INSTALLERS / 'gaming' / name).read_text()
            # Steam libraries are looked for only in the scratch home.
            text = text.replace('find "$HOME" /mnt', 'find "$HOME"')
            (gaming / name).write_text(text)
        self.gaming = gaming

    def library(self, game):
        common = self.home / '.local/share/Steam/steamapps/common' / game
        common.mkdir(parents=True)
        return common.parent.parent

    def run_script(self, name):
        env = {'PATH': f'{self.bin}:/usr/bin:/bin', 'FAKE': str(self.fake), 'HOME': str(self.home),
               'TMPDIR': str(self.tmp), 'HELIXFORGE_LIB': str(ROOT / 'scripts/shared')}
        return subprocess.run(['sh', name], cwd=self.gaming, env=env, capture_output=True, text=True,
                              stdin=subprocess.DEVNULL)

    def test_a_directory_made_in_advance_is_never_copied_from(self):
        steamapps = self.library('Arc Raiders')
        (self.fake / 'repo/compatdata').mkdir(parents=True)
        (self.fake / 'repo/compatdata/Engine.ini').write_text('from the clone\n')
        (self.fake / 'repo/common').mkdir()
        (self.fake / 'repo/common/intro.bk2').write_text('movie\n')
        # What another account could leave under the old fixed name.
        planted = self.tmp / 'arc-raiders'
        (planted / 'compatdata').mkdir(parents=True)
        (planted / 'compatdata/Engine.ini').write_text('planted\n')
        (planted / 'common').mkdir()
        (planted / 'common/planted.dll').write_text('planted\n')
        result = self.run_script('arc-raiders-mod.sh')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(any(line.startswith('git clone ') for line in (self.fake / 'log').read_text().splitlines()))
        config = steamapps / 'compatdata/1808500/pfx/drive_c/users/steamuser/Local Settings/Application Data/PioneerGame/Saved/Config/WindowsClient'
        self.assertEqual((config / 'Engine.ini').read_text(), 'from the clone\n')
        movies = steamapps / 'common/Arc Raiders/PioneerGame/Content/Movies/Frontend'
        self.assertTrue((movies / 'intro.bk2').exists())
        self.assertFalse((movies / 'planted.dll').exists())
        # The private clone is gone; what was planted is left alone.
        self.assertEqual(sorted(path.name for path in self.tmp.iterdir()), ['arc-raiders'])

    def test_an_engine_ini_symlink_is_refused_before_chmod(self):
        self.library('Arc Raiders')
        secret = self.home / 'secret'
        self.home.mkdir(exist_ok=True)
        secret.write_text('token\n')
        secret.chmod(0o600)
        (self.fake / 'repo/compatdata').mkdir(parents=True)
        (self.fake / 'repo/compatdata/Engine.ini').symlink_to(secret)
        (self.fake / 'repo/common').mkdir()
        result = self.run_script('arc-raiders-mod.sh')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('symbolic link', result.stdout)
        self.assertEqual(secret.stat().st_mode & 0o777, 0o600)

    def test_starfield_clones_fresh_even_when_its_old_directory_exists(self):
        steamapps = self.library('Starfield')
        game = steamapps / 'common/Starfield'
        (game / 'Starfield.exe').write_text('stock\n')
        (self.fake / 'repo').mkdir()
        (self.fake / 'repo/sfse_loader.exe').write_text('loader from the clone\n')
        (self.fake / 'repo/StarfieldCustom.ini').write_text('[General]\n')
        (steamapps / 'compatdata/1716740/pfx/drive_c/users/steamuser/Documents/My Games/Starfield').mkdir(parents=True)
        (self.tmp / 'starfield-config').mkdir()
        (self.tmp / 'starfield-config/sfse_loader.exe').write_text('planted\n')
        result = self.run_script('starfield-titus-mod.sh')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual((game / 'Starfield.exe').read_text(), 'loader from the clone\n')

    def test_no_mod_script_reuses_its_clone_directory(self):
        for script in (INSTALLERS / 'gaming').glob('*-mod.sh'):
            self.assertNotIn('[ -d "$tmp_dir" ] ||', script.read_text(), script.name)
        common = (INSTALLERS / 'gaming/common-steam-script.sh').read_text()
        self.assertNotIn('tmp_dir="/tmp/', common)
        self.assertIn('mktemp -d', common)


class LogCleanupTests(unittest.TestCase):
    """The Full System Cleanup's log truncation, run on a scratch tree."""

    def test_logs_in_directories_others_can_write_are_left_alone(self):
        script = (INSTALLERS / 'system-setup/system-cleanup.sh').read_text()
        command = re.search(r'"\$ESCALATION_TOOL" (find /var/log .*?)\n    fi', script, re.S).group(1)
        logs = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, logs)
        # This account stands in for root: tests never run as root.
        me = getpass.getuser()
        command = command.replace('/var/log', str(logs)).replace('-user root', f'-user {me}').replace('\\\n', ' ')
        (logs / 'system.log').write_text('old\n')
        (logs / 'notes.txt').write_text('kept\n')
        service = logs / 'service'
        service.mkdir()
        (service / 'service.log').write_text('kept\n')
        service.chmod(0o775)
        shared = logs / 'shared'
        shared.mkdir()
        (shared / 'shared.log').write_text('kept\n')
        shared.chmod(0o777)
        subprocess.run(['sh', '-c', command], check=True)
        self.assertEqual((logs / 'system.log').read_text(), '')
        self.assertEqual((logs / 'notes.txt').read_text(), 'kept\n')
        self.assertEqual((service / 'service.log').read_text(), 'kept\n')
        self.assertEqual((shared / 'shared.log').read_text(), 'kept\n')


class SambaShareTests(unittest.TestCase):
    def test_the_share_is_never_made_world_writable(self):
        script = (INSTALLERS / 'utils/samba-ssh-setup.sh').read_text()
        self.assertNotIn('chmod -R', script)
        self.assertNotIn('0777', script)
        self.assertIn('chmod 0770 "$SHARED_DIR"', script)


if __name__ == '__main__':
    unittest.main()
