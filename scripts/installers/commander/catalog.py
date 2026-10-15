#!/usr/bin/env python3
"""Validate source-owned Toolbox catalogs and install exported user configs."""
import argparse
import json
import os
from pathlib import Path, PurePosixPath
import re

from configure import check_destination, deploy


BUILTINS = {
    'dotfiles': {'dotfiles-nvim', 'dotfiles-fastfetch', 'dotfiles-starship',
                 'dotfiles-konsole', 'dotfiles-ghostty', 'zellij', 'netwatch', 'tfm', 'cassette'},
    'Myfish': {'myfish', 'myfish-fish', 'myfish-bash', 'myfish-zsh'},
}
MANAGERS = {'apt-get', 'dnf', 'pacman'}


def relative(value):
    if not isinstance(value, str) or not value or '\\' in value:
        raise ValueError('Expected a relative POSIX path.')
    path = PurePosixPath(value)
    if path.is_absolute() or any(p in ('', '.', '..') for p in value.split('/')):
        raise ValueError(f'Unsafe relative path: {value}')
    if any(ord(c) < 32 for c in value):
        raise ValueError('Control characters are not allowed in paths.')
    return path


def source_path(root, value):
    path = root / relative(value)
    if not path.exists() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError(f'Missing or escaping source: {value}')
    for item in (path, *path.parents):
        if item == root:
            break
        if item.is_symlink():
            raise ValueError(f'Symlink source is not supported: {value}')
    if path.is_dir() and any(p.is_symlink() for p in path.rglob('*')):
        raise ValueError(f'Symlinks inside exported directory: {value}')
    return path


def validate(document, source, root=None):
    if source not in BUILTINS or document.get('version') != 1:
        raise ValueError('Unsupported source or catalog version.')
    entries = document.get('entries')
    if not isinstance(entries, list) or not entries:
        raise ValueError('The catalog must contain entries.')
    ids, names = set(), set()
    for entry in entries:
        if not isinstance(entry, dict):
            raise ValueError('Each entry must be an object.')
        identity = entry.get('id', '')
        if not re.fullmatch(r'[a-z][a-z0-9-]{0,63}', identity) or identity in ids:
            raise ValueError(f'Invalid or duplicate entry ID: {identity}')
        ids.add(identity)
        for key in ('name', 'description'):
            value = entry.get(key)
            if not isinstance(value, str) or not value.strip() or any(ord(c) < 32 for c in value):
                raise ValueError(f'Invalid {key}: {identity}')
        if entry['name'] in names:
            raise ValueError(f'Duplicate menu name: {entry["name"]}')
        names.add(entry['name'])
        base_keys = {'id', 'name', 'description', 'type'}
        if entry.get('type') == 'builtin':
            if set(entry) != base_keys | {'handler'} or entry['handler'] not in BUILTINS[source]:
                raise ValueError(f'Unknown built-in handler: {identity}')
        elif entry.get('type') == 'config':
            if set(entry) != base_keys | {'files', 'packages'}:
                raise ValueError(f'Unexpected config fields: {identity}')
            packages = entry['packages']
            if not isinstance(packages, dict) or not packages or set(packages) - MANAGERS:
                raise ValueError(f'Declare supported package managers: {identity}')
            for values in packages.values():
                if not isinstance(values, list) or any(
                    not isinstance(p, str) or not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9+_.:-]*', p)
                    for p in values
                ):
                    raise ValueError(f'Invalid package names: {identity}')
            if not isinstance(entry['files'], list) or not entry['files']:
                raise ValueError(f'Config entry needs files: {identity}')
            destinations = []
            for file in entry['files']:
                if not isinstance(file, dict) or set(file) != {'source', 'target'}:
                    raise ValueError('Files need source and target paths.')
                relative(file['source'])
                target = relative(file['target'])
                if any(target == other or target in other.parents or other in target.parents
                       for other in destinations):
                    raise ValueError('Overlapping config destinations.')
                destinations.append(target)
                if root is not None:
                    source_path(root, file['source'])
        else:
            raise ValueError(f'Unsupported entry type: {identity}')
    return entries


def preflight(entry, root, config, manager):
    manager = 'apt-get' if manager == 'nala' else manager
    if manager not in entry['packages']:
        raise ValueError(f'This tool does not declare support for {manager}.')
    for file in entry['files']:
        source_path(root, file['source'])
        check_destination(config / relative(file['target']))
    return entry['packages'][manager]


def install(entry, root, config, manager):
    preflight(entry, root, config, manager)
    for file in entry['files']:
        deploy(source_path(root, file['source']), config / relative(file['target']))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('packages', 'install'))
    parser.add_argument('source', choices=BUILTINS)
    parser.add_argument('identity')
    parser.add_argument('root', type=Path)
    parser.add_argument('manager')
    args = parser.parse_args()
    document = json.loads((Path(__file__).parent / 'catalogs' / f'{args.source}.json').read_text())
    entries = validate(document, args.source, args.root)
    entry = next(e for e in entries if e['id'] == args.identity)
    if entry['type'] != 'config':
        raise ValueError('Built-in entries use their dedicated installer.')
    config = Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config')
    if not config.is_absolute():
        raise ValueError('XDG_CONFIG_HOME must be absolute.')
    packages = preflight(entry, args.root, config, args.manager)
    if args.action == 'packages':
        for package in packages:
            print(package)
    else:
        install(entry, args.root, config, args.manager)


if __name__ == '__main__':
    main()
