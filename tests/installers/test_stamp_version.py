import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('stamp_version', ROOT / 'scripts/stamp-version.py')
stamp_version = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stamp_version)


class StampVersionTest(unittest.TestCase):
    def test_the_workspace_and_its_crates_get_the_release_version(self):
        text = (ROOT / 'Cargo.toml').read_text()
        stamped = stamp_version.stamp(text, '7.8.9')
        self.assertIn('\nversion = "7.8.9"\n', stamped)
        for crate in ('core', 'catalog', 'platform', 'engine'):
            self.assertRegex(stamped, rf'helixforge-{crate} = {{ path = "[^"]+", version = "=7\.8\.9" }}')
        # Other dependencies keep theirs.
        self.assertIn('clap = { version = "4.5"', stamped)
        self.assertEqual(stamped.count('7.8.9'), 5)

    def test_only_release_versions_are_accepted(self):
        text = (ROOT / 'Cargo.toml').read_text()
        for bad in ('1.0', 'v1.0.0', '1.0.0-rc1', '1.0.0\n[x]'):
            with self.assertRaises(ValueError):
                stamp_version.stamp(text, bad)


if __name__ == '__main__':
    unittest.main()
