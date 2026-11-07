#!/usr/bin/env python3
"""The version of the release a Helix Toolbox build is published as.

Releases are tagged vMAJOR.MINOR.PATCH. The commit already has a version tag:
that version again (a rebuild). Otherwise one patch above the newest version
tag, or the workspace version in Cargo.toml when that is higher, so editing it
to 0.2.0 or 1.0.0 starts the next series. With no version tag yet, the
workspace version.

Prints version=..., released=true|false and documentation_only=true|false,
the lines the release workflow appends to $GITHUB_OUTPUT.
documentation_only says the commit differs from the newest released commit
before it only in documentation (the paths the workflows' paths-ignore
lists), which a scheduled run does not publish. --changed says a commit is about to be added to the
checked-out one (the source sync), so its tag does not count.
"""
import argparse
import os
from pathlib import Path
import re
import subprocess
try:
    import tomllib
except ModuleNotFoundError:  # Python before 3.11: the same parser, as tomli
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
TAG = re.compile(r'^v(\d+)\.(\d+)\.(\d+)$')
# What a release does not need to be built for: Markdown at the top, the
# documentation and the pictures. The paths-ignore of the push triggers in
# distributions.yml and sync-sources.yml name the same.
DOCUMENTATION = re.compile(r'^([^/]+\.md|docs/.+|assets/.+)$')


def parse(text):
    """(major, minor, patch) for a version or a vX.Y.Z tag; None otherwise."""
    match = TAG.match(text if text.startswith('v') else f'v{text}')
    return tuple(int(part) for part in match.groups()) if match else None


def workspace_version(root=ROOT):
    manifest = tomllib.loads((root / 'Cargo.toml').read_text())
    version = manifest['workspace']['package']['version']
    if parse(version) is None:
        raise ValueError(f'the workspace version {version!r} is not MAJOR.MINOR.PATCH')
    return version


def next_version(workspace, tags):
    """The version after the newest of `tags`, or `workspace` when higher."""
    versions = [parse(tag) for tag in tags if parse(tag)]
    base = parse(workspace)
    if versions:
        major, minor, patch = max(versions)
        base = max(base, (major, minor, patch + 1))
    return '.'.join(str(part) for part in base)


def release_for(revision, tags, workspace, changed=False):
    """(version, released): the commit's own version, or the next one."""
    if not changed:
        own = sorted((parse(name), name) for name, commit in tags if commit == revision and parse(name))
        if own:
            return own[-1][1][1:], True
    return next_version(workspace, [name for name, _ in tags]), False


def git(root, *arguments):
    return subprocess.run(['git', *arguments], capture_output=True, text=True, cwd=root)


def documentation_only(revision, tags, root=ROOT):
    """Whether `revision` differs from its newest released ancestor only in
    documentation. False when no released commit is among its ancestors here."""
    nearest = None
    for name, commit in tags:
        if not parse(name) or git(root, 'merge-base', '--is-ancestor', commit, revision).returncode != 0:
            continue
        distance = int(git(root, 'rev-list', '--count', f'{commit}..{revision}').stdout)
        if nearest is None or distance < nearest[0]:
            nearest = (distance, commit)
    if nearest is None:
        return False
    changed = git(root, 'diff', '--name-only', nearest[1], revision)
    if changed.returncode != 0:
        return False
    return all(DOCUMENTATION.match(path) for path in changed.stdout.splitlines())


def remote_tags(repository):
    """(name, commit) for every tag of the GitHub repository."""
    output = subprocess.run(
        ['gh', 'api', '--paginate', f'repos/{repository}/tags', '--jq', '.[] | [.name, .commit.sha] | @tsv'],
        check=True, capture_output=True, text=True).stdout
    return [tuple(line.split('\t')) for line in output.splitlines() if line]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--changed', action='store_true', help='a commit is about to be added to HEAD')
    arguments = parser.parse_args()
    revision = subprocess.run(['git', 'rev-parse', 'HEAD'], check=True, capture_output=True, text=True,
                              cwd=ROOT).stdout.strip()
    tags = remote_tags(os.environ['GITHUB_REPOSITORY'])
    version, released = release_for(revision, tags, workspace_version(), arguments.changed)
    print(f'version={version}')
    print(f'released={str(released).lower()}')
    print(f'documentation_only={str(not arguments.changed and documentation_only(revision, tags)).lower()}')


if __name__ == '__main__':
    main()
