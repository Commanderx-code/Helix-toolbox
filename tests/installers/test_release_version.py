import importlib.util
from pathlib import Path
import subprocess
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
        # A lower one does not go backward.
        self.assertEqual(release.next_version('0.1.0', ['v0.2.3']), '0.2.4')

    def test_a_commit_with_a_version_keeps_it_unless_a_commit_is_added(self):
        tags = [('v0.1.0', 'aaa'), ('v0.1.1', 'bbb'), ('helixforge-bbb', 'bbb')]
        self.assertEqual(release.release_for('bbb', tags, '0.1.0'), ('0.1.1', True))
        self.assertEqual(release.release_for('ccc', tags, '0.1.0'), ('0.1.2', False))
        self.assertEqual(release.release_for('bbb', tags, '0.1.0', changed=True), ('0.1.2', False))

    def test_only_documentation_since_the_last_release(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)

            def git(*arguments):
                return subprocess.run(['git', '-c', 'user.name=t', '-c', 'user.email=t@t', *arguments],
                                      cwd=root, check=True, capture_output=True, text=True).stdout.strip()

            def commit(path, text):
                (root / path).parent.mkdir(parents=True, exist_ok=True)
                (root / path).write_text(text)
                git('add', '-A')
                git('commit', '-q', '-m', path)
                return git('rev-parse', 'HEAD')

            git('init', '-q')
            first = commit('src/main.rs', 'fn main() {}\n')
            # Before any release, there is nothing to compare with.
            self.assertFalse(release.documentation_only(first, [], root))
            tags = [('v1.0.0', first), ('helixforge-x', first)]
            self.assertTrue(release.documentation_only(first, tags, root))
            commit('README.md', 'banner\n')
            commit('docs/GUIDE.md', 'guide\n')
            readme = commit('assets/banner.png', 'png\n')
            self.assertTrue(release.documentation_only(readme, tags, root))
            code = commit('src/main.rs', 'fn main() { }\n')
            self.assertFalse(release.documentation_only(code, tags, root))
            # Released, then documentation again.
            tags.append(('v1.0.1', code))
            self.assertTrue(release.documentation_only(commit('CHANGELOG.md', 'log\n'), tags, root))
            # A Markdown file below the top is not documentation.
            self.assertFalse(release.documentation_only(commit('catalog/notes.md', 'x\n'), tags, root))
            # A tag on a commit this checkout does not have counts for nothing.
            self.assertFalse(release.documentation_only(code, [('v9.9.9', '0' * 40)], root))

    def test_the_workspace_version_must_be_semantic(self):
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp) / 'Cargo.toml').write_text('[workspace.package]\nversion = "0.1"\n')
            with self.assertRaises(ValueError):
                release.workspace_version(Path(tmp))
        self.assertIsNotNone(release.parse(release.workspace_version()))
        self.assertIsNone(release.parse('v1.2'))
        self.assertIsNone(release.parse('v1.2.3-rc1'))


if __name__ == '__main__':
    unittest.main()
