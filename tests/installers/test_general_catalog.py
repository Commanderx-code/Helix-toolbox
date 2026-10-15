"""The general catalog carried over from Linutil: every script parses, every
entry points at a script that exists, and the forwarding shims reach the
shared helpers."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
INSTALLERS = ROOT / 'scripts/installers'
CATALOG = ROOT / 'catalog'
GENERAL = [path for path in INSTALLERS.rglob('*.sh') if 'commander' not in path.relative_to(INSTALLERS).parts]


def interpreter(path):
    first = path.read_text(errors='replace').splitlines()[:1]
    if first and first[0].startswith('#!'):
        words = first[0][2:].split()
        program = words[-1] if words and words[0].endswith('/env') else (words[0] if words else 'sh')
        return 'bash' if 'bash' in program else 'sh'
    return 'sh'


class GeneralCatalogTests(unittest.TestCase):
    def test_every_script_parses_with_its_interpreter(self):
        self.assertGreater(len(GENERAL), 150)
        for path in GENERAL:
            with self.subTest(script=str(path.relative_to(ROOT))):
                result = subprocess.run([interpreter(path), '-n', str(path)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)

    def test_every_script_entry_names_an_existing_script(self):
        count = 0
        for path in CATALOG.rglob('*.toml'):
            if path.name in ('tab.toml', 'group.toml', 'tabs.toml'):
                continue
            data = tomllib.loads(path.read_text())
            script = data.get('script', {})
            if 'id' in script:
                count += 1
                with self.subTest(entry=str(path.relative_to(ROOT))):
                    self.assertTrue((INSTALLERS / f'{script["id"]}.sh').is_file(), script['id'])
                    self.assertNotIn('..', script['id'])
        self.assertGreater(count, 150)

    def test_converted_entries_are_package_data(self):
        # Installers that were one package per manager are catalog data now.
        packages = 0
        for path in CATALOG.rglob('*.toml'):
            if path.name in ('tab.toml', 'group.toml', 'tabs.toml'):
                continue
            data = tomllib.loads(path.read_text())
            if 'package' in data:
                packages += 1
                package = data['package']
                with self.subTest(entry=str(path.relative_to(ROOT))):
                    self.assertTrue(any(key in package for key in ('arch', 'debian', 'fedora', 'suse', 'alpine', 'void', 'solus', 'aur', 'flatpak')))
                    self.assertNotIn('script', data)
        self.assertGreaterEqual(packages, 44)

    def test_entry_ids_are_unique(self):
        stems = [path.stem for path in CATALOG.rglob('*.toml') if path.name not in ('tab.toml', 'group.toml', 'tabs.toml')]
        duplicates = sorted({stem for stem in stems if stems.count(stem) > 1})
        self.assertEqual(duplicates, [])

    def test_shims_forward_to_the_shared_helpers(self):
        with tempfile.TemporaryDirectory() as tmp:
            script = Path(tmp) / 'probe.sh'
            script.write_text(f'. {INSTALLERS / "common-script.sh"}\ncommand_exists sh && printf "%s\\n" "$GREEN"\n')
            result = subprocess.run(['sh', '-e', str(script)], capture_output=True, text=True,
                                    env=dict(os.environ, HELIXFORGE_LIB=str(ROOT / 'scripts/shared')))
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout, '\\033[32m\n')

    def test_scripts_only_source_files_inside_the_installers(self):
        # Entry scripts source helpers relative to their own directory, which is
        # where the engine runs them. Shared libraries such as utility_functions.sh
        # are sourced from their callers' directories, and system files by
        # absolute path; neither is checked here.
        pattern = re.compile(r'^\s*\.\s+(\S+)', re.M)
        libraries = {'utility_functions.sh', 'common-script.sh', 'common-service-script.sh', 'common-steam-script.sh'}
        for path in GENERAL:
            if path.name in libraries:
                continue
            for target in pattern.findall(path.read_text(errors='replace')):
                if '$' in target or target.startswith('/'):
                    continue
                with self.subTest(script=str(path.relative_to(ROOT)), target=target):
                    resolved = (path.parent / target).resolve()
                    self.assertTrue(resolved.is_file(), resolved)
                    self.assertTrue(resolved.is_relative_to(INSTALLERS.resolve()), resolved)


if __name__ == '__main__':
    unittest.main()
