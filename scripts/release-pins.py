#!/usr/bin/env python3
"""Pin catalog entries to their projects' latest GitHub releases.

For each entry named, reads its [release] table, asks the GitHub API for the
project's latest release, downloads the file for each architecture the entry
has into a fresh private folder, computes its SHA-256 and compares it with
what the project publishes: the API's digest for the file and a checksums
file in the release, when there is one. A difference stops it. The files are
only read, never unpacked or run; an archive's listing gives the program's
path inside it.

    scripts/release-pins.py ENTRY...            print the new pins
    scripts/release-pins.py ENTRY... --write    rewrite the entries' files
    scripts/release-pins.py --show OWNER/NAME   list a latest release's files

Set GITHUB_TOKEN to ask the API with a token instead of anonymously.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import sys
import tarfile
import tempfile
import tomllib
from urllib.request import Request, urlopen
import zipfile

ROOT = Path(__file__).resolve().parents[1]
CATALOG = ROOT / 'catalog'
API = 'https://api.github.com'
ARCHITECTURES = ('x86_64', 'aarch64')
# Files a release publishes its checksums in.
SUMS = re.compile(r'(checksums?|sha256sums?)(\.txt)?$|\.sha256(sum)?$', re.IGNORECASE)


class Refused(Exception):
    """Something does not match; nothing is written."""


def fetch(url, accept='application/octet-stream'):
    if not url.startswith('https://'):
        raise Refused(f'{url} is not an https URL')
    headers = {'Accept': accept, 'User-Agent': 'helix-toolbox-release-pins'}
    token = os.environ.get('GITHUB_TOKEN')
    if token and url.startswith(API):
        headers['Authorization'] = f'Bearer {token}'
    return urlopen(Request(url, headers=headers), timeout=120)


def latest(repository):
    with fetch(f'{API}/repos/{repository}/releases/latest', 'application/vnd.github+json') as answer:
        return json.load(answer)


def download(url, path):
    with fetch(url) as answer, open(path, 'wb') as out:
        shutil.copyfileobj(answer, out)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, 'rb') as file:
        for block in iter(lambda: file.read(1 << 20), b''):
            digest.update(block)
    return digest.hexdigest()


def entry_file(entry):
    found = [path for path in CATALOG.rglob(f'{entry}.toml') if path.stem == entry]
    if len(found) != 1:
        raise Refused(f'{entry}: {len(found)} catalog files have that id')
    return found[0]


def members(archive):
    """The regular files an archive holds, by path; it is only listed."""
    name = archive.name
    if name.endswith(('.tar.gz', '.tgz', '.tar.xz')):
        with tarfile.open(archive) as tar:
            return [member.name.removeprefix('./') for member in tar.getmembers() if member.isfile()]
    if name.endswith('.zip'):
        with zipfile.ZipFile(archive) as zipped:
            return [info.filename for info in zipped.infolist() if not info.is_dir()]
    return None


def published_sums(release, work):
    """Every file name to SHA-256 the release's checksums files list."""
    sums = {}
    for asset in release.get('assets', []):
        if not SUMS.search(asset['name']) or asset.get('size', 0) > 1 << 20:
            continue
        path = work / ('sums-' + asset['name'])
        download(asset['browser_download_url'], path)
        for line in path.read_text(errors='replace').splitlines():
            parts = line.split()
            if len(parts) >= 2 and re.fullmatch(r'[0-9a-fA-F]{64}', parts[0]):
                sums[parts[-1].lstrip('*').removeprefix('./')] = parts[0].lower()
            elif len(parts) == 1 and re.fullmatch(r'[0-9a-fA-F]{64}', parts[0]):
                # A sum file for one asset: tool.tar.gz.sha256.
                sums[re.sub(r'\.sha256(sum)?$', '', asset['name'])] = parts[0].lower()
    return sums


def pin(entry, work):
    """The entry's file and its new [release] values."""
    path = entry_file(entry)
    table = tomllib.loads(path.read_text()).get('release')
    if not table:
        raise Refused(f'{entry} has no [release] table')
    release = latest(table['repository'])
    tag = release['tag_name']
    version = tag[1:] if tag[:1] in 'vV' and tag[1:2].isdigit() else tag
    old = table['version']
    assets = {asset['name']: asset for asset in release.get('assets', [])}
    sums = None
    pins = {'tag': tag, 'version': version}
    for architecture in ARCHITECTURES:
        current = table.get(architecture)
        if not current:
            continue
        name = current['asset'].replace(old, version)
        asset = assets.get(name)
        if not asset:
            raise Refused(f'{entry}: {tag} has no {name}; choose its file by hand')
        local = work / name
        download(asset['browser_download_url'], local)
        found = sha256(local)
        checked = []
        digest = (asset.get('digest') or '').removeprefix('sha256:')
        if digest:
            if digest != found:
                raise Refused(f'{entry}: {name} is {found}, and GitHub says {digest}')
            checked.append("GitHub's digest")
        if sums is None:
            sums = published_sums(release, work)
        if name in sums:
            if sums[name] != found:
                raise Refused(f"{entry}: {name} is {found}, and the release's checksums say {sums[name]}")
            checked.append("the release's checksums")
        if not checked:
            print(f'{entry}: nothing in {tag} confirms {name}; check {found} another way', file=sys.stderr)
        program = None
        listed = members(local)
        if listed is not None:
            wanted = (current.get('path') or table['binary']).replace(old, version)
            if wanted in listed:
                program = wanted
            else:
                named = [member for member in listed if member.rsplit('/', 1)[-1] == table['binary']]
                if len(named) != 1:
                    raise Refused(f'{entry}: {name} holds {len(named)} files named {table["binary"]}')
                program = named[0]
        pins[architecture] = {'asset': name, 'sha256': found, 'path': program, 'checked': checked}
        local.unlink()
    return path, pins


def rewrite(path, pins):
    """The entry's file with its release pinned anew; nothing else changes."""
    text = path.read_text()
    sections = re.split(r'(?m)^(?=\[)', text)
    out = []
    for section in sections:
        header = section.split('\n', 1)[0].strip()
        values = None
        if header == '[release]':
            values = {'tag': pins['tag'], 'version': pins['version']}
        for architecture in ARCHITECTURES:
            if header == f'[release.{architecture}]' and architecture in pins:
                values = {key: pins[architecture][key] for key in ('asset', 'sha256', 'path')}
        for key, value in (values or {}).items():
            line = re.compile(rf'(?m)^{key} = ".*"$')
            if value is None:
                section = line.sub('', section).replace('\n\n\n', '\n\n')
            elif line.search(section):
                section = line.sub(f'{key} = "{value}"', section)
            else:
                section = section.rstrip('\n') + f'\n{key} = "{value}"\n'
        out.append(section)
    path.write_text(''.join(out))


def show(repository):
    release = latest(repository)
    print(release['tag_name'])
    for asset in release.get('assets', []):
        digest = asset.get('digest') or '-'
        print(f"  {asset['name']}  {asset.get('size', 0)}  {digest}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('entries', nargs='*')
    parser.add_argument('--write', action='store_true', help="rewrite the entries' files")
    parser.add_argument('--show', metavar='OWNER/NAME', help="list a latest release's files")
    args = parser.parse_args()
    if args.show:
        show(args.show)
        return 0
    if not args.entries:
        parser.error('name an entry, or --show a repository')
    failed = False
    for entry in args.entries:
        work = Path(tempfile.mkdtemp(prefix='helix-toolbox-pins-'))
        try:
            path, pins = pin(entry, work)
        except Refused as error:
            print(f'Refused: {error}', file=sys.stderr)
            failed = True
            continue
        finally:
            shutil.rmtree(work, ignore_errors=True)
        print(f'{entry}: {pins["tag"]}')
        for architecture in ARCHITECTURES:
            if architecture in pins:
                found = pins[architecture]
                checked = ', '.join(found['checked']) or 'nothing'
                print(f'  {architecture}: {found["asset"]} {found["sha256"]} ({checked})')
                if found['path']:
                    print(f'    program: {found["path"]}')
        if args.write:
            rewrite(path, pins)
            print(f'  wrote {path.relative_to(ROOT)}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
