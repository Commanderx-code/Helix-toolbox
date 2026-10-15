import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'
HIDDEN = {'flatpak', 'curl', 'pacman', 'apt-get', 'dnf', 'zypper', 'sudo', 'doas', 'id', 'fusermount', 'fusermount3'}

FLATPAK = '#!/bin/sh\necho "flatpak $*" >> "$FAKE/log"\n[ "$1" != install ] || [ ! -e "$FAKE/fail-$(echo "$*" | grep -o "nvidia" | head -n 1)" ]\n'
# Writes what $FAKE/download holds to the -o path.
CURL = '#!/bin/sh\nwhile [ $# -gt 0 ]; do [ "$1" = -o ] && out=$2; shift; done\ncp "$FAKE/download" "$out"\n'
APPIMAGE = b'\x7fELF\x02\x01\x01\x00AI\x02' + b'\x00' * 64


class RuneScapeTests(unittest.TestCase):
    """The Jagex Launcher scripts against stand-in flatpak, curl and package tools."""

    def setUp(self):
        self.fake = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.fake)
        self.home = self.fake / 'home'
        self.home.mkdir()
        self.bin = self.fake / 'bin'
        self.bin.mkdir()
        self.tools = self.fake / 'tools'
        self.tools.mkdir()
        for directory in ('/usr/bin', '/bin'):
            for tool in Path(directory).iterdir():
                link = self.tools / tool.name
                if tool.name not in HIDDEN and not os.path.lexists(link):
                    link.symlink_to(tool)
        real_id = shutil.which('id')
        self.stand_in('id', f'#!/bin/sh\n[ "$1" = -u ] && echo 1000 && exit\nexec {real_id} "$@"\n')
        self.stand_in('sudo', '#!/bin/sh\nexec "$@"\n')
        self.stand_in('pacman', '#!/bin/sh\necho "pacman $*" >> "$FAKE/log"\n')
        (self.fake / 'log').write_text('')

    def stand_in(self, name, text):
        (self.bin / name).write_text(text)
        (self.bin / name).chmod(0o755)

    def run_script(self, name, nvidia=None):
        script = (TOOLS / name).read_text().replace('/proc/driver/nvidia/version', str(self.fake / 'nvidia'))
        (self.fake / name).write_text(script)
        if nvidia:
            (self.fake / 'nvidia').write_text(nvidia)
        env = {'PATH': f'{self.bin}:{self.tools}', 'FAKE': str(self.fake), 'HOME': str(self.home),
               'HELIXFORGE_LIB': str(ROOT / 'scripts/shared')}
        return subprocess.run(['sh', '-e', str(self.fake / name)], cwd=self.fake, env=env,
                              capture_output=True, text=True, stdin=subprocess.DEVNULL)

    def log(self):
        return (self.fake / 'log').read_text().splitlines()

    def test_the_flatpak_comes_from_its_repository_with_the_32_bit_runtime(self):
        self.stand_in('flatpak', FLATPAK)
        result = self.run_script('jagex-launcher-flatpak.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        log = self.log()
        self.assertIn('flatpak remote-add --user --if-not-exists JLauncher '
                      'https://nmlynch94.github.io/com.jagexlauncher.JagexLauncher/JagexLauncher.flatpakrepo', log)
        runtime = next(line for line in log if 'Compat.i386' in line)
        self.assertIn('org.freedesktop.Platform.GL32.default/x86_64/25.08', runtime)
        self.assertIn('flatpak install --user -y JLauncher com.jagexlauncher.JagexLauncher', log)
        self.assertFalse(any('nvidia' in line for line in log))

    def test_nvidia_libraries_match_the_driver_and_their_absence_is_not_fatal(self):
        self.stand_in('flatpak', FLATPAK)
        version = 'NVRM version: NVIDIA UNIX Open Kernel Module for x86_64  615.78.08  Release Build  (root@pc)\n'
        result = self.run_script('jagex-launcher-flatpak.sh', nvidia=version)
        self.assertEqual(result.returncode, 0, result.stderr)
        nvidia = next(line for line in self.log() if 'nvidia' in line)
        self.assertIn('org.freedesktop.Platform.GL.nvidia-615-78-08/x86_64', nvidia)
        self.assertIn('org.freedesktop.Platform.GL32.nvidia-615-78-08/x86_64', nvidia)
        (self.fake / 'fail-nvidia').write_text('')
        result = self.run_script('jagex-launcher-flatpak.sh', nvidia=version)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('no libraries for NVIDIA driver 615-78-08', result.stdout)

    def test_without_flatpak_nothing_happens(self):
        result = self.run_script('jagex-launcher-flatpak.sh')
        self.assertEqual(result.returncode, 1)
        self.assertIn('Flatpak is not installed', result.stderr)

    def test_the_appimage_is_kept_with_a_command_and_a_menu_entry(self):
        self.stand_in('curl', CURL)
        self.stand_in('fusermount3', '#!/bin/sh\n')
        (self.fake / 'download').write_bytes(APPIMAGE)
        result = self.run_script('jagex-launcher-appimage.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        appimage = self.home / '.local/share/helixforge/appimages/jagex-launcher.AppImage'
        self.assertEqual(appimage.read_bytes(), APPIMAGE)
        self.assertTrue(os.access(appimage, os.X_OK))
        self.assertEqual((self.home / '.local/bin/jagex-launcher').resolve(), appimage)
        entry = (self.home / '.local/share/applications/jagex-launcher.desktop').read_text()
        self.assertIn(f'Exec={appimage}\n', entry)
        self.assertEqual(self.log(), [], 'fuse3 is not installed when fusermount3 is there')
        self.assertEqual([p.name for p in appimage.parent.iterdir()], ['jagex-launcher.AppImage'])

    def test_a_download_that_is_not_an_appimage_changes_nothing(self):
        self.stand_in('curl', CURL)
        self.stand_in('fusermount3', '#!/bin/sh\n')
        (self.fake / 'download').write_bytes(b'<html>Service unavailable</html>')
        result = self.run_script('jagex-launcher-appimage.sh')
        self.assertEqual(result.returncode, 1)
        self.assertIn('not an AppImage', result.stderr)
        self.assertEqual(list((self.home / '.local/share/helixforge/appimages').iterdir()), [])
        self.assertFalse((self.home / '.local/bin/jagex-launcher').exists())

    def test_fuse3_is_installed_when_appimages_cannot_start(self):
        self.stand_in('curl', CURL)
        (self.fake / 'download').write_bytes(APPIMAGE)
        result = self.run_script('jagex-launcher-appimage.sh')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.log(), ['pacman -S --needed fuse3'])


if __name__ == '__main__':
    unittest.main()
