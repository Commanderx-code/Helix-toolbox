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


class KernelArgumentTests(unittest.TestCase):
    def test_words_are_added_once_and_other_settings_stay(self):
        original = 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet loglevel=3"\nGRUB_CMDLINE_LINUX="root=UUID=x"\n'
        rendered = configure.render_kernel_cmdline(original, 'add', ['splash'])
        self.assertEqual(rendered, 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet loglevel=3 splash"\nGRUB_CMDLINE_LINUX="root=UUID=x"\n')
        self.assertEqual(configure.render_kernel_cmdline(rendered, 'add', ['splash']), rendered)

    def test_words_are_removed_and_the_line_is_created_when_missing(self):
        rendered = configure.render_kernel_cmdline('GRUB_CMDLINE_LINUX_DEFAULT="quiet splash rhgb"\n', 'remove', ['splash', 'rhgb'])
        self.assertEqual(rendered, 'GRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.assertEqual(configure.render_kernel_cmdline('GRUB_TIMEOUT=5\n', 'add', ['splash']),
                         'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="splash"\n')
        self.assertEqual(configure.render_kernel_cmdline('', 'remove', ['splash']), 'GRUB_CMDLINE_LINUX_DEFAULT=""\n')

    def test_the_last_assignment_and_its_export_prefix_are_kept(self):
        original = 'GRUB_CMDLINE_LINUX_DEFAULT="old"\nexport GRUB_CMDLINE_LINUX_DEFAULT=\'quiet\'  # trailing\n'
        rendered = configure.render_kernel_cmdline(original, 'add', ['splash'])
        self.assertEqual(rendered, 'GRUB_CMDLINE_LINUX_DEFAULT="old"\nexport GRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')

    def test_set_replaces_words_with_the_same_key(self):
        original = 'GRUB_CMDLINE_LINUX_DEFAULT="quiet lsm=landlock,yama splash"\n'
        rendered = configure.render_kernel_cmdline(original, 'set', ['lsm=landlock,lockdown,yama,integrity,apparmor,bpf'])
        self.assertEqual(rendered, 'GRUB_CMDLINE_LINUX_DEFAULT="quiet splash lsm=landlock,lockdown,yama,integrity,apparmor,bpf"\n')
        self.assertEqual(configure.render_kernel_cmdline(rendered, 'set', ['lsm=landlock,lockdown,yama,integrity,apparmor,bpf']), rendered)
        self.assertEqual(configure.render_kernel_cmdline('GRUB_CMDLINE_LINUX_DEFAULT="lsm"\n', 'set', ['lsm=apparmor']),
                         'GRUB_CMDLINE_LINUX_DEFAULT="lsm lsm=apparmor"\n')

    def test_unknown_actions_and_broken_quoting_are_refused(self):
        with self.assertRaises(ValueError):
            configure.render_kernel_cmdline('', 'toggle', ['splash'])
        with self.assertRaises(ValueError):
            configure.render_kernel_cmdline('GRUB_CMDLINE_LINUX_DEFAULT="quiet\n', 'add', ['splash'])


class MkinitcpioHookTests(unittest.TestCase):
    def test_plymouth_follows_udev_or_systemd_and_is_added_once(self):
        rendered = configure.render_mkinitcpio_hooks('# hooks\nHOOKS=(base udev autodetect encrypt filesystems)\n', 'add')
        self.assertEqual(rendered, '# hooks\nHOOKS=(base udev plymouth autodetect encrypt filesystems)\n')
        self.assertEqual(configure.render_mkinitcpio_hooks(rendered, 'add'), rendered)
        self.assertEqual(configure.render_mkinitcpio_hooks('HOOKS="base systemd autodetect"\n', 'add'),
                         'HOOKS=(base systemd plymouth autodetect)\n')
        self.assertEqual(configure.render_mkinitcpio_hooks('HOOKS=(keyboard)\n', 'add'), 'HOOKS=(plymouth keyboard)\n')

    def test_removal_and_missing_hooks(self):
        self.assertEqual(configure.render_mkinitcpio_hooks('HOOKS=(base udev plymouth autodetect)\n', 'remove'),
                         'HOOKS=(base udev autodetect)\n')
        with self.assertRaises(ValueError):
            configure.render_mkinitcpio_hooks('#HOOKS=(base udev)\nMODULES=()\n', 'add')


class BootSplashScriptTests(unittest.TestCase):
    """plymouth.sh runs against a fake root with the same transaction safety as the GRUB themes."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.defaults = self.root / 'etc/default/grub'
        self.defaults.parent.mkdir(parents=True)
        self.defaults.write_text('GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.boot = self.root / 'boot/grub'
        self.boot.mkdir(parents=True)
        self.menu = self.boot / 'grub.cfg'
        self.menu.write_text('original boot menu')
        self.hooks = self.root / 'etc/mkinitcpio.conf'
        self.hooks.write_text('HOOKS=(base udev autodetect filesystems)\n')
        self.log = self.root / 'calls.log'
        self.themes = self.root / 'usr/share/plymouth/themes'
        self.themes.mkdir(parents=True)
        for name in ('plymouth.sh', 'kernel-cmdline.sh'):
            script = (TOOLS / name).read_text()
            for path in ('/etc/default/grub', '/etc/default/limine', '/boot/grub', '/etc/mkinitcpio.conf', '/usr/share/plymouth/themes'):
                script = script.replace(path, str(self.root) + path)
            (self.root / name).write_text(script)
        self.source = self.root / 'dotfiles'
        (self.source / 'plymouth').mkdir(parents=True)
        (self.source / 'plymouth/theme').write_text('garuda-mac-style-purple\n')
        shutil.copy2(TOOLS / 'configure.py', self.root / 'configure.py')
        self.write_tool('grub-mkconfig', 'printf "new menu" > "$2"')
        self.write_tool('grub-script-check', 'exit 0')
        for tool in ('mkinitcpio', 'dracut-rebuild'):
            self.write_tool(tool, f'echo "{tool} $*" >> "$CALLS"')
        self.write_tool('plymouth-set-default-theme',
                        'if [ "$1" = --list ]; then printf "bgrt\\nspinner\\n"; elif [ $# -eq 0 ]; then echo bgrt; else echo "theme $1" >> "$CALLS"; fi')

    def write_tool(self, name, body):
        path = self.root / name
        path.write_text(f'#!/bin/sh\n{body}\n')
        path.chmod(0o755)

    def run_script(self, action, tools, stdin=''):
        # The fake helpers answer every confirmation and know only the listed tools.
        known = '|'.join(tools + ['git'])
        (self.root / 'common.sh').write_text(f'''commander_init() {{ DTYPE=arch; PACKAGER=pacman; ESCALATION_TOOL=env; }}
checkCommandRequirements() {{ :; }}
command_exists() {{ case "$1" in {known}) return 0 ;; *) return 1 ;; esac; }}
confirm() {{ :; }}
install_packages() {{ echo "install $*" >> "$CALLS"; }}
backup_system() {{ backup="$1.backup"; cp -a "$1" "$backup"; }}
fetch_source() {{ source_dir="{self.source}"; }}
''')
        return subprocess.run(['sh', '-e', './plymouth.sh', action], cwd=self.root, input=stdin,
                              env=dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'], CALLS=str(self.log)),
                              text=True, capture_output=True)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_enable_adds_the_hook_and_argument_then_rebuilds(self):
        result = self.run_script('on', ['mkinitcpio', 'grub-mkconfig'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')
        self.assertEqual(self.hooks.read_text(), 'HOOKS=(base udev plymouth autodetect filesystems)\n')
        self.assertEqual(self.menu.read_text(), 'new menu')
        self.assertEqual(self.calls(), ['install plymouth', 'mkinitcpio -P'])
        self.assertTrue(self.defaults.with_name('grub.backup').is_file())
        self.assertTrue(self.hooks.with_name('mkinitcpio.conf.backup').is_file())

    def test_disable_removes_the_arguments_and_keeps_the_hook(self):
        self.defaults.write_text('GRUB_CMDLINE_LINUX_DEFAULT="quiet splash rhgb"\n')
        self.hooks.write_text('HOOKS=(base udev plymouth autodetect)\n')
        result = self.run_script('off', ['dracut-rebuild', 'mkinitcpio', 'grub-mkconfig'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.defaults.read_text(), 'GRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.assertEqual(self.hooks.read_text(), 'HOOKS=(base udev plymouth autodetect)\n')
        self.assertEqual(self.menu.read_text(), 'new menu')
        self.assertEqual(self.calls(), [])

    def test_failed_generation_restores_the_defaults_and_menu(self):
        self.write_tool('grub-mkconfig', 'exit 1')
        result = self.run_script('on', ['dracut-rebuild', 'grub-mkconfig'])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.assertEqual(self.menu.read_text(), 'original boot menu')
        self.assertEqual(list(self.boot.glob('.commander-grub.*')), [])
        self.assertNotIn('dracut-rebuild', self.calls())

    def test_theme_choice_is_applied_with_the_distro_rebuild_tool(self):
        result = self.run_script('theme', ['dracut-rebuild', 'mkinitcpio'], stdin='2\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ['theme spinner', 'dracut-rebuild '])
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        result = self.run_script('theme', ['dracut-rebuild'], stdin='9\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), ['theme spinner', 'dracut-rebuild '])

    def test_dotfiles_theme_is_selected_when_installed(self):
        (self.themes / 'garuda-mac-style-purple').mkdir()
        (self.themes / 'garuda-mac-style-purple/garuda-mac-style-purple.plymouth').write_text('[Plymouth Theme]')
        result = self.run_script('dotfiles', ['dracut-rebuild'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ['theme garuda-mac-style-purple', 'dracut-rebuild '])

    def test_dotfiles_theme_is_copied_from_the_repository(self):
        vendored = self.source / 'plymouth/themes/garuda-mac-style-purple'
        vendored.mkdir(parents=True)
        (vendored / 'garuda-mac-style-purple.plymouth').write_text('[Plymouth Theme]')
        (vendored / 'frame.png').write_bytes(b'png')
        result = self.run_script('dotfiles', ['mkinitcpio'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.themes / 'garuda-mac-style-purple/frame.png').read_bytes(), b'png')
        self.assertEqual(self.calls(), ['theme garuda-mac-style-purple', 'mkinitcpio -P'])

    def test_dotfiles_theme_comes_from_a_package_or_is_explained(self):
        self.write_tool('pacman', 'exit 0')
        result = self.run_script('dotfiles', ['dracut-rebuild'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ['install plymouth-theme-garuda-mac-style-purple', 'theme garuda-mac-style-purple', 'dracut-rebuild '])
        self.log.unlink()
        self.write_tool('pacman', 'exit 1')
        result = self.run_script('dotfiles', ['dracut-rebuild'])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Get New', result.stderr)
        self.assertEqual(self.calls(), [])

    def test_dotfiles_theme_already_current_changes_nothing(self):
        (self.source / 'plymouth/theme').write_text('bgrt\n')
        (self.themes / 'bgrt').mkdir()
        (self.themes / 'bgrt/bgrt.plymouth').write_text('[Plymouth Theme]')
        result = self.run_script('dotfiles', ['dracut-rebuild'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn('already current', result.stdout)
        self.assertEqual(self.calls(), [])

    def test_dotfiles_theme_name_is_validated(self):
        (self.source / 'plymouth/theme').write_text('../etc\n')
        result = self.run_script('dotfiles', ['dracut-rebuild'])
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [])

    def test_limine_entries_get_the_argument_through_the_entry_tool(self):
        # /etc/default/grub stays in place: the grub package leaves it behind on Limine machines.
        limine = self.root / 'etc/default/limine'
        limine.write_text('ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet"\n')
        self.write_tool('limine-mkinitcpio', 'echo "limine-mkinitcpio $*" >> "$CALLS"')
        result = self.run_script('on', ['mkinitcpio', 'limine-mkinitcpio'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(limine.read_text(), 'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet splash"\n')
        self.assertEqual(self.hooks.read_text(), 'HOOKS=(base udev plymouth autodetect filesystems)\n')
        self.assertTrue(limine.with_name('limine.backup').is_file())
        # The entry tool rebuilt the boot images; mkinitcpio is not run a second time.
        self.assertEqual(self.calls(), ['install plymouth', 'limine-mkinitcpio '])
        self.assertEqual(self.menu.read_text(), 'original boot menu')
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.log.unlink()
        result = self.run_script('off', ['mkinitcpio', 'limine-mkinitcpio'])
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(limine.read_text(), 'ESP_PATH="/boot"\nKERNEL_CMDLINE[default]="quiet"\n')
        self.assertEqual(self.calls(), ['limine-mkinitcpio '])

    def test_limine_without_the_entry_tool_or_its_cmdline_line_changes_nothing(self):
        self.defaults.unlink()
        limine = self.root / 'etc/default/limine'
        limine.write_text('ESP_PATH="/boot"\n')
        result = self.run_script('on', ['mkinitcpio'])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('entry tool', result.stderr)
        self.write_tool('limine-mkinitcpio', 'echo "limine-mkinitcpio $*" >> "$CALLS"')
        result = self.run_script('on', ['mkinitcpio', 'limine-mkinitcpio'])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('KERNEL_CMDLINE[default]', result.stderr)
        self.assertEqual(limine.read_text(), 'ESP_PATH="/boot"\n')
        self.assertNotIn('limine-mkinitcpio ', self.calls())

    def test_without_an_initramfs_tool_nothing_changes(self):
        result = self.run_script('on', ['grub-mkconfig'])
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('No initramfs tool', result.stderr)
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet"\n')
        self.assertEqual(self.calls(), [])


if __name__ == '__main__':
    unittest.main()
