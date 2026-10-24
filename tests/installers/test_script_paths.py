from collections import defaultdict
from pathlib import Path
import re
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
REFERENCE = re.compile(r'(?<![\w$/.])((?:\.\./)+[\w./-]+\.(?:sh|py))')
SOURCED = re.compile(r'^\s*(?:\.|source)\s+((?:\.\./)+[\w./-]+\.sh)\b')


def unresolved(installers):
    """Relative script paths that would not exist when their script runs.

    The engine runs a script from its own directory, so its relative paths
    resolve there. A helper that is sourced runs in its caller's directory
    instead, so its paths must resolve from every directory that sources it.
    """
    scripts = sorted(installers.rglob('*.sh'))
    sourced_from = defaultdict(set)
    for script in scripts:
        for line in script.read_text().splitlines():
            match = SOURCED.match(line)
            if match:
                sourced_from[(script.parent / match.group(1)).resolve()].add(script.parent)
    missing = []
    for script in scripts:
        directories = sourced_from.get(script.resolve()) or {script.parent}
        for number, line in enumerate(script.read_text().splitlines(), 1):
            for reference in REFERENCE.findall(line):
                for directory in sorted(directories):
                    if not (directory / reference).resolve().is_file():
                        missing.append(f'{script.relative_to(installers)}:{number}: {reference} from {directory.relative_to(installers)}')
    return missing


class ScriptPathTests(unittest.TestCase):
    def test_relative_script_paths_exist(self):
        self.assertEqual(unresolved(ROOT / 'scripts/installers'), [])

    def test_a_missing_target_is_reported(self):
        with tempfile.TemporaryDirectory() as tmp:
            installers = Path(tmp)
            (installers / 'system-setup/debian').mkdir(parents=True)
            (installers / 'utils/tools').mkdir(parents=True)
            (installers / 'common-script.sh').write_text('')
            (installers / 'system-setup/debian/nala-setup.sh').write_text(
                '. ../../common-script.sh\npython3 ../../commander/configure.py nala-check\n')
            (installers / 'utils/helpers.sh').write_text('. ../../common-script.sh\n')
            (installers / 'utils/tools/tool.sh').write_text('. ../helpers.sh\n')
            self.assertEqual(unresolved(installers), [
                'system-setup/debian/nala-setup.sh:2: ../../commander/configure.py from system-setup/debian',
            ])


if __name__ == '__main__':
    unittest.main()
