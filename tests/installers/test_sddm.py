import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOLS = ROOT / 'scripts/installers/commander'
PATHS = ('/usr/share/sddm/themes/silent', '/etc/sddm.conf.d', '/etc/sddm.conf', '/usr/local/share/fonts/silent-sddm',
         '/etc/systemd/system/display-manager.service')


class SddmScriptTests(unittest.TestCase):
    """sddm.sh installs SilentSDDM and the dotfiles presets against a fake root."""

    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        self.addCleanup(shutil.rmtree, self.root)
        self.tools = self.root / 'tools'
        self.tools.mkdir()
        # One pass, longest path first, so /etc/sddm.conf never matches inside /etc/sddm.conf.d.
        pattern = '|'.join(re.escape(path) for path in sorted(PATHS, key=len, reverse=True))
        script = re.sub(pattern, lambda match: str(self.root) + match.group(0), (TOOLS / 'sddm.sh').read_text())
        (self.tools / 'sddm.sh').write_text(script)
        shutil.copy2(TOOLS / 'configure.py', self.tools / 'configure.py')
        self.theme = self.root / 'usr/share/sddm/themes/silent'
        self.conf_dir = self.root / 'etc/sddm.conf.d'
        self.conf_dir.mkdir(parents=True)
        (self.conf_dir / 'kde_settings.conf').write_text('[Theme]\nCurrent=breeze\n')
        self.log = self.root / 'calls.log'
        # Upstream SilentSDDM and the dotfiles presets, as fetch_source would provide them.
        self.upstream = self.root / 'sources/silentsddm'
        for item in ('backgrounds', 'components', 'configs', 'fonts/redhat', 'fonts/redhat-vf', 'icons'):
            (self.upstream / item).mkdir(parents=True)
        (self.upstream / 'Main.qml').write_text('Item {\n    autoPlay: false\n}\n')
        for name in ('qmldir', 'metadata.desktop', 'LICENSE', 'test.sh', 'change_avatar.sh'):
            (self.upstream / name).write_text(f'upstream {name}\n')
        (self.upstream / 'configs/default.conf').write_text('upstream default\n')
        (self.upstream / 'components/LoginScreen.qml').write_text('upstream login\n')
        (self.upstream / 'fonts/redhat/RedHatDisplay.ttf').write_bytes(b'font')
        self.dotfiles = self.root / 'sources/dotfiles'
        for item in ('sddm/configs', 'sddm/backgrounds', 'sddm/components', 'sddm/sddm-conf'):
            (self.dotfiles / item).mkdir(parents=True)
        (self.dotfiles / 'sddm/configs/kratos.conf').write_text('kratos preset\n')
        (self.dotfiles / 'sddm/backgrounds/gow.mp4').write_bytes(b'video')
        (self.dotfiles / 'sddm/components/LoginScreen.qml').write_text('dotfiles login\n')
        (self.dotfiles / 'sddm/metadata.desktop').write_text('ConfigFile=configs/kratos.conf\n')
        (self.dotfiles / 'sddm/sddm-conf/10-numlock.conf').write_text('[General]\nNumlock=on\n')
        (self.dotfiles / 'sddm/sddm-conf/20-silent.conf').write_text('[General]\nInputMethod=qtvirtualkeyboard\n')
        for name, body in {
            'sddm': 'echo "sddm 0.21.0"',
            'sddm-greeter-qt6': 'echo "preview $QML2_IMPORT_PATH" >> "$CALLS"',
            'fc-cache': 'echo "fc-cache $*" >> "$CALLS"',
            'systemctl': 'echo "systemctl $*" >> "$CALLS"',
            'pacman': 'exit 1',
        }.items():
            path = self.root / name
            path.write_text(f'#!/bin/sh\n{body}\n')
            path.chmod(0o755)

    def run_script(self, answer='y', packager='pacman'):
        (self.tools / 'common.sh').write_text(f'''commander_init() {{ DTYPE=arch; PACKAGER={packager}; ESCALATION_TOOL=env; }}
checkCommandRequirements() {{ :; }}
command_exists() {{ command -v "$1" >/dev/null 2>&1; }}
confirm() {{ :; }}
install_packages() {{ echo "install $*" >> "$CALLS"; }}
backup_system() {{ if [ -e "$1" ]; then backup="$1.backup"; cp -a "$1" "$backup"; fi; }}
fetch_source() {{ source_dir="{self.root}/sources/$1"; }}
''')
        return subprocess.run(['sh', '-e', './sddm.sh'], cwd=self.tools, input=answer + '\n',
                              env=dict(os.environ, PATH=str(self.root) + ':' + os.environ['PATH'], CALLS=str(self.log),
                                       DISPLAY=':0'), text=True, capture_output=True)

    def calls(self):
        return self.log.read_text().splitlines() if self.log.exists() else []

    def test_theme_presets_fonts_and_config_are_installed_and_the_login_screen_switched(self):
        service = self.root / 'etc/systemd/system/display-manager.service'
        service.parent.mkdir(parents=True)
        target = self.root / 'usr/lib/systemd/system/plasmalogin.service'
        target.parent.mkdir(parents=True)
        target.write_text('[Unit]\n')
        service.symlink_to(target)
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.theme / 'Main.qml').read_text(), 'Item {\n    autoPlay: true\n}\n')
        self.assertEqual((self.theme / 'configs/default.conf').read_text(), 'upstream default\n')
        self.assertEqual((self.theme / 'configs/kratos.conf').read_text(), 'kratos preset\n')
        self.assertEqual((self.theme / 'backgrounds/gow.mp4').read_bytes(), b'video')
        self.assertEqual((self.theme / 'components/LoginScreen.qml').read_text(), 'dotfiles login\n')
        self.assertEqual((self.theme / 'metadata.desktop').read_text(), 'ConfigFile=configs/kratos.conf\n')
        self.assertFalse((self.theme / '.git').exists())
        self.assertEqual((self.root / 'usr/local/share/fonts/silent-sddm/redhat/RedHatDisplay.ttf').read_bytes(), b'font')
        self.assertEqual((self.conf_dir / 'zz-commander-theme.conf').read_text(), '[Theme]\nCurrent=silent\n')
        self.assertEqual((self.conf_dir / '10-numlock.conf').read_text(), '[General]\nNumlock=on\n')
        self.assertEqual((self.conf_dir / 'kde_settings.conf').read_text(), '[Theme]\nCurrent=breeze\n')
        self.assertEqual(self.calls(), [
            'install qt6-svg qt6-virtualkeyboard qt6-multimedia qt6-multimedia-ffmpeg qt6-declarative qt6-imageformats',
            f'fc-cache -f {self.root}/usr/local/share/fonts/silent-sddm',
            f'preview {self.theme}/components',
            'systemctl disable plasmalogin.service', 'systemctl enable --force sddm.service'])

    def test_existing_theme_and_sddm_conf_are_backed_up_and_the_switch_can_be_declined(self):
        self.theme.mkdir(parents=True)
        (self.theme / 'Main.qml').write_text('old theme\n')
        sddm_conf = self.root / 'etc/sddm.conf'
        sddm_conf.write_text('[Autologin]\nUser=test\n[Theme]\nCurrent=breeze\n')
        result = self.run_script(answer='')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.theme.with_name('silent.backup') / 'Main.qml').read_text(), 'old theme\n')
        self.assertIn('autoPlay: true', (self.theme / 'Main.qml').read_text())
        self.assertEqual(sddm_conf.with_name('sddm.conf.backup').read_text(), '[Autologin]\nUser=test\n[Theme]\nCurrent=breeze\n')
        self.assertIn('Current=silent', sddm_conf.read_text())
        self.assertIn('User=test', sddm_conf.read_text())
        self.assertNotIn('systemctl enable --force sddm.service', self.calls())
        self.assertIn('enable --force sddm.service', result.stdout)

    def test_a_packaged_theme_only_gets_the_presets(self):
        (self.root / 'pacman').write_text('#!/bin/sh\ncase "$1" in -Qo) exit 0 ;; -Qoq) echo sddm-silent-theme-git ;; esac\n')
        self.theme.mkdir(parents=True)
        (self.theme / 'Main.qml').write_text('    autoPlay: false\n')
        (self.theme / 'configs').mkdir()
        (self.theme / 'configs/default.conf').write_text('packaged default\n')
        result = self.run_script(answer='')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.theme / 'configs/default.conf').read_text(), 'packaged default\n')
        self.assertEqual((self.theme / 'configs/kratos.conf').read_text(), 'kratos preset\n')
        self.assertEqual((self.theme / 'Main.qml').read_text(), '    autoPlay: true\n')
        self.assertFalse((self.root / 'usr/local/share/fonts/silent-sddm').exists())
        self.assertNotIn('fc-cache', ' '.join(self.calls()))


if __name__ == '__main__':
    unittest.main()
