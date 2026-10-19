#!/usr/bin/env python3
"""The record of what the Distributions workflow verified, per container.

  record IMAGE HISTORY CATALOG   what one container's Helix Toolbox history shows
                                 it ran and verified, as JSON on stdout
                                 (scripts/vm-tests.py writes the same for a
                                 virtual machine, as image vm:NAME)
  merge OUT RUN URL COMMIT PART...
                                 one record from every container's
  fetch OUT                      in the release workflow: the record of the
                                 Distributions run for this commit, waiting
                                 for it, or of the newest successful run on an
                                 ancestor when this commit has none (a source
                                 update); fails when this commit's run failed

Release builds carry the record as catalog/validation-results.json, which the
reliability screen reads. It only ever lists what a container's own history
recorded as a successful run.
"""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

REPOSITORY = os.environ.get('GITHUB_REPOSITORY', 'Commanderx-code/Helix-toolbox')
SKIPPED = {'tabs.toml', 'tab.toml', 'group.toml', 'validation.toml', 'setup.toml'}


def entry_ids(catalog):
    return {path.stem for path in Path(catalog).rglob('*.toml') if path.name not in SKIPPED}


def record(image, history, catalog):
    known = entry_ids(catalog)
    runs = json.loads(Path(history).read_text())['data']['runs']
    entries = {}
    for run in runs:
        if not run.get('success'):
            continue
        outcome = 'verified' if 'verified' in run.get('result', '') else 'ran'
        for entry in run.get('entries', []):
            if entry in known and entries.get(entry) != 'verified':
                entries[entry] = outcome
    return {'image': image, 'entries': dict(sorted(entries.items()))}


def merge(run, url, commit, parts):
    images, vms = {}, {}
    for part in parts:
        found = json.loads(Path(part).read_text())
        if found['image'].startswith('vm:'):
            # A virtual machine's cases: only those verified count.
            verified = {entry: 'verified' for entry, outcome in found['entries'].items() if outcome == 'verified'}
            vms[found['image'][3:]] = {'entries': verified}
        else:
            images[found['image']] = {'entries': found['entries']}
    return {
        'format': 1,
        'commit': commit,
        'run': int(run),
        'url': url,
        'finished': int(time.time()),
        'images': dict(sorted(images.items())),
        'vms': dict(sorted(vms.items())),
    }


def gh(*arguments):
    return subprocess.run(['gh', *arguments], check=True, capture_output=True, text=True).stdout


def runs_for(commit):
    found = json.loads(gh('api', f'repos/{REPOSITORY}/actions/workflows/distributions.yml/runs'
                              f'?head_sha={commit}&per_page=20'))['workflow_runs']
    return [run for run in found if run['head_sha'] == commit]


def download(run, out):
    with tempfile.TemporaryDirectory() as directory:
        gh('run', 'download', str(run['id']), '--repo', REPOSITORY, '--name', 'validation', '--dir', directory)
        found = json.loads((Path(directory) / 'validation.json').read_text())
    if found.get('format') != 1 or not isinstance(found.get('images'), dict):
        raise SystemExit('the Distributions record is not in a format this script knows')
    Path(out).write_text(json.dumps(found, indent=1, sort_keys=True) + '\n')
    print(f'Distributions run {run["id"]} on {run["head_sha"][:12]}: '
          f'{sum(len(image["entries"]) for image in found["images"].values())} results '
          f'from {len(found["images"])} containers.')


def fetch(out, patience=3600, appear=180):
    head = subprocess.run(['git', 'rev-parse', 'HEAD'], check=True, capture_output=True, text=True).stdout.strip()
    started = time.time()
    while True:
        runs = [run for run in runs_for(head) if run['event'] in ('push', 'workflow_dispatch', 'schedule')]
        if runs:
            latest = max(runs, key=lambda run: (run['id'], run.get('run_attempt', 1)))
            if latest['status'] == 'completed':
                if latest['conclusion'] != 'success':
                    raise SystemExit(f'Distributions run {latest["id"]} on this commit did not pass; not publishing.')
                return download(latest, out)
        elif time.time() - started > appear:
            break
        if time.time() - started > patience:
            raise SystemExit('Distributions did not finish on this commit in time; not publishing.')
        time.sleep(20)
    # No run for this commit (a source update pushed by the workflow itself):
    # the newest successful one on an ancestor.
    ancestors = subprocess.run(['git', 'rev-list', '--max-count=50', 'HEAD^'], check=True,
                               capture_output=True, text=True).stdout.split()
    for commit in ancestors:
        passed = [run for run in runs_for(commit)
                  if run['status'] == 'completed' and run['conclusion'] == 'success']
        if passed:
            print(f'No Distributions run for {head[:12]}; using the one on its ancestor {commit[:12]}.')
            return download(max(passed, key=lambda run: run['id']), out)
    raise SystemExit('no successful Distributions run on this commit or the 50 before it')


def main(arguments):
    if len(arguments) == 4 and arguments[0] == 'record':
        print(json.dumps(record(*arguments[1:]), indent=1))
    elif len(arguments) >= 6 and arguments[0] == 'merge':
        out, run, url, commit, *parts = arguments[1:]
        Path(out).write_text(json.dumps(merge(run, url, commit, parts), indent=1) + '\n')
    elif len(arguments) == 2 and arguments[0] == 'fetch':
        fetch(arguments[1])
    else:
        raise SystemExit(__doc__)


if __name__ == '__main__':
    main(sys.argv[1:])
