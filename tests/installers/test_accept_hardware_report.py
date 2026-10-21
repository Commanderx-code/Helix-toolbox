import importlib.util
from pathlib import Path
import unittest

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('accept', ROOT / 'scripts/accept-hardware-report.py')
accept = importlib.util.module_from_spec(spec)
spec.loader.exec_module(accept)

# An issue from the form, as GitHub writes it.
BODY = '''### Entries

SDDM (sddm), GRUB Dracula theme (grub-dracula)

### System

Garuda Linux,  KDE Plasma 6.5.0 (wayland), Lenovo ThinkPad T15p Gen 2i, NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)

### Helix Toolbox version

1.15.0

### When it ran

2026-10-21

### What you checked

Rebooted; the login screen came up with the theme,
and the boot menu too.

### The record

```toml
[[hardware]]
entry = "sddm"
system = "Garuda Linux, Lenovo ThinkPad"
version = "1.15.0"
date = "2026-10-21"

[[hardware]]
entry = "grub-dracula"
system = "Garuda Linux, Lenovo ThinkPad"
version = "1.15.0"
date = "2026-10-21"
```

### Before you submit

- [X] This ran on a real machine, not in a virtual machine or a container.
- [X] The run finished, and what it set up still works after a reboot.
'''


class AcceptHardwareReportTest(unittest.TestCase):
    def setUp(self):
        self.known = accept.validation_record.entry_ids(ROOT / 'catalog')

    def test_the_records_take_the_forms_system_and_what_was_checked(self):
        found = accept.records(42, BODY, self.known)
        self.assertEqual([claim['entry'] for claim in found], ['sddm', 'grub-dracula'])
        self.assertEqual(found[0]['system'], 'Garuda Linux, KDE Plasma 6.5.0 (wayland), Lenovo ThinkPad T15p Gen 2i, NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)')
        self.assertEqual(found[0]['note'], 'Rebooted; the login screen came up with the theme, and the boot menu too. (#42)')

    def test_they_are_added_to_validation_toml_once(self):
        text = (ROOT / 'catalog/validation.toml').read_text()
        found = accept.records(42, BODY, self.known)
        changed, new = accept.added(text, found)
        self.assertEqual(len(new), 2)
        self.assertTrue(changed.startswith(text.rstrip('\n')))
        hardware = tomllib.loads(changed)['hardware']
        self.assertEqual([claim['entry'] for claim in hardware[-2:]], ['sddm', 'grub-dracula'])
        again, none = accept.added(changed, found)
        self.assertEqual((again, none), (changed, []))

    def test_reports_that_are_not_ready_are_refused(self):
        refusals = {
            'both boxes': BODY.replace('- [X] The run', '- [ ] The run'),
            'not an entry': BODY.replace('entry = "sddm"', 'entry = "no-such-entry"'),
            'not a date': BODY.replace('date = "2026-10-21"\n\n', 'date = "yesterday"\n\n'),
            'what they checked': BODY.replace('Rebooted; the login screen came up with the theme,\nand the boot menu too.', '_No response_'),
            'not TOML': BODY.replace('[[hardware]]\nentry = "sddm"', '[[hardware]\nentry = "sddm"'),
        }
        for why, body in refusals.items():
            with self.assertRaisesRegex(ValueError, why):
                accept.records(42, body, self.known)


if __name__ == '__main__':
    unittest.main()
