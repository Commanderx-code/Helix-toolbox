#!/usr/bin/env python3
"""Print the package names the catalog's entries install for one family.

usage: scripts/package-names.py KEY [--detect]

KEY is the field the catalog names a family's package with: arch, debian,
fedora or suse. Names come from [package] tables; with --detect, also from
the [detect] tables of script entries. The Distributions workflow checks
each one exists in that distribution's repositories.
"""

import pathlib
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python before 3.11
    import tomli as tomllib

CATALOG = pathlib.Path(__file__).resolve().parent.parent / 'catalog'
KEYS = ('arch', 'debian', 'fedora', 'suse')


def names(key, detect=False):
    tables = ('package', 'detect') if detect else ('package',)
    found = set()
    for path in CATALOG.rglob('*.toml'):
        entry = tomllib.loads(path.read_text())
        for table in tables:
            name = entry.get(table, {}).get(key)
            if name:
                found.add(name)
    return sorted(found)


def main(argv):
    if not argv or argv[0] not in KEYS or argv[1:] not in ([], ['--detect']):
        print(__doc__.strip().splitlines()[2], file=sys.stderr)
        return 2
    for name in names(argv[0], detect=argv[1:] == ['--detect']):
        print(name)
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
