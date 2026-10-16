#!/usr/bin/env python3
"""Set the workspace version, and the pinned versions of its own crates, to a
release version, so the crates published to crates.io carry the version the
release is tagged with. The release workflow runs it in its checkout only;
nothing is committed.

usage: scripts/stamp-version.py VERSION [CARGO_TOML]
"""

from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
VERSION = re.compile(r'^\d+\.\d+\.\d+$')


def stamp(text: str, version: str) -> str:
    if not VERSION.match(version):
        raise ValueError(f'not a release version: {version}')
    lines = text.split('\n')
    section = ''
    stamped_package = False
    pinned = 0
    for index, line in enumerate(lines):
        header = re.match(r'^\[([^\]]+)\]\s*$', line)
        if header:
            section = header.group(1)
            continue
        if section == 'workspace.package' and re.match(r'^version\s*=', line):
            lines[index] = f'version = "{version}"'
            stamped_package = True
        elif section == 'workspace.dependencies' and line.startswith('helixforge-'):
            lines[index], count = re.subn(r'version = "=[^"]*"', f'version = "={version}"', line)
            pinned += count
    if not stamped_package or pinned == 0:
        raise ValueError('no workspace version or pinned helixforge crates found')
    return '\n'.join(lines)


def main(argv: list[str]) -> int:
    if len(argv) not in (2, 3):
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    path = Path(argv[2]) if len(argv) == 3 else ROOT / 'Cargo.toml'
    path.write_text(stamp(path.read_text(), argv[1]))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
