import importlib.util
import os
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

FRAGMENT = '# look\ntimeout: 2\nwallpaper: boot():/dotfiles-splash.png\nterm_foreground: cdd6f4\ninterface_branding:\n'
MACHINE = ('# Limine boot loader configuration\ntimeout: 5\nremember_last_entry: yes\nwallpaper: boot():/a.png\nwallpaper: boot():/b.png\n\n'
           '/+CachyOS\n    comment: machine-id=abc\n    //linux-cachyos\n        cmdline: quiet splash\n')


class LimineRenderTests(unittest.TestCase):
    def test_appearance_merges_into_the_global_section_only_and_is_repeatable(self):
        merged = configure.render_limine_appearance(MACHINE, FRAGMENT)
        head, _, entries = merged.partition('/+CachyOS')
        self.assertEqual(head.count('timeout:'), 1)
        self.assertIn('timeout: 2', head)
        self.assertEqual(head.count('wallpaper:'), 1)
        self.assertIn('remember_last_entry: yes', head)
        self.assertIn('interface_branding:', head)
        self.assertEqual('/+CachyOS' + entries, MACHINE[MACHINE.index('/+CachyOS'):])
        self.assertEqual(configure.render_limine_appearance(merged, FRAGMENT), merged)
        with self.assertRaises(ValueError):
            configure.render_limine_appearance(MACHINE, '/evil\n')

    def test_cmdline_edits_the_last_default_assignment(self):
        original = 'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet lsm=yama"\nKERNEL_CMDLINE["linux-lts"]="quiet"\n'
        rendered = configure.render_limine_cmdline(original, 'set', ['lsm=landlock,apparmor'])
        self.assertEqual(rendered, 'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet lsm=landlock,apparmor"\nKERNEL_CMDLINE["linux-lts"]="quiet"\n')
        self.assertEqual(configure.render_limine_cmdline(rendered, 'add', ['splash']),
                         'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet lsm=landlock,apparmor splash"\nKERNEL_CMDLINE["linux-lts"]="quiet"\n')
        self.assertEqual(configure.render_limine_cmdline('KERNEL_CMDLINE[default]+=" splash rhgb"\n', 'remove', ['splash', 'rhgb']),
                         'KERNEL_CMDLINE[default]+=""\n')
        for text in ('BOOT_ORDER="*"\n', 'KERNEL_CMDLINE[default]="quiet\n'):
            with self.assertRaises(ValueError):
                configure.render_limine_cmdline(text, 'add', ['splash'])


class LimineScriptTests(unittest.TestCase):
    """limine.sh installs the wallpaper and merges the look, with a backup, from a fake root."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        script = (TOOLS / 'limine.sh').read_text().replace('/etc/default/limine', str(self.root) + '/etc/default/limine')
        self.garuda = 'timeout: 2\nwallpaper: boot():/dotfiles-splash.png\nterm_foreground_bright: 02ccfc\n'
        (self.tools / 'limine.sh').write_text(script)
        shutil.copy2(TOOLS / 'configure.py', self.tools / 'configure.py')
        self.boot = self.root / 'boot'
        self.boot.mkdir()
        self.conf = self.boot / 'limine.conf'
        self.conf.write_text(MACHINE)
        (self.root / 'etc/default').mkdir(parents=True)
        (self.root / 'etc/default/limine').write_text(f'ESP_PATH="{self.boot}"\n')
        self.source = self.root / 'dotfiles'
        for look, fragment, image in (('cachyos', FRAGMENT, b'png'), ('garuda', self.garuda, b'garuda')):
            (self.source / 'limine/themes' / look).mkdir(parents=True)
            (self.source / 'limine/themes' / look / 'limine.conf').write_text(fragment)
            (self.source / 'limine/themes' / look / 'splash.png').write_bytes(image)
        (self.tools / 'common.sh').write_text(f'''commander_init() {{ DTYPE=arch; PACKAGER=pacman; ESCALATION_TOOL=env; }}
checkCommandRequirements() {{ :; }}
command_exists() {{ command -v "$1" >/dev/null 2>&1; }}
confirm() {{ :; }}
install_packages() {{ :; }}
backup_system() {{ if [ -e "$1" ]; then backup="$1.backup"; cp -a "$1" "$backup"; fi; }}
fetch_source() {{ source_dir="{self.source}"; }}
''')

    def run_script(self, *args):
        return subprocess.run(['sh', '-e', './limine.sh', *args], cwd=self.tools, text=True, capture_output=True)

    def test_look_and_wallpaper_are_installed_with_a_backup(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.conf.read_text(), configure.render_limine_appearance(MACHINE, FRAGMENT))
        self.assertEqual(self.conf.with_name('limine.conf.backup').read_text(), MACHINE)
        self.assertEqual((self.boot / 'dotfiles-splash.png').read_bytes(), b'png')

    def test_the_garuda_look_uses_its_own_files(self):
        result = self.run_script('garuda')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.conf.read_text(), configure.render_limine_appearance(MACHINE, self.garuda))
        self.assertEqual((self.boot / 'dotfiles-splash.png').read_bytes(), b'garuda')
        self.assertIn('Garuda dr460nized', result.stdout)
        self.assertNotEqual(self.run_script('neon').returncode, 0)

    def test_without_limine_conf_nothing_changes(self):
        self.conf.unlink()
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not seem to boot with Limine', result.stderr)
        self.assertFalse((self.boot / 'dotfiles-splash.png').exists())


if __name__ == '__main__':
    unittest.main()
