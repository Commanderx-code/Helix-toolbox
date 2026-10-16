import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'


class AppArmorScriptTests(unittest.TestCase):
    """apparmor.sh adds the lsm= parameter on Arch with GRUB, with the GRUB transaction safety."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        self.defaults = self.root / 'etc/default/grub'
        self.defaults.parent.mkdir(parents=True)
        self.defaults.write_text('GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')
        self.boot = self.root / 'boot/grub'
        self.boot.mkdir(parents=True)
        self.menu = self.boot / 'grub.cfg'
        self.menu.write_text('original boot menu')
        self.enabled = self.root / 'sys/module/apparmor/parameters/enabled'
        self.enabled.parent.mkdir(parents=True)
        self.log = self.root / 'calls.log'
        for name in ('apparmor.sh', 'kernel-cmdline.sh'):
            script = (TOOLS / name).read_text()
            for path in ('/etc/default/grub', '/etc/default/limine', '/boot/grub', '/sys/module/apparmor/parameters/enabled'):
                script = script.replace(path, str(self.root) + path)
            (self.tools / name).write_text(script)
        shutil.copy2(TOOLS / 'configure.py', self.tools / 'configure.py')
        (self.root / 'common-service-script.sh').write_text('startAndEnableService() { echo "service $1" >> "$CALLS"; }\n')
        self.write_tool('grub-mkconfig', 'printf "new menu" > "$2"')
        self.write_tool('grub-script-check', 'exit 0')
        self.write_tool('aa-status', 'echo "aa-status" >> "$CALLS"')

    def write_tool(self, name, body):
        path = self.root / name
        path.write_text(f'#!/bin/sh\n{body}\n')
        path.chmod(0o755)

    def run_script(self, packager='pacman', dtype='arch'):
        (self.tools / 'common.sh').write_text(f'''commander_init() {{ DTYPE={dtype}; PACKAGER={packager}; ESCALATION_TOOL=env; }}
checkCommandRequirements() {{ :; }}
command_exists() {{ command -v "$1" >/dev/null 2>&1; }}
confirm() {{ :; }}
install_packages() {{ echo "install $*" >> "$CALLS"; }}
backup_system() {{ backup="$1.backup"; cp -a "$1" "$backup"; }}
''')
        return subprocess.run(['sh', '-e', './apparmor.sh'], cwd=self.tools,
                              env=dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'], CALLS=str(self.log), HELIXFORGE_LIB=str(self.root)),
                              text=True, capture_output=True)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_arch_without_apparmor_gets_the_kernel_parameter_and_stops(self):
        self.enabled.write_text('N\n')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.defaults.read_text(),
                         'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash lsm=landlock,lockdown,yama,integrity,apparmor,bpf"\n')
        self.assertEqual(self.menu.read_text(), 'new menu')
        self.assertTrue(self.defaults.with_name('grub.backup').is_file())
        self.assertEqual(self.calls(), [])
        self.assertIn('Reboot', result.stdout)

    def test_failed_menu_generation_restores_the_defaults(self):
        self.enabled.write_text('N\n')
        self.write_tool('grub-mkconfig', 'exit 1')
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')
        self.assertEqual(self.menu.read_text(), 'original boot menu')
        self.assertEqual(list(self.boot.glob('.commander-grub.*')), [])

    def test_limine_machines_get_the_parameter_through_the_entry_tool(self):
        self.enabled.write_text('N\n')
        self.defaults.unlink()
        limine = self.root / 'etc/default/limine'
        limine.write_text('KERNEL_CMDLINE[default]="quiet splash lsm=landlock,yama"\n')
        self.write_tool('limine-mkinitcpio', 'echo "limine-mkinitcpio" >> "$CALLS"')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(limine.read_text(), 'KERNEL_CMDLINE[default]="quiet splash lsm=landlock,lockdown,yama,integrity,apparmor,bpf"\n')
        self.assertEqual(self.calls(), ['limine-mkinitcpio'])
        self.assertTrue(limine.with_name('limine.backup').is_file())

    def test_debian_without_apparmor_is_only_told(self):
        self.enabled.write_text('N\n')
        result = self.run_script(packager='apt-get', dtype='debian')
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')
        self.assertEqual(self.calls(), [])

    def test_a_service_systemd_skipped_is_started_after_asking_again(self):
        # Seen in a VM: systemd kept its answer from before the package and
        # skipped apparmor.service until it was re-executed.
        self.enabled.write_text('Y\n')
        (self.root / 'common-service-script.sh').write_text(
            'INIT_MANAGER=systemctl\n'
            'startAndEnableService() { echo "service $1" >> "$CALLS"; }\n'
            'isServiceActive() { return 1; }\n')
        self.write_tool('systemctl', 'echo "systemctl $*" >> "$CALLS"')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ['install apparmor', 'service apparmor', 'systemctl daemon-reexec',
                                        'systemctl start apparmor', 'aa-status'])

    def test_enabled_kernel_installs_and_starts_the_service(self):
        self.enabled.write_text('Y\n')
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(self.calls(), ['install apparmor', 'service apparmor', 'aa-status'])
        self.assertEqual(self.defaults.read_text(), 'GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT="quiet splash"\n')


if __name__ == '__main__':
    unittest.main()
