"""The dotfiles preview against what the installer then writes."""
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('configure', ROOT / 'scripts/installers/commander/configure.py')
configure = importlib.util.module_from_spec(spec)
spec.loader.exec_module(configure)

COMPONENTS = ('nvim', 'fastfetch', 'starship', 'konsole', 'ghostty')


def make_repo(repo):
    """A dotfiles checkout with every component the installers read."""
    files = {
        'configs/nvim/init.lua': 'require("commander")\n',
        'configs/nvim/lua/commander/init.lua': 'vim.opt.number = true\n',
        'configs/fastfetch/config.jsonc': json.dumps({'logo': {'type': 'kitty'}, 'modules': ['os']}),
        'configs/starship/starship.toml': 'add_newline = false\n',
        'configs/konsole/Garuda.profile': '[General]\nName=Garuda\n',
        'configs/konsole/Sweet.colorscheme': '[Background]\nColor=22,25,37\n',
        'configs/ghostty/config': 'theme = commander\n',
        'configs/ghostty/spotatui.conf': 'spotify\n',
    }
    for name, text in files.items():
        (repo / name).parent.mkdir(parents=True, exist_ok=True)
        (repo / name).write_text(text)
    (repo / 'configs/fastfetch/visuals').mkdir()
    (repo / 'configs/fastfetch/visuals/dr460nized-fastfetch.png').write_bytes(b'\x89PNG\r\n\x1a\n\xff\xfe')
    palette = {'terminal': {'ansi': ['#000000'] * 16, 'cursor': '#ffffff', 'cursorText': '#000000',
                            'selection': '#333333', 'selectionText': '#ffffff'},
               'colors': {'base': '#111111', 'text': '#eeeeee', 'mantle': '#222222'}}
    (repo / 'configs/themes').mkdir()
    (repo / 'configs/themes/eldritch.json').write_text(json.dumps(palette))


class DotfilesPreviewTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        base = Path(self.temporary.name)
        self.repo, self.home = base / 'dotfiles', base / 'home'
        self.config, self.data = self.home / '.config', self.home / '.local/share'
        make_repo(self.repo)
        self.home.mkdir()
        environment = patch.dict(os.environ, {'HOME': str(self.home), 'SHELL': '/bin/bash',
                                              'XDG_CONFIG_HOME': str(self.config), 'XDG_DATA_HOME': str(self.data)})
        environment.start()
        self.addCleanup(environment.stop)

    def tearDown(self):
        self.temporary.cleanup()

    def preview(self, component):
        return configure.preview_dotfiles(self.repo, component, self.config, self.data)

    def install(self, component):
        with patch('builtins.print'):
            configure.install_dotfiles(self.repo, component, self.config, self.data)

    def written(self):
        """Every file in the home folder except backups."""
        return {path for path in self.home.rglob('*')
                if path.is_file() and '.commander-backup-' not in str(path)}

    def test_the_installer_writes_exactly_what_the_preview_lists(self):
        for component in COMPONENTS:
            with self.subTest(component):
                before = self.written()
                shown = self.preview(component)
                self.assertTrue(all(file['state'] == 'new' for file in shown['files']), shown)
                self.install(component)
                self.assertEqual(self.written() - before, {Path(file['path']) for file in shown['files']})
                for file in shown['files']:
                    path = Path(file['path'])
                    if file['text'] is None:
                        self.assertTrue(path.suffix == '.png', path)
                    else:
                        self.assertEqual(path.read_text(), file['text'], path)

    def test_after_an_install_nothing_changes(self):
        for component in COMPONENTS:
            with self.subTest(component):
                self.install(component)
                self.assertEqual({file['state'] for file in self.preview(component)['files']}, {'same'})

    def test_changed_and_left_behind_files_are_shown(self):
        self.install('nvim')
        nvim = self.config / 'nvim'
        (nvim / 'init.lua').write_text('-- my own\n')
        (nvim / 'local.lua').write_text('-- only here\n')
        shown = {Path(file['path']).relative_to(nvim).as_posix(): file for file in self.preview('nvim')['files']}
        self.assertEqual(shown['init.lua']['state'], 'changed')
        self.assertEqual(shown['init.lua']['text'], 'require("commander")\n')
        self.assertEqual(shown['lua/commander/init.lua']['state'], 'same')
        self.assertEqual(shown['local.lua']['state'], 'removed')
        self.assertEqual(self.preview('nvim')['folders'], [str(nvim)])

    def test_files_another_tool_manages_are_refused_as_the_installer_does(self):
        self.config.mkdir(parents=True)
        (self.home / 'managed.toml').write_text('')
        (self.config / 'starship.toml').symlink_to(self.home / 'managed.toml')
        with self.assertRaisesRegex(ValueError, 'symlink-managed'):
            self.preview('starship')
        with self.assertRaisesRegex(ValueError, 'symlink-managed'):
            self.install('starship')

    def test_the_preview_writes_nothing(self):
        for component in COMPONENTS:
            self.preview(component)
        self.assertEqual(self.written(), set())
        self.assertFalse(self.config.exists())

    def test_the_command_prints_json(self):
        import subprocess
        result = subprocess.run(['python3', '-B', './configure.py', 'dotfiles-preview', str(self.repo), 'starship'],
                                cwd=ROOT / 'scripts/installers/commander', capture_output=True, text=True, check=True)
        shown = json.loads(result.stdout)
        self.assertEqual(shown['files'], [{'path': str(self.config / 'starship.toml'), 'state': 'new',
                                           'text': 'add_newline = false\n'}])


if __name__ == '__main__':
    unittest.main()
