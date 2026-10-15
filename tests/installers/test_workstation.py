import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'


class WorkstationScriptTests(unittest.TestCase):
    """workstation.sh clones or updates the dotfiles checkout, then hands over to scripts/setup."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        shutil.copy2(TOOLS / 'workstation.sh', self.tools / 'workstation.sh')
        self.checkout = self.root / 'home/github/projects/dotfiles'
        self.log = self.root / 'calls.log'
        git = self.root / 'git'
        git.write_text('''#!/bin/sh
echo "git $*" >> "$CALLS"
case "$1" in
    clone) mkdir -p "$3/scripts" "$3/.git"; printf '#!/bin/sh\\necho "setup $PWD" >> "$CALLS"\\n' > "$3/scripts/setup" ;;
    -C) [ "$3" = pull ] && [ "$PULL_OK" = yes ] || exit 1 ;;
esac
''')
        git.chmod(0o755)

    def run_script(self, packager='pacman', pull_ok=True, checkout=None):
        (self.tools / 'common.sh').write_text(f'''commander_init() {{ PACKAGER={packager}; ESCALATION_TOOL=env; }}
command_exists() {{ command -v "$1" >/dev/null 2>&1; }}
confirm() {{ :; }}
install_packages() {{ echo "install $*" >> "$CALLS"; }}
''')
        env = dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'], CALLS=str(self.log),
                   HOME=str(self.root / 'home'), PULL_OK='yes' if pull_ok else 'no')
        env.pop('COMMANDER_DOTFILES_DIR', None)
        if checkout is not None:
            env['COMMANDER_DOTFILES_DIR'] = str(checkout)
        return subprocess.run(['sh', '-e', './workstation.sh'], cwd=self.tools, env=env, text=True, capture_output=True)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_fresh_machine_clones_then_runs_the_setup(self):
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [f'git clone https://github.com/Commanderx-code/dotfiles.git {self.checkout}',
                                        f'setup {self.checkout}'])

    def test_existing_checkout_is_fast_forwarded_then_set_up(self):
        (self.checkout / 'scripts').mkdir(parents=True)
        (self.checkout / '.git').mkdir()
        (self.checkout / 'scripts/setup').write_text('#!/bin/sh\necho "setup $PWD" >> "$CALLS"\n')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [f'git -C {self.checkout} pull --ff-only', f'setup {self.checkout}'])
        self.log.unlink()
        result = self.run_script(pull_ok=False)
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.calls(), [f'git -C {self.checkout} pull --ff-only'])

    def test_other_directories_and_package_managers_are_refused(self):
        self.checkout.mkdir(parents=True)
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('not a Git checkout', result.stderr)
        result = self.run_script(packager='dnf')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('Arch-based', result.stderr)
        self.assertEqual(self.calls(), [])
        elsewhere = self.root / 'elsewhere'
        result = self.run_script(checkout=elsewhere)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), [f'git clone https://github.com/Commanderx-code/dotfiles.git {elsewhere}', f'setup {elsewhere}'])


if __name__ == '__main__':
    unittest.main()
