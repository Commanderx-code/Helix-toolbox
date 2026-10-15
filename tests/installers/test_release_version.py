import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('release_version', ROOT / 'scripts/release-version.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseVersionTests(unittest.TestCase):
    def test_the_first_release_is_the_workspace_version(self):
        self.assertEqual(release.next_version('0.1.0', []), '0.1.0')
        # The commit-named releases from before version tags are ignored.
        self.assertEqual(release.next_version('0.1.0', ['helixforge-134ad752cc82']), '0.1.0')

    def test_each_release_adds_a_patch(self):
        self.assertEqual(release.next_version('0.1.0', ['v0.1.0']), '0.1.1')
        self.assertEqual(release.next_version('0.1.0', ['v0.1.9', 'v0.1.10', 'v0.1.2']), '0.1.11')

    def test_a_higher_workspace_version_starts_the_next_series(self):
        self.assertEqual(release.next_version('0.2.0', ['v0.1.14']), '0.2.0')
        self.assertEqual(release.next_version('1.0.0', ['v0.2.3']), '1.0.0')
        # A lower one does not go backwards.
        self.assertEqual(release.next_version('0.1.0', ['v0.2.3']), '0.2.4')

    def test_a_commit_with_a_version_keeps_it_unless_a_commit_is_added(self):
        tags = [('v0.1.0', 'aaa'), ('v0.1.1', 'bbb'), ('helixforge-bbb', 'bbb')]
        self.assertEqual(release.release_for('bbb', tags, '0.1.0'), ('0.1.1', True))
        self.assertEqual(release.release_for('ccc', tags, '0.1.0'), ('0.1.2', False))
        self.assertEqual(release.release_for('bbb', tags, '0.1.0', changed=True), ('0.1.2', False))

    def test_the_workspace_version_must_be_semantic(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'Cargo.toml').write_text('[workspace.package]\nversion = "0.1"\n')
            with self.assertRaises(ValueError):
                release.workspace_version(Path(tmp))
        self.assertEqual(release.workspace_version(), '0.1.0')
        self.assertIsNone(release.parse('v1.2'))
        self.assertIsNone(release.parse('v1.2.3-rc1'))


if __name__ == '__main__':
    unittest.main()
