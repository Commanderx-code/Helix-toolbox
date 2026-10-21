#!/usr/bin/env python3
"""Add a hardware report's records to catalog/validation.toml, from its issue.

A hardware report issue (the "It worked on my machine" form, which Helix
Toolbox fills in) holds the records Helix Toolbox made for the run. This
reads the issue with gh, checks that its boxes are ticked and that each
record names a catalog entry, a system, a version and a date, takes the
System line from the form (where the reporter may have corrected it) and
what they checked as the note, and adds the records under the comment that
describes them. Nothing is committed; review the change, then open a pull
request that closes the issue.

usage: scripts/accept-hardware-report.py ISSUE [--dry-run]
"""

import importlib.util
import json
from pathlib import Path
import re
import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[1]
REPOSITORY = 'Commanderx-code/Helix-toolbox'
CLAIMS = ROOT / 'catalog/validation.toml'
FIELDS = ('entry', 'system', 'version', 'date')

spec = importlib.util.spec_from_file_location('validation_record', ROOT / 'scripts/validation-record.py')
validation_record = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validation_record)


def sections(body):
    """An issue form's answers, by their labels."""
    found = {}
    label = None
    for line in body.replace('\r\n', '\n').split('\n'):
        heading = re.match(r'^### (.+?)\s*$', line)
        if heading:
            label = heading.group(1)
            found[label] = []
        elif label is not None:
            found[label].append(line)
    return {label: '\n'.join(lines).strip() for label, lines in found.items()}


def unfenced(text):
    match = re.search(r'```[a-z]*\n(.*?)```', text, re.S)
    return match.group(1) if match else text


def records(number, body, known):
    """The records an issue asks for, ready to add; ValueError says why not."""
    answers = sections(body)
    boxes = answers.get('Before you submit', '')
    if boxes.count('- [X]') + boxes.count('- [x]') < 2:
        raise ValueError('the reporter has not ticked both boxes under "Before you submit"')
    try:
        claims = tomllib.loads(unfenced(answers.get('The record', '')))
    except tomllib.TOMLDecodeError as error:
        raise ValueError(f'the record is not TOML: {error}') from error
    hardware = claims.get('hardware')
    if not isinstance(hardware, list) or not hardware:
        raise ValueError('the record has no [[hardware]] table')
    system = ' '.join(answers.get('System', '').split())
    checked = ' '.join(answers.get('What you checked', '').split())
    if not checked or checked == '_No response_':
        raise ValueError('the reporter did not say what they checked')
    if len(checked) > 300:
        checked = checked[:297].rstrip() + '...'
    found = []
    for claim in hardware:
        if not isinstance(claim, dict) or not all(isinstance(claim.get(field), str) and claim[field].strip() for field in FIELDS):
            raise ValueError(f'a record lacks one of {", ".join(FIELDS)}: {claim}')
        if claim['entry'] not in known:
            raise ValueError(f'{claim["entry"]} is not an entry in this catalog')
        if not re.fullmatch(r'\d{4}-\d{2}-\d{2}', claim['date']):
            raise ValueError(f'{claim["date"]} is not a date')
        if not re.fullmatch(r'\d+\.\d+\.\d+', claim['version']):
            raise ValueError(f'{claim["version"]} is not a Helix Toolbox version')
        found.append({
            'entry': claim['entry'],
            'system': system or ' '.join(claim['system'].split()),
            'version': claim['version'],
            'date': claim['date'],
            'note': f'{checked} (#{number})',
        })
    return found


def toml(found):
    lines = []
    for claim in found:
        lines.append('[[hardware]]')
        for field in ('entry', 'system', 'version', 'date', 'note'):
            lines.append(f'{field} = {json.dumps(claim[field], ensure_ascii=False)}')
        lines.append('')
    return '\n'.join(lines)


def added(text, found):
    """validation.toml with the records added, leaving out any it has."""
    have = tomllib.loads(text).get('hardware', [])
    same = lambda one, other: all(one[field] == other.get(field) for field in FIELDS)
    new = [claim for claim in found if not any(same(claim, old) for old in have)]
    if not new:
        return text, []
    return text.rstrip('\n') + '\n\n' + toml(new), new


def main(arguments):
    if not arguments or not arguments[0].isdigit():
        print(__doc__.strip().splitlines()[-1], file=sys.stderr)
        return 2
    number, dry_run = int(arguments[0]), '--dry-run' in arguments[1:]
    issue = json.loads(subprocess.run(
        ['gh', 'issue', 'view', str(number), '--repo', REPOSITORY, '--json', 'body,labels,state'],
        check=True, capture_output=True, text=True).stdout)
    if 'hardware-report' not in [label['name'] for label in issue['labels']]:
        print(f'#{number} is not a hardware report', file=sys.stderr)
        return 1
    try:
        found = records(number, issue['body'], validation_record.entry_ids(ROOT / 'catalog'))
    except ValueError as error:
        print(f'#{number}: {error}', file=sys.stderr)
        return 1
    text, new = added(CLAIMS.read_text(), found)
    if not new:
        print(f'catalog/validation.toml already has the records of #{number}')
        return 0
    print(toml(new), end='')
    if dry_run:
        print('Dry run: catalog/validation.toml is unchanged.')
    else:
        tomllib.loads(text)
        CLAIMS.write_text(text)
        print(f'Added to catalog/validation.toml. Open a pull request that says "Closes #{number}".')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
