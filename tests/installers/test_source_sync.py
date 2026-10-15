import copy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'scripts/installers/commander'))
import catalog

spec = importlib.util.spec_from_file_location('source_sync', ROOT / 'scripts/sync-sources.py')
sync = importlib.util.module_from_spec(spec)
spec.loader.exec_module(sync)


def exported_tool():
    return {'id': 'example-tool', 'name': 'Example tool', 'description': 'Install example settings.',
            'type': 'config', 'packages': {'apt-get': ['example'], 'dnf': ['example'], 'pacman': ['example']},
            'files': [{'source': 'configs/example/config.toml', 'target': 'example/config.toml'}]}


class SourceSyncTests(unittest.TestCase):
    def request(self, runs):
        return lambda path: {'sha': 'a' * 40} if path.endswith('/commits/main') else {'workflow_runs': runs}

    def run_record(self, **values):
        return dict({'id': 1, 'run_attempt': 1, 'head_sha': 'a' * 40, 'event': 'push',
                     'head_branch': 'main', 'head_repository': {'full_name': 'Commanderx-code/dotfiles'},
                     'status': 'completed', 'conclusion': 'success'}, **values)

    def test_only_successful_latest_main_revision_is_eligible(self):
        good = self.run_record()
        self.assertEqual(sync.eligible_revision('dotfiles', self.request([good])), 'a' * 40)
        for bad in ([], [self.run_record(conclusion='failure')],
                    [self.run_record(status='in_progress', conclusion=None)],
                    [self.run_record(head_sha='b' * 40)], [self.run_record(event='pull_request')],
                    [self.run_record(head_repository={'full_name': 'other/dotfiles'})],
                    [good, self.run_record(id=2, conclusion='failure')],
                    [good, self.run_record(run_attempt=2, conclusion='failure')]):
            with self.subTest(runs=bad):
                self.assertIsNone(sync.eligible_revision('dotfiles', self.request(bad)))

    def test_dispatched_validation_accepts_bot_commits_and_newer_failure_vetoes(self):
        good = self.run_record(event='workflow_dispatch')
        self.assertEqual(sync.eligible_revision('dotfiles', self.request([good])), 'a' * 40)
        bad = self.run_record(id=2, event='workflow_dispatch', conclusion='failure')
        self.assertIsNone(sync.eligible_revision('dotfiles', self.request([good, bad])))

    def test_source_errors_do_not_fall_back_to_untested_head(self):
        with self.assertRaises(OSError):
            sync.eligible_revision('dotfiles', lambda _: (_ for _ in ()).throw(OSError('offline')))

    def test_new_tool_generates_confirmed_installer_without_hand_editing_menu(self):
        documents = {name: json.loads((sync.INSTALLERS / 'catalogs' / f'{name}.json').read_text()) for name in sync.SOURCES}
        documents['dotfiles']['entries'].append(exported_tool())
        files, wrappers = sync.render(documents)
        entry = files['configurations/dotfiles/dotfiles-example-tool.toml']
        self.assertIn('name = "Example tool"', entry)
        self.assertIn('id = "commander/generated/dotfiles-example-tool"', entry)
        self.assertIn('writes = ["~/.config/example/config.toml"]', entry)
        self.assertIn('id = "commander/dotfiles-nvim"', files['configurations/dotfiles/dotfiles-nvim.toml'])
        self.assertIn('values = ["nix", "git"]', files['applications/terminal-tools/netwatch.toml'])
        self.assertIn('id = "commander/myfish-fish"', files['configurations/myfish/myfish-fish.toml'])
        wrapper = wrappers['dotfiles-example-tool.sh']
        self.assertIn('exec sh -e ./catalog-install.sh dotfiles example-tool', wrapper)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'generated').mkdir()
            path = root / 'generated/entry.sh'
            path.write_text(wrapper)
            (root / 'catalog-install.sh').write_text('#!/bin/sh\nprintf "%s\\n" "$PWD" "$@"\n')
            result = subprocess.run(['sh', '-e', str(path)], cwd=root / 'generated',
                                    capture_output=True, text=True, check=True)
            self.assertEqual(result.stdout.splitlines(), [str(root), 'dotfiles', 'example-tool'])

    def test_rendered_catalog_matches_the_committed_files(self):
        documents = {name: json.loads((sync.INSTALLERS / 'catalogs' / f'{name}.json').read_text()) for name in sync.SOURCES}
        files, _ = sync.render(documents)
        for path, text in files.items():
            self.assertEqual((sync.CATALOG / path).read_text(), text, path)

    def test_catalog_rejects_traversal_package_options_duplicates_and_unknown_handlers(self):
        for mutate in (
            lambda e: e['files'][0].update(source='../secret'),
            lambda e: e['files'][0].update(target='/etc/profile'),
            lambda e: e['files'][0].update(target='../fish/config.fish'),
            lambda e: e['packages'].update(pacman=['--overwrite']),
            lambda e: e.update(id='tool;echo'),
            lambda e: e.update(type='command'),
            lambda e: e['files'].append({'source': 'configs/other', 'target': 'example'}),
        ):
            entry = exported_tool()
            mutate(entry)
            with self.assertRaises(ValueError):
                catalog.validate({'version': 1, 'entries': [entry]}, 'dotfiles')
        entry = exported_tool()
        with self.assertRaises(ValueError):
            catalog.validate({'version': 1, 'entries': [entry, copy.deepcopy(entry)]}, 'dotfiles')
        with self.assertRaises(ValueError):
            catalog.validate({'version': 1, 'entries': [dict(entry, type='builtin', handler='../../evil')]}, 'dotfiles')

    def test_config_updates_backup_files_and_respect_managed_destinations(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            repo, config = base / 'repo', base / 'config'
            source = repo / 'configs/example/config.toml'
            source.parent.mkdir(parents=True)
            source.write_text('updated')
            entry = exported_tool()
            catalog.validate({'version': 1, 'entries': [entry]}, 'dotfiles', repo)
            target = config / 'example/config.toml'
            target.parent.mkdir(parents=True)
            target.write_text('personal')
            catalog.install(entry, repo, config, 'pacman')
            self.assertEqual(target.read_text(), 'updated')
            self.assertEqual(next(target.parent.glob('*.commander-backup-*')).read_text(), 'personal')
            target.unlink()
            target.symlink_to(base / 'missing-nix-store-file')
            with self.assertRaises(ValueError):
                catalog.preflight(entry, repo, config, 'pacman')
            self.assertTrue(target.is_symlink())

    def test_preflight_rejects_missing_sources_and_unsupported_managers_before_writes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for manager in ('unsupported', 'dnf'):
                with self.assertRaises(ValueError):
                    catalog.install(exported_tool(), root, root / 'target', manager)
                self.assertFalse((root / 'target').exists())

    def test_export_cannot_copy_symlinks_out_of_source_tree(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'repo').mkdir()
            (root / 'outside').write_text('not exported')
            (root / 'repo/link').symlink_to(root / 'outside')
            with self.assertRaises(ValueError):
                catalog.source_path(root / 'repo', 'link')

    def test_generated_installer_confirmation_packages_and_managed_file_boundaries(self):
        for scenario in ('install', 'decline', 'managed', 'package-failure'):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                repo, tools, config = root / 'repo', root / 'tools', root / 'config'
                tools.mkdir()
                (tools / 'catalogs').mkdir()
                for name in ('catalog-install.sh', 'catalog.py', 'configure.py'):
                    shutil.copy2(sync.INSTALLERS / name, tools / name)
                (tools / 'catalogs/dotfiles.json').write_text(json.dumps({'version': 1, 'entries': [exported_tool()]}))
                source = repo / 'configs/example/config.toml'
                source.parent.mkdir(parents=True)
                source.write_text('published config')
                target = config / 'example/config.toml'
                if scenario == 'managed':
                    target.parent.mkdir(parents=True)
                    target.symlink_to(root / 'absent-store-path')
                (tools / 'common.sh').write_text('''commander_init() { PACKAGER=pacman; }
checkCommandRequirements() { :; }
confirm() { read -r answer; [ "$answer" = APPLY ] || exit 1; }
command_exists() { command -v "$1" >/dev/null; }
fetch_source() { source_dir="$FIXTURE_REPO"; }
install_packages() { printf '%s\\n' "$@" > "$PACKAGE_LOG"; [ "$FAIL_PACKAGES" = no ]; }
''')
                log = root / 'packages'
                result = subprocess.run(['sh', '-e', './catalog-install.sh', 'dotfiles', 'example-tool'],
                                        cwd=tools, text=True, capture_output=True,
                                        input='no\n' if scenario == 'decline' else 'APPLY\n',
                                        env=dict(os.environ, FIXTURE_REPO=str(repo), XDG_CONFIG_HOME=str(config),
                                                 PACKAGE_LOG=str(log), FAIL_PACKAGES='yes' if scenario == 'package-failure' else 'no'))
                self.assertEqual(result.returncode == 0, scenario == 'install', result.stderr)
                self.assertEqual(log.exists(), scenario in ('install', 'package-failure'))
                if scenario == 'install':
                    self.assertEqual(log.read_text(), 'example\n')
                    self.assertEqual(target.read_text(), 'published config')
                else:
                    self.assertFalse(target.exists())
                    self.assertEqual(target.is_symlink(), scenario == 'managed')

    def test_nix_tools_are_filtered_and_do_not_claim_privileged_installation(self):
        documents = {name: json.loads((sync.INSTALLERS / 'catalogs' / f'{name}.json').read_text()) for name in sync.SOURCES}
        documents['dotfiles']['entries'] = [dict(id='tfm', name='TFM trial', description='Install TFM.',
                                               type='builtin', handler='tfm')]
        files, _ = sync.render(documents)
        entry = files['applications/terminal-tools/tfm.toml']
        self.assertIn('name = "TFM trial"', entry)
        self.assertIn('flags = "FM MP"', entry)
        self.assertIn('check = "command_exists"', entry)
        self.assertIn('values = ["nix", "git"]', entry)

    def test_terminal_tools_confirm_and_preserve_existing_installations(self):
        for scenario in ('install', 'cancel', 'existing', 'wrong-arch', 'missing-nix', 'nix-failure'):
            with self.subTest(scenario=scenario), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                shutil.copy2(sync.INSTALLERS / 'terminal-tool.sh', root / 'terminal-tool.sh')
                (root / 'common.sh').write_text('''
id() { printf '1000\\n'; }
uname() { case "$1" in -s) printf 'Linux\\n';; -m) printf '%s\\n' "$TEST_ARCH";; esac; }
checkCommandRequirements() { [ "$SCENARIO" != missing-nix ] || exit 1; }
command_exists() { [ "$SCENARIO" = existing ]; }
confirm() { read -r answer; [ "$answer" = APPLY ] || exit 1; }
fetch_source() { printf '%s\\n' "$1" > "$FETCH_LOG"; source_dir='/tested/source with spaces'; }
nix() { printf '%s\\n' "$@" > "$NIX_LOG"; [ "$SCENARIO" != nix-failure ]; }
''')
                result = subprocess.run(['sh', '-e', './terminal-tool.sh', 'tfm'], cwd=root,
                                        input='no\n' if scenario == 'cancel' else 'APPLY\n',
                                        capture_output=True, text=True,
                                        env=dict(os.environ, SCENARIO=scenario,
                                                 TEST_ARCH='aarch64' if scenario == 'wrong-arch' else 'x86_64',
                                                 FETCH_LOG=str(root / 'fetch'), NIX_LOG=str(root / 'nix')))
                self.assertEqual(result.returncode == 0, scenario in ('install', 'existing'), result.stderr)
                self.assertEqual((root / 'fetch').exists(), scenario in ('install', 'nix-failure'))
                self.assertEqual((root / 'nix').exists(), scenario in ('install', 'nix-failure'))
                if scenario == 'install':
                    self.assertEqual((root / 'nix').read_text().splitlines(),
                                     ['--extra-experimental-features', 'nix-command flakes', 'profile',
                                      'install', 'path:/tested/source with spaces?dir=home-manager#tfm'])


if __name__ == '__main__':
    unittest.main()
