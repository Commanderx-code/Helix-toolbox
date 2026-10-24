from collections import defaultdict
from pathlib import Path
import re
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
REFERENCE = re.compile(r'(?<![\w$/.])((?:\.{1,2}/)+[\w./-]+\.(?:sh|py))')
SOURCED = re.compile(r'^\s*(?:\.|source)\s+((?:\.{1,2}/)+[\w./-]+\.sh)\b')
CD = re.compile(r'(?:^|[;&|(]\s*)cd\s')


def checked_lines(script):
    """The script's numbered lines up to its first `cd`, after which its
    relative paths point into whatever directory it moved to."""
    lines = []
    for number, line in enumerate(script.read_text().splitlines(), 1):
        if CD.search(line.lstrip()):
            break
        lines.append((number, line))
    return lines


def unresolved(installers):
    """Relative script paths that would not exist when their script runs.

    The engine runs a script from its own directory, so its relative paths
    resolve there. A helper that is sourced runs in its caller's directory
    instead, so its paths must resolve from every directory it is sourced
    in, through helpers that source other helpers too.
    """
    lines = {script.resolve(): checked_lines(script) for script in sorted(installers.rglob('*.sh'))}
    sources = {
        script: [match.group(1) for _, line in numbered if (match := SOURCED.match(line))]
        for script, numbered in lines.items()
    }
    directories = {script: {script.parent} for script in lines}
    while True:
        sourced_in = defaultdict(set)
        for script, paths in sources.items():
            for directory in directories[script]:
                for path in paths:
                    sourced_in[(directory / path).resolve()].add(directory)
        settled = {script: sourced_in.get(script) or {script.parent} for script in lines}
        if settled == directories:
            break
        directories = settled
    missing = []
    for script, numbered in lines.items():
        for number, line in numbered:
            for reference in REFERENCE.findall(line):
                for directory in sorted(directories[script]):
                    if not (directory / reference).resolve().is_file():
                        missing.append(f'{script.relative_to(installers.resolve())}:{number}: {reference} '
                                       f'from {directory.relative_to(installers.resolve())}')
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
            (installers / 'system-setup/debian/apt.sh').write_text('./missing.sh\ncd /tmp\n./elsewhere.sh\n')
            # A helper sourcing a helper: both run in tools/, where the inner
            # helper's path does not resolve, though it would from utils/.
            (installers / 'utils/helpers.sh').write_text('. ../../utils/inner.sh\n')
            (installers / 'utils/inner.sh').write_text('. ../common-script.sh\n')
            (installers / 'utils/tools/tool.sh').write_text('. ../helpers.sh\n')
            self.assertEqual(unresolved(installers), [
                'system-setup/debian/apt.sh:1: ./missing.sh from system-setup/debian',
                'system-setup/debian/nala-setup.sh:2: ../../commander/configure.py from system-setup/debian',
                'utils/inner.sh:1: ../common-script.sh from utils/tools',
            ])


if __name__ == '__main__':
    unittest.main()
