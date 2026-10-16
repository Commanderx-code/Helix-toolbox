import importlib.util
import json
from pathlib import Path
import tempfile
try:
    import tomllib
except ModuleNotFoundError:  # Python before 3.11: the same parser, as tomli
    import tomli as tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('configure', ROOT / 'scripts/installers/commander/configure.py')
configure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(configure)


class ConfigurationTests(unittest.TestCase):
    def test_grub_preserves_boot_settings_and_is_repeatable(self):
        original = 'GRUB_CMDLINE_LINUX="root=UUID=target security=selinux"\nGRUB_TIMEOUT=7\nGRUB_THEME="old"\nGRUB_BACKGROUND="old"\n'
        rendered = configure.render_grub(original)
        self.assertIn('root=UUID=target security=selinux', rendered)
        self.assertIn('GRUB_TIMEOUT=7', rendered)
        self.assertEqual(rendered.count('GRUB_THEME='), 1)
        self.assertEqual(configure.render_grub(rendered), rendered)

    def test_topgrade_has_no_machine_paths_and_parses_on_both_distros(self):
        template = '''[misc]
assume_yes = true
[linux]
arch_package_manager = "garuda_update"
home_manager_arguments = ["--flake", @HOME_MANAGER_FLAKE@]
[git]
repos = [@DOTFILES_DIRECTORY@]
'''
        for distro in ('garuda', 'fedora', 'ubuntu'):
            parsed = tomllib.loads(configure.render_topgrade(template, distro))
            self.assertEqual(parsed['linux']['arch_package_manager'], 'garuda_update' if distro == 'garuda' else 'autodetect')
            self.assertFalse(parsed['misc']['assume_yes'])
            self.assertEqual(parsed['git']['repos'], [])
            self.assertNotIn('home_manager_arguments', parsed['linux'])

    def test_sddm_preserves_unrelated_settings_and_environment(self):
        rendered = configure.render_sddm('[Autologin]\nUser=test\n[Theme]\nCurrent=old\n[General]\nGreeterEnvironment=FOO=bar,QT_IM_MODULE=old\n')
        self.assertIn('User=test', rendered)
        self.assertIn('FOO=bar', rendered)
        self.assertIn('Current=silent', rendered)
        self.assertIn('QML2_IMPORT_PATH=/usr/share/sddm/themes/silent/components/', rendered)
        self.assertEqual(rendered.count('QT_IM_MODULE='), 1)
        self.assertEqual(configure.render_sddm(rendered), rendered)

    def test_ghostty_preset_and_palette_files_follow_rice_json(self):
        palette = {'name': 'Nord', 'colors': {'base': '#2e3440', 'mantle': '#272c36', 'text': '#eceff4'},
                   'terminal': {'ansi': [f'#{index:06x}' for index in range(16)], 'cursor': '#d8dee9',
                                'cursorText': '#2e3440', 'selection': '#434c5e', 'selectionText': '#eceff4'}}
        with tempfile.TemporaryDirectory() as tmp:
            repo, config = Path(tmp) / 'repo', Path(tmp) / 'config'
            (repo / 'configs/ghostty').mkdir(parents=True)
            (repo / 'configs/themes').mkdir()
            (repo / 'home-manager').mkdir()
            (repo / 'configs/ghostty/config').write_text('theme = commander\n')
            (repo / 'configs/ghostty/spotatui.conf').write_text('spotify\n')
            (repo / 'configs/themes/nord.json').write_text(json.dumps(palette))
            (repo / 'home-manager/rice.json').write_text('{"theme": "nord"}')
            (config / 'ghostty').mkdir(parents=True)
            (config / 'ghostty/config').write_text('personal')
            configure.install_dotfiles(repo, 'ghostty', config, Path(tmp) / 'data')
            self.assertEqual((config / 'ghostty/config').read_text(), 'theme = commander\n')
            self.assertEqual(next((config / 'ghostty').glob('config.commander-backup-*')).read_text(), 'personal')
            theme = (config / 'ghostty/themes/commander').read_text()
            self.assertIn('palette = 0=#000000', theme)
            self.assertIn('palette = 15=#00000f', theme)
            self.assertIn('background = #2e3440', theme)
            self.assertIn('selection-foreground = #eceff4', theme)
            self.assertIn('background-color: #272c36', (config / 'ghostty/topbar.css').read_text())
            self.assertEqual((config / 'ghostty/spotatui.conf').read_text(), 'spotify\n')
            (repo / 'home-manager/rice.json').write_text('{"theme": "../etc"}')
            with self.assertRaises(ValueError):
                configure.load_palette(repo)

    def test_existing_file_is_backed_up(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / 'config'
            target.write_text('old')
            configure.deploy(None, target, 'new')
            self.assertEqual(target.read_text(), 'new')
            self.assertEqual(next(Path(tmp).glob('config.commander-backup-*')).read_text(), 'old')

    def test_copy_failure_restores_original(self):
        with tempfile.TemporaryDirectory() as tmp:
            target = Path(tmp) / 'config'
            target.write_text('old')
            with patch.object(configure.shutil, 'copy2', side_effect=OSError('copy failed')):
                with self.assertRaises(OSError):
                    configure.deploy(Path(tmp) / 'missing', target)
            self.assertEqual(target.read_text(), 'old')

    def test_symlink_and_symlink_parent_are_not_overwritten(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            real = base / 'managed'
            real.mkdir()
            (real / 'config').write_text('managed')
            link = base / 'link'
            link.symlink_to(real, target_is_directory=True)
            for target in (link, link / 'config'):
                with self.assertRaises(ValueError):
                    configure.deploy(None, target, 'bad')
            self.assertEqual((real / 'config').read_text(), 'managed')

    def test_nala_alias_preserves_existing_config_and_is_idempotent(self):
        for shell in ('bash', 'zsh', 'fish'):
            original = '# personal settings\n'
            rendered = configure.render_nala_alias(original, shell)
            self.assertIn(original, rendered)
            self.assertEqual(rendered.count(configure.NALA_START), 1)
            self.assertEqual(configure.render_nala_alias(rendered, shell), rendered)
        with self.assertRaises(ValueError):
            configure.render_nala_alias(configure.NALA_START, 'bash')

    def test_nala_respects_shell_config_locations(self):
        home, config = Path('/example/home'), Path('/example/config')
        self.assertEqual(configure.nala_alias_target(home, config, 'bash'), home / '.bashrc')
        self.assertEqual(configure.nala_alias_target(home, config, 'fish'), config / 'fish/conf.d/zz-commander-nala.fish')
        with patch.dict(configure.os.environ, {'ZDOTDIR': '/example/zsh'}):
            self.assertEqual(configure.nala_alias_target(home, config, 'zsh'), Path('/example/zsh/.zshrc'))
        with self.assertRaises(ValueError):
            configure.nala_alias_target(home, config, 'unsupported')


if __name__ == '__main__':
    unittest.main()
