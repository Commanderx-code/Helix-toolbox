import importlib.util
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'
spec = importlib.util.spec_from_file_location('configure', TOOLS / 'configure.py')
configure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(configure)


class GrubThemeTests(unittest.TestCase):
    def test_ctt_shodan_fonts_are_discoverable_and_assets_preserved(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'repo/themes/Shodan'
            (source / 'fonts').mkdir(parents=True)
            (source / 'theme.txt').write_text('desktop-image: "background.png"')
            (source / 'background.png').write_bytes(b'theme image')
            (source / 'fonts/unifont_16.pf2').write_bytes(b'font data')
            (root / 'repo/LICENSE').write_text('source license')
            target = root / 'prepared'
            configure.prepare_grub_theme(root / 'repo', target, 'ctt-shodan')
            self.assertEqual((target / 'unifont_16.pf2').read_bytes(), b'font data')
            self.assertEqual((target / 'background.png').read_bytes(), b'theme image')
            self.assertEqual((target / 'LICENSE').read_text(), 'source license')

    def test_cachyos_theme_comes_from_the_dotfiles_folder(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            source = root / 'repo/grub/themes/cachyos'
            (source / 'icons').mkdir(parents=True)
            (source / 'theme.txt').write_text('desktop-image: "background.png"')
            (source / 'background.png').write_bytes(b'image')
            (source / 'terminus-18.pf2').write_bytes(b'font')
            (source / 'icons/garuda.png').write_bytes(b'icon')
            (source / 'LICENSE').write_text('gpl')
            target = root / 'prepared'
            configure.prepare_grub_theme(root / 'repo', target, 'cachyos')
            self.assertEqual((target / 'background.png').read_bytes(), b'image')
            self.assertEqual((target / 'terminus-18.pf2').read_bytes(), b'font')
            self.assertEqual((target / 'icons/garuda.png').read_bytes(), b'icon')
            self.assertEqual((target / 'LICENSE').read_text(), 'gpl')

    def test_commander_theme_is_not_prepared_from_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaises(ValueError):
                configure.prepare_grub_theme(Path(tmp), Path(tmp) / 'prepared', 'commander')
            self.assertFalse((Path(tmp) / 'prepared').exists())

    def test_default_restores_plain_appearance_only(self):
        original = 'GRUB_THEME="old"\nGRUB_BACKGROUND="old.png"\nGRUB_TERMINAL_OUTPUT="gfxterm"\nGRUB_TIMEOUT=7\nGRUB_CMDLINE_LINUX="root=UUID=target quiet"\n'
        restored = configure.render_grub(original, '', '')
        self.assertIn('GRUB_THEME=""', restored)
        self.assertIn('GRUB_BACKGROUND=""', restored)
        self.assertIn('GRUB_TERMINAL_OUTPUT="console"', restored)
        self.assertIn('GRUB_TIMEOUT=7', restored)
        self.assertIn('root=UUID=target quiet', restored)
        self.assertEqual(configure.render_grub(restored, '', ''), restored)

    def test_themes_do_not_override_serial_terminal(self):
        with self.assertRaises(ValueError):
            configure.render_grub('GRUB_TERMINAL="serial"\n')

    def run_transaction(self, failure=None, variant='default', package_available=True):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            defaults = root / 'etc/default/grub'
            defaults.parent.mkdir(parents=True)
            original = 'GRUB_THEME="custom"\nGRUB_CMDLINE_LINUX="keep-me"\n'
            defaults.write_text(original)
            boot = root / 'boot/grub'
            boot.mkdir(parents=True)
            menu = boot / 'grub.cfg'
            menu.write_text('original boot menu')
            garuda_theme = root / 'usr/share/grub/themes/garuda-dr460nized'
            garuda_theme.mkdir(parents=True)
            (garuda_theme / 'theme.txt').write_text('title-text: ""')
            script = (TOOLS / 'grub.sh').read_text()
            script = script.replace('/etc/default/grub', str(defaults)).replace('/boot/grub', str(boot))
            script = script.replace('/usr/share/grub/themes/garuda-dr460nized', str(garuda_theme))
            (root / 'grub.sh').write_text(script)
            shutil.copy2(TOOLS / 'configure.py', root / 'configure.py')
            (root / 'common.sh').write_text('''commander_init() { DTYPE=debian; PACKAGER=pacman; ESCALATION_TOOL=env; }
checkCommandRequirements() { :; }
command_exists() { return 0; }
confirm() { :; }
install_packages() { echo "install $*" >> ./calls.log; }
backup_system() { backup="$1.backup"; cp -a "$1" "$backup"; }
''')
            pacman = root / 'pacman'
            pacman.write_text('#!/bin/sh\nexit ' + ('0' if package_available else '1') + '\n')
            pacman.chmod(0o755)
            for name in ('grub-mkconfig', 'grub-script-check'):
                executable = root / name
                if (failure == 'generate' and name == 'grub-mkconfig') or (failure == 'check' and name == 'grub-script-check'):
                    executable.write_text('#!/bin/sh\nexit 1\n')
                elif name == 'grub-mkconfig':
                    executable.write_text('#!/bin/sh\nprintf "new menu" > "$2"\n')
                else:
                    executable.write_text('#!/bin/sh\nexit 0\n')
                executable.chmod(0o755)
            import os
            result = subprocess.run(['sh', '-e', './grub.sh', variant], cwd=root,
                                    env=dict(os.environ, PATH=str(root) + ':' + os.environ['PATH']),
                                    text=True, capture_output=True)
            if failure or not package_available:
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(defaults.read_text(), original)
                self.assertEqual(menu.read_text(), 'original boot menu')
                self.assertFalse((root / 'calls.log').exists())
            elif variant == 'commander':
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn(f'GRUB_THEME="{garuda_theme}/theme.txt"', defaults.read_text())
                self.assertIn('GRUB_TERMINAL_OUTPUT="gfxterm"', defaults.read_text())
                self.assertIn('keep-me', defaults.read_text())
                self.assertEqual(menu.read_text(), 'new menu')
                self.assertEqual((root / 'calls.log').read_text(), 'install grub-theme-garuda-dr460nized\n')
            else:
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertIn('GRUB_THEME=""', defaults.read_text())
                self.assertIn('keep-me', defaults.read_text())
                self.assertEqual(menu.read_text(), 'new menu')
            self.assertEqual(list(boot.glob('.commander-grub.*')), [])

    def test_generation_failure_rolls_back(self):
        self.run_transaction('generate')

    def test_syntax_failure_rolls_back(self):
        self.run_transaction('check')

    def test_success_replaces_menu(self):
        self.run_transaction()

    def test_commander_theme_installs_the_garuda_package(self):
        self.run_transaction(variant='commander')

    def test_commander_theme_needs_the_garuda_repositories(self):
        self.run_transaction(variant='commander', package_available=False)
