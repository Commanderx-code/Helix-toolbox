import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/installers/system-setup/arch/pacman-config.sh'

STOCK = '''[options]
#UseSyslog
#Color
#NoProgressBar
CheckSpace
#VerbosePkgLists
#ParallelDownloads = 5
DownloadUser = alpm

[core]
Include = /etc/pacman.d/mirrorlist

#[multilib]
#Include = /etc/pacman.d/mirrorlist
'''


class PacmanConfigTests(unittest.TestCase):
    """pacman-config.sh against copies of pacman.conf and makepkg.conf."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.pacman = self.root / 'pacman.conf'
        self.makepkg = self.root / 'makepkg.conf'
        here = self.root / 'a/b'
        here.mkdir(parents=True)
        text = SCRIPT.read_text().replace('/etc/pacman.conf', str(self.pacman)).replace('/etc/makepkg.conf', str(self.makepkg))
        (here / 'pacman-config.sh').write_text(text)
        (self.root / 'common-script.sh').write_text('checkEnv() { ESCALATION_TOOL=env; }\nRED=; RC=; GREEN=\n')
        self.cwd = here
        self.cores = subprocess.run(['nproc'], capture_output=True, text=True).stdout.strip()
        # A minimal system: no diffutils (cmp, diff), as in the Arch and Fedora images.
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        for directory in ('/usr/bin', '/bin'):
            for tool in Path(directory).iterdir():
                link = self.tools / tool.name
                if tool.name not in ('cmp', 'diff', 'diff3', 'sdiff') and not os.path.lexists(link):
                    link.symlink_to(tool)

    def run_script(self):
        result = subprocess.run(['sh', '-e', './pacman-config.sh'], cwd=self.cwd, capture_output=True, text=True,
                                env=dict(os.environ, PATH=str(self.tools)))
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def backups(self, path):
        return list(path.parent.glob(path.name + '.helixforge-backup-*'))

    def test_a_stock_config_is_turned_on_once(self):
        self.pacman.write_text(STOCK)
        self.makepkg.write_text('CFLAGS="-O2"\n#MAKEFLAGS="-j2"\n')
        out = self.run_script()
        conf = self.pacman.read_text()
        for line in ('Color\nILoveCandy\n', '\nVerbosePkgLists\n', '\nParallelDownloads = 5\n',
                     '\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n'):
            self.assertIn(line, conf)
        self.assertIn('#NoProgressBar', conf, 'other settings stay as they were')
        self.assertEqual(self.makepkg.read_text(), f'CFLAGS="-O2"\nMAKEFLAGS="-j{self.cores}"\n')
        self.assertIn('Color              turned on', out)
        self.assertIn('multilib           turned on', out)
        self.assertEqual(self.backups(self.pacman)[0].read_text(), STOCK)
        # A second run changes nothing and says so.
        out = self.run_script()
        self.assertEqual(self.pacman.read_text(), conf)
        self.assertEqual(conf.count('ILoveCandy'), 1)
        self.assertIn('pacman.conf is unchanged', out)
        self.assertIn('Color              already on', out)

    def test_repeated_candy_lines_are_removed_and_custom_makeflags_kept(self):
        self.pacman.write_text('[options]\nColor\nILoveCandy\nILoveCandy\nVerbosePkgLists\nILoveCandy\nParallelDownloads = 20\n[multilib]\nInclude = x\n')
        self.makepkg.write_text('MAKEFLAGS="-j8 -l8"\n')
        out = self.run_script()
        self.assertEqual(self.pacman.read_text().count('ILoveCandy'), 1)
        self.assertIn('removed 2 repeated line(s)', out)
        self.assertIn('ParallelDownloads  already on (20)', out)
        self.assertEqual(self.makepkg.read_text(), 'MAKEFLAGS="-j8 -l8"\n')
        self.assertIn('kept your own ("-j8 -l8")', out)
        self.assertEqual(self.backups(self.makepkg), [], 'an unchanged file is not backed up')

    def test_missing_options_are_added(self):
        self.pacman.write_text('[options]\nCheckSpace\n')
        self.makepkg.write_text('CFLAGS="-O2"\n')
        out = self.run_script()
        conf = self.pacman.read_text()
        self.assertIn('Color\nILoveCandy\n', conf)
        self.assertIn('ParallelDownloads = 5', conf)
        self.assertIn('[multilib]\nInclude = /etc/pacman.d/mirrorlist\n', conf)
        self.assertIn(f'MAKEFLAGS="-j{self.cores}"', self.makepkg.read_text())
        self.assertIn('ParallelDownloads  added (5)', out)


if __name__ == '__main__':
    unittest.main()
