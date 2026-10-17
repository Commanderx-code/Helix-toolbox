"""server-setup.sh's timezone question, with a stand-in for its lookup."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/installers/system-setup/arch/server-setup.sh'


def function(name):
    match = re.search(rf'^{name} \(\) {{\n.*?^}}\n', SCRIPT.read_text(), re.M | re.S)
    return match.group(0)


class TimezoneTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.bin = Path(self.temporary.name) / 'bin'
        self.bin.mkdir()
        # The timezones it knows, which minimal containers lack.
        self.zones = Path(self.temporary.name) / 'zoneinfo'
        for zone in ('UTC', 'Europe/London'):
            (self.zones / zone).parent.mkdir(parents=True, exist_ok=True)
            (self.zones / zone).write_text('TZif')

    def tearDown(self):
        self.temporary.cleanup()

    def ask(self, lookup, answers):
        curl = self.bin / 'curl'
        curl.write_text(f'#!/bin/sh\n{lookup}\n')
        curl.chmod(0o755)
        try:
            program = function('timezone').replace('/usr/share/zoneinfo', str(self.zones))
            result = subprocess.run(['bash', '-c', program + 'timezone\necho "chosen=$TIMEZONE"'],
                                    input=answers, capture_output=True, text=True, timeout=10,
                                    env={**os.environ, 'PATH': f'{self.bin}:{os.environ["PATH"]}'})
        except subprocess.TimeoutExpired:
            self.fail('the question is asked again and again')
        return result.stdout.splitlines()[-1]

    def test_the_timezone_found_is_offered(self):
        self.assertEqual(self.ask("printf 'Europe/London'", 'y\n'), 'chosen=Europe/London')

    def test_when_the_lookup_fails_utc_is_offered(self):
        # The lookup service limits how often it answers, and needs a network.
        self.assertEqual(self.ask('exit 22', 'y\n'), 'chosen=UTC')
        self.assertEqual(self.ask("printf 'Not/AZone'", '\n'), 'chosen=UTC')


if __name__ == '__main__':
    unittest.main()
