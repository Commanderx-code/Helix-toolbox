import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'
HIDDEN = {'pacman', 'sudo', 'doas', 'paru', 'yay', 'apt-get', 'nala', 'dnf', 'id'}

PACMAN = '''#!/bin/sh
echo "pacman $*" >> "$FAKE/log"
action=$1
shift
case "$action" in
    -Si) for name; do grep -qx "$name" "$FAKE/repo" || exit 1; done ;;
    -Q) for name; do grep -qx "$name" "$FAKE/installed" || exit 1; done ;;
    -S) for name; do [ "$name" = --needed ] || echo "$name" >> "$FAKE/installed"; done ;;
esac
'''
HELPER = '''#!/bin/sh
echo "paru $*" >> "$FAKE/log"
shift
for name; do [ "$name" = --needed ] || echo "$name" >> "$FAKE/installed"; done
'''


class CachyosGamingTests(unittest.TestCase):
    """cachyos-gaming.sh against stand-in pacman, sudo and paru; nothing real is installed."""

    def setUp(self):
        self.fake = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.fake)
        self.bin = self.fake / 'bin'
        self.bin.mkdir()
        # The system's own tools, without its package managers or sudo.
        self.tools = self.fake / 'tools'
        self.tools.mkdir()
        for directory in ('/usr/bin', '/bin'):
            for tool in Path(directory).iterdir():
                link = self.tools / tool.name
                if tool.name not in HIDDEN and not os.path.lexists(link):
                    link.symlink_to(tool)
        (self.bin / 'pacman').write_text(PACMAN)
        (self.bin / 'sudo').write_text('#!/bin/sh\nexec "$@"\n')
        # A normal user, also where the tests run as root, as in containers.
        real_id = shutil.which('id')
        (self.bin / 'id').write_text(f'#!/bin/sh\n[ "$1" = -u ] && echo "${{FAKE_UID:-1000}}" && exit\nexec {real_id} "$@"\n')
        for tool in self.bin.iterdir():
            tool.chmod(0o755)
        (self.fake / 'installed').write_text('')
        (self.fake / 'log').write_text('')

    def run_script(self, kind, repo=(), installed=(), helper=True):
        (self.fake / 'repo').write_text(''.join(f'{name}\n' for name in repo))
        (self.fake / 'installed').write_text(''.join(f'{name}\n' for name in installed))
        # True: a working paru; False: none; None: whatever is there.
        if helper:
            (self.bin / 'paru').write_text(HELPER)
            (self.bin / 'paru').chmod(0o755)
        elif helper is False:
            (self.bin / 'paru').unlink(missing_ok=True)
        env = dict(os.environ, PATH=f'{self.bin}:{self.tools}', FAKE=str(self.fake),
                   HELIXFORGE_LIB=str(ROOT / 'scripts/shared'))
        return subprocess.run(['sh', '-e', str(TOOLS / 'cachyos-gaming.sh'), kind], cwd=TOOLS, env=env,
                              capture_output=True, text=True, stdin=subprocess.DEVNULL)

    def installs(self):
        return [line for line in (self.fake / 'log').read_text().splitlines() if ' -S ' in f'{line} ']

    def test_cachyos_installs_its_own_meta_package(self):
        result = self.run_script('applications', repo=['cachyos-gaming-applications', 'lib32-gtk3'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.installs(), ['pacman -S --needed cachyos-gaming-applications'])

    def test_other_arch_systems_get_the_same_packages_and_they_are_verified(self):
        result = self.run_script('meta', repo=['lib32-gtk3'])
        self.assertEqual(result.returncode, 0, result.stderr)
        native, aur = self.installs()
        self.assertTrue(native.startswith('pacman -S --needed '))
        for name in ('umu-launcher', 'protontricks', 'winetricks', 'lib32-ocl-icd', 'wine'):
            self.assertIn(f' {name}', native)
        self.assertNotIn('steam', native)
        self.assertEqual(aur, 'paru -S --needed proton-cachyos-slr')
        self.assertIn('Installed and verified:', result.stdout)

    def test_applications_add_the_launchers(self):
        result = self.run_script('applications', repo=['lib32-gtk3'])
        self.assertEqual(result.returncode, 0, result.stderr)
        native, aur = self.installs()
        for name in ('steam', 'lutris', 'gamescope', 'lib32-mangohud', 'umu-launcher'):
            self.assertIn(f' {name}', native)
        self.assertEqual(aur, 'paru -S --needed proton-cachyos-slr heroic-games-launcher-bin faugus-launcher')

    def test_wine_staging_is_kept(self):
        result = self.run_script('meta', repo=['lib32-gtk3'], installed=['wine-staging'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertNotIn(' wine ', self.installs()[0] + ' ')
        self.assertIn('keeping it in place of wine', result.stdout)

    def test_nothing_is_installed_without_multilib_or_an_aur_helper(self):
        result = self.run_script('meta')
        self.assertEqual(result.returncode, 1)
        self.assertIn('[multilib]', result.stderr)
        result = self.run_script('meta', repo=['lib32-gtk3'], helper=False)
        self.assertEqual(result.returncode, 1)
        self.assertIn('paru or yay', result.stderr)
        self.assertEqual(self.installs(), [])

    def test_a_package_that_did_not_install_fails_the_run(self):
        (self.bin / 'paru').write_text('#!/bin/sh\necho "paru $*" >> "$FAKE/log"\n')
        (self.bin / 'paru').chmod(0o755)
        result = self.run_script('meta', repo=['lib32-gtk3'], helper=None)
        self.assertEqual(result.returncode, 1)
        self.assertIn('Not installed: proton-cachyos-slr', result.stderr)

    def test_root_is_refused(self):
        os.environ['FAKE_UID'] = '0'
        self.addCleanup(os.environ.pop, 'FAKE_UID')
        result = self.run_script('meta', repo=['lib32-gtk3'])
        self.assertEqual(result.returncode, 1)
        self.assertIn('normal user', result.stderr)
        self.assertEqual(self.installs(), [])

    def test_an_unknown_kind_is_refused(self):
        result = self.run_script('everything', repo=['lib32-gtk3'])
        self.assertEqual(result.returncode, 2)
        self.assertEqual(self.installs(), [])


if __name__ == '__main__':
    unittest.main()
