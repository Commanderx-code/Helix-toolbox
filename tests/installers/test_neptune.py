"""linux-neptune.sh's choice of kernel, with a stand-in pacman."""
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SCRIPT = ROOT / 'scripts/installers/system-setup/arch/linux-neptune.sh'

# Part of what Valve's jupiter-staging offered in October 2026.
OFFERED = """jupiter-staging linux-neptune 5.13.0.valve37-1
jupiter-staging linux-neptune-headers 5.13.0.valve37-1
jupiter-staging linux-neptune-61 6.1.52.valve19-1
jupiter-staging linux-neptune-611 6.11.11.valve29-1
jupiter-staging linux-neptune-618 6.18.39.valve1-1
jupiter-staging linux-neptune-618-devel 6.18.28.valve1-1
jupiter-staging linux-neptune-618-drm-exec 6.18.33.drmexec.valve2-1
jupiter-staging linux-neptune-618-headers 6.18.39.valve1-1
jupiter-staging linux-neptune-65 6.5.0.valve23-1
jupiter-staging linux-neptune-72 7.2.0.rc3.valve.beta1-1
jupiter-staging linux-neptune-72-headers 7.2.0.rc3.valve.beta1-1
"""


class KernelTests(unittest.TestCase):
    def choose(self, offered):
        function = re.search(r'^latestKernel\(\) \{\n.*?^\}\n', SCRIPT.read_text(), re.M | re.S)
        self.assertIsNotNone(function, 'linux-neptune.sh does not choose a kernel')
        with tempfile.TemporaryDirectory() as folder:
            pacman = Path(folder) / 'pacman'
            pacman.write_text(f"#!/bin/sh\ncat <<'LIST'\n{offered}LIST\n")
            pacman.chmod(0o755)
            result = subprocess.run(['sh', '-c', f'PACKAGER=pacman\n{function.group(0)}latestKernel'],
                                    capture_output=True, text=True, check=True,
                                    env={**os.environ, 'PATH': f'{folder}:{os.environ["PATH"]}'})
        return result.stdout.strip()

    def test_the_newest_released_kernel_not_the_2021_one(self):
        self.assertEqual(self.choose(OFFERED), 'linux-neptune-618')

    def test_nothing_offered_chooses_nothing(self):
        self.assertEqual(self.choose(''), '')


if __name__ == '__main__':
    unittest.main()
