import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('validation_record', ROOT / 'scripts/validation-record.py')
validation_record = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validation_record)


class ValidationRecordTest(unittest.TestCase):
    def test_only_successful_runs_of_catalog_entries_are_recorded(self):
        runs = [
            {'entries': ['htop', 'ripgrep'], 'success': True, 'result': 'exit 0, verified'},
            {'entries': ['system-report'], 'success': True, 'result': 'exit 0'},
            {'entries': ['fzf'], 'success': False, 'result': 'exit 1'},
            {'entries': ['fallback-sample'], 'success': True, 'result': 'exit 0, verified'},
            {'entries': ['htop'], 'success': True, 'result': 'exit 0'},
        ]
        with tempfile.TemporaryDirectory() as directory:
            history = Path(directory) / 'history.json'
            history.write_text(json.dumps({'data': {'runs': runs}}))
            found = validation_record.record('archlinux:latest', history, ROOT / 'catalog')
        self.assertEqual(found['image'], 'archlinux:latest')
        # A later run without verification does not undo an earlier one.
        self.assertEqual(found['entries'], {'htop': 'verified', 'ripgrep': 'verified', 'system-report': 'ran'})

    def test_records_from_each_container_merge_into_one(self):
        with tempfile.TemporaryDirectory() as directory:
            parts = []
            for index, image in enumerate(['fedora:latest', 'archlinux:latest']):
                part = Path(directory) / f'validation-{index}.json'
                part.write_text(json.dumps({'image': image, 'entries': {'htop': 'verified'}}))
                parts.append(part)
            merged = validation_record.merge('42', 'https://example.invalid/runs/42', 'abc', parts)
        self.assertEqual(merged['format'], 1)
        self.assertEqual(merged['run'], 42)
        self.assertEqual(list(merged['images']), ['archlinux:latest', 'fedora:latest'])

    def test_virtual_machines_count_only_their_verified_cases(self):
        with tempfile.TemporaryDirectory() as directory:
            part = Path(directory) / 'validation-vm-arch.json'
            part.write_text(json.dumps({'image': 'vm:arch', 'entries': {'grub-dracula': 'verified'},
                                        'failures': {'sddm': 'no login screen'}}))
            merged = validation_record.merge('7', 'url', 'abc', [part])
        self.assertEqual(merged['images'], {})
        self.assertEqual(merged['vms'], {'arch': {'entries': {'grub-dracula': 'verified'}}})

    def test_validation_toml_names_real_entries_and_tests(self):
        claims = tomllib.loads((ROOT / 'catalog/validation.toml').read_text())
        known = validation_record.entry_ids(ROOT / 'catalog')
        named = list(claims['critical']['entries'])
        for group in claims['stand-ins']:
            named.extend(group['entries'])
            self.assertTrue(group['covers'])
            for test in group['tests']:
                self.assertTrue((ROOT / 'tests/installers' / f'{test}.py').is_file(), test)
        for report in claims.get('hardware', []) + claims.get('vm', []):
            named.append(report['entry'])
            for field in ('system', 'date'):
                self.assertTrue(report[field], report)
        for entry in named:
            self.assertIn(entry, known)

    def test_the_record_of_a_release_is_never_committed(self):
        ignored = (ROOT / '.gitignore').read_text().splitlines()
        self.assertIn('/catalog/validation-results.json', ignored)


if __name__ == '__main__':
    unittest.main()
