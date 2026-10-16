#!/usr/bin/env python3
"""Run HelixForge's boot, login-screen and kernel entries in a virtual machine.

  vm-tests.py --distro arch|debian --binary PATH --work DIR --out RESULT.json
              [--shots DIR] [--only ID,ID]

Boots the distribution's cloud image under QEMU with KVM, makes it boot like
a desktop installation (a user in the administrators' group, GRUB on the
screen with a visible menu), copies HelixForge in, and for each case: runs
the entry through `helixforge run`, answering its questions, reboots,
captures the screen while it boots, and checks what the entry should have
done (its configuration, the kernel command line, the boot images, services)
and that the machine came back. The result lists each entry as `verified` or
`failed` with why, as the reliability record expects; any failure makes the
script exit 1. Nothing here touches the machine running it beyond the work
folder.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import shlex
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

IMAGES = {
    'arch': {
        'url': 'https://geo.mirror.pkgbuild.com/images/latest/Arch-Linux-x86_64-cloudimg.qcow2',
        'sums': 'https://geo.mirror.pkgbuild.com/images/latest/Arch-Linux-x86_64-cloudimg.qcow2.SHA256',
        'hash': 'sha256',
        'admins': 'wheel',
    },
    'debian': {
        'url': 'https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-amd64.qcow2',
        'sums': 'https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS',
        'hash': 'sha512',
        'admins': 'sudo',
    },
}

PORT = 2222


def log(message):
    print(f'vm-tests: {message}', flush=True)


class Machine:
    def __init__(self, work, distro, port=PORT):
        self.port = port
        self.work = work
        self.distro = distro
        self.key = work / 'key'
        # Unix socket paths are short: a private folder of its own in /tmp.
        self.monitor = Path(tempfile.mkdtemp(prefix='hf-vm-', dir='/tmp')) / 'monitor'

    # ----- the image and the machine -----

    def fetch(self):
        image = IMAGES[self.distro]
        base = self.work / f'{self.distro}.qcow2'
        if not base.exists():
            log(f'downloading {image["url"]}')
            partial = base.with_suffix('.part')
            urllib.request.urlretrieve(image['url'], partial)
            sums = urllib.request.urlopen(image['sums'], timeout=60).read().decode()
            name = image['url'].rsplit('/', 1)[1]
            want = next(line.split()[0] for line in sums.splitlines()
                        if line.split() and line.split()[-1].lstrip('*') == name)
            digest = hashlib.new(image['hash'])
            with open(partial, 'rb') as stream:
                for block in iter(lambda: stream.read(1 << 20), b''):
                    digest.update(block)
            if digest.hexdigest() != want:
                partial.unlink()
                sys.exit(f'{name} does not match its published checksum')
            partial.rename(base)
        return base

    def seed(self):
        if not self.key.exists():
            subprocess.run(['ssh-keygen', '-q', '-t', 'ed25519', '-N', '', '-f', str(self.key)], check=True)
        folder = self.work / 'seed'
        folder.mkdir(exist_ok=True)
        public = (self.work / 'key.pub').read_text().strip()
        admins = IMAGES[self.distro]['admins']
        (folder / 'user-data').write_text(
            '#cloud-config\n'
            'users:\n'
            '  - name: tester\n'
            f'    groups: [{admins}]\n'
            '    sudo: ALL=(ALL) NOPASSWD:ALL\n'
            '    shell: /bin/bash\n'
            f'    ssh_authorized_keys: ["{public}"]\n')
        (folder / 'meta-data').write_text('instance-id: helixforge-vm\nlocal-hostname: helixforge-vm\n')
        iso = self.work / 'seed.iso'
        subprocess.run(['genisoimage', '-quiet', '-output', str(iso), '-volid', 'cidata', '-joliet', '-rock',
                        str(folder / 'user-data'), str(folder / 'meta-data')], check=True)
        return iso

    def start(self):
        base = self.fetch()
        first = True
        iso = self.seed()
        disk = self.work / 'disk.qcow2'
        disk.unlink(missing_ok=True)
        subprocess.run(['qemu-img', 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(disk), '16G'],
                       check=True)
        self.monitor.unlink(missing_ok=True)
        subprocess.run([
            'qemu-system-x86_64', '-enable-kvm', '-cpu', 'host', '-m', '4096', '-smp', '2',
            '-drive', f'file={disk},if=virtio', '-drive', f'file={iso},if=virtio,format=raw,readonly=on',
            '-nic', f'user,model=virtio-net-pci,hostfwd=tcp:127.0.0.1:{self.port}-:22', '-display', 'none', '-vga', 'std',
            '-monitor', f'unix:{self.monitor},server,nowait', '-serial', f'file:{self.work / "serial.log"}',
            '-daemonize', '-pidfile', str(self.work / 'qemu.pid')], check=True)
        # The first boot runs cloud-init, which takes Debian about two minutes.
        self.wait(patience=600 if first else 240)

    def stop(self):
        pid = self.work / 'qemu.pid'
        if pid.exists():
            subprocess.run(['kill', pid.read_text().strip()], check=False)

    # ----- talking to it -----

    def ssh(self, command, stdin=None, check=True):
        result = subprocess.run(
            ['ssh', '-q', '-i', str(self.key), '-o', 'StrictHostKeyChecking=no', '-o', 'UserKnownHostsFile=/dev/null',
             '-o', 'ConnectTimeout=5', '-o', 'ServerAliveInterval=10', '-p', str(self.port), 'tester@127.0.0.1', command],
            input=stdin, capture_output=True, text=True)
        if check and result.returncode != 0:
            raise RuntimeError(f'{command}: exit {result.returncode}\n{result.stdout[-3000:]}\n{result.stderr[-3000:]}')
        return result

    def ok(self, command):
        return self.ssh(command, check=False).returncode == 0

    def wait(self, patience=240):
        deadline = time.time() + patience
        while time.time() < deadline:
            try:
                with socket.create_connection(('127.0.0.1', self.port), timeout=3):
                    pass
                if self.ok('true'):
                    return
            except OSError:
                pass
            time.sleep(2)
        raise RuntimeError('the machine did not come back')

    def copy(self, source, target):
        subprocess.run(['scp', '-q', '-i', str(self.key), '-o', 'StrictHostKeyChecking=no', '-o',
                        'UserKnownHostsFile=/dev/null', '-P', str(self.port), str(source), f'tester@127.0.0.1:{target}'],
                       check=True)

    def fetch_file(self, source, target):
        subprocess.run(['scp', '-q', '-i', str(self.key), '-o', 'StrictHostKeyChecking=no', '-o',
                        'UserKnownHostsFile=/dev/null', '-P', str(self.port), f'tester@127.0.0.1:{source}', str(target)],
                       check=False)

    def screen(self, path):
        with socket.socket(socket.AF_UNIX) as monitor:
            monitor.connect(str(self.monitor))
            monitor.sendall(f'screendump {path}\n'.encode())
            time.sleep(0.3)

    def boot_id(self):
        return self.ssh('cat /proc/sys/kernel/random/boot_id').stdout.strip()

    def reboot(self, shots, name):
        """Reboot, capturing the screen every half second until it is back;
        returns how many colours the most colourful early frame had."""
        before = self.boot_id()
        self.ssh('sudo systemctl reboot', check=False)
        frames = []
        deadline = time.time() + 240
        index = 0
        while time.time() < deadline:
            frame = self.work / f'frame-{index:03}.ppm'
            try:
                self.screen(frame)
                frames.append(frame)
            except OSError:
                pass
            index += 1
            time.sleep(0.5)
            if index > 20 and index % 4 == 0 and self.ok('true'):
                break
        self.wait()
        if self.boot_id() == before:
            raise RuntimeError('the machine did not reboot')
        return keep_frames(frames, shots, name)

    # ----- preparation -----

    def prepare(self, binary):
        """Make the cloud image boot like a desktop installation."""
        if self.distro == 'arch':
            self.ssh('sudo pacman -Syu --noconfirm --needed git python >/dev/null')
            regenerate = 'sudo grub-mkconfig -o /boot/grub/grub.cfg'
        else:
            self.ssh('sudo apt-get update -q >/dev/null && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q git python3 >/dev/null')
            regenerate = 'sudo update-grub'
        # The cloud images send GRUB and the kernel's console to the serial
        # line, which also keeps Plymouth's splash away, and hide the menu.
        self.ssh("sudo sed -i -e '/^GRUB_TERMINAL=/d' -e '/^GRUB_SERIAL_COMMAND=/d' "
                 "-e 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=5/' -e 's/^GRUB_TIMEOUT_STYLE=.*/GRUB_TIMEOUT_STYLE=menu/' "
                 "-e '/^GRUB_CMDLINE_LINUX/s/ *console=[^ \"]*//g' "
                 "/etc/default/grub && " + regenerate + ' >/dev/null 2>&1')
        # Debian's cloud drop-ins also hide the menu and add serial consoles.
        for drop_in in self.ssh('ls /etc/default/grub.d 2>/dev/null || true').stdout.split():
            self.ssh(f"sudo sed -i -e '/GRUB_TERMINAL/d' -e '/GRUB_SERIAL/d' -e '/GRUB_TIMEOUT/d' "
                     f"-e '/^GRUB_CMDLINE_LINUX/s/ *console=[^ \"]*//g' /etc/default/grub.d/{shlex.quote(drop_in)}")
        self.ssh(regenerate + ' >/dev/null 2>&1')
        self.copy(binary, 'helixforge')
        self.ssh('chmod +x helixforge && ./helixforge --version')

    def run(self, entry, environment=''):
        """Run an entry, answering its questions as they come: the installers'
        APPLY, the package managers' [Y/n], and yes to [y/N]."""
        command = f'{environment} ./helixforge run {shlex.quote(entry)} --yes'.strip()
        process = subprocess.Popen(
            ['ssh', '-q', '-i', str(self.key), '-o', 'StrictHostKeyChecking=no', '-o', 'UserKnownHostsFile=/dev/null',
             '-o', 'ServerAliveInterval=10', '-p', str(self.port), 'tester@127.0.0.1', command],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        output = b''
        answered = 0
        deadline = time.time() + 1800
        while time.time() < deadline:
            ready, _, _ = select.select([process.stdout], [], [], 1)
            if ready:
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                output += chunk
                tail = output[answered:].decode(errors='replace')
                # pacman's provider and group choices take their default.
                for prompt, answer in (('Type APPLY to continue:', 'APPLY'), ('[Y/n]', 'y'), ('[y/N]', 'y'),
                                       ('(default=1):', ''), ('(default=all):', '')):
                    if tail.rstrip().endswith(prompt) or tail.rstrip().endswith(prompt + ' '):
                        process.stdin.write(f'{answer}\n'.encode())
                        process.stdin.flush()
                        answered = len(output)
                        break
            elif process.poll() is not None:
                break
        process.wait(timeout=30)
        text = output.decode(errors='replace')
        if process.returncode != 0:
            raise RuntimeError(f'helixforge run {entry} failed:\n{text[-4000:]}')
        return text


def colours(path):
    from PIL import Image
    image = Image.open(path).convert('RGB')
    return image.size, len(set(image.resize((64, 48)).getdata()))


def keep_frames(frames, shots, name):
    """Keep the most colourful graphical frame and the last one as PNGs;
    return that frame's colour count. Graphical: neither VGA text mode
    (720x400) nor the kernel's own console, which the last frame shows, so
    coloured boot messages never count as a boot menu."""
    from PIL import Image
    measured = []
    for frame in frames:
        try:
            size, count = colours(frame)
            measured.append((size, count, frame))
        except Exception:
            continue
    if not measured:
        return 0
    console = measured[-1][0]
    graphical = [item for item in measured if item[0] not in ((720, 400), console)]
    best = max(graphical or measured, key=lambda item: item[1])
    best = (best[1] if graphical else 0, best[2])
    measured = [(count, frame) for _, count, frame in measured]
    if shots:
        shots.mkdir(parents=True, exist_ok=True)
        Image.open(best[1]).save(shots / f'{name}-boot.png')
        Image.open(measured[-1][1]).save(shots / f'{name}-after.png')
    for frame in frames:
        frame.unlink(missing_ok=True)
    return best[0]


# ----- the cases -----

def grub_theme(machine, shots):
    machine.run('grub-dracula')
    theme = machine.ssh("sed -n 's/^GRUB_THEME=\"\\(.*\\)\"/\\1/p' /etc/default/grub").stdout.strip()
    if 'commander-dracula-' not in theme or not machine.ok(f'test -f {shlex.quote(theme)}'):
        raise RuntimeError(f'GRUB_THEME does not name the installed theme: {theme!r}')
    if not machine.ok('grep -q "^GRUB_TERMINAL_OUTPUT=\\"gfxterm\\"" /etc/default/grub'):
        raise RuntimeError('graphical output was not selected')
    shown = machine.reboot(shots, 'grub-dracula')
    if shown < 60:
        raise RuntimeError(f'no graphical boot menu was seen ({shown} colours at most)')


def grub_default(machine, shots):
    machine.run('grub-default')
    if machine.ok('grep -q "^GRUB_THEME=\\"[^\\"]" /etc/default/grub'):
        raise RuntimeError('a theme is still selected')
    machine.reboot(shots, 'grub-default')


def plymouth_on(machine, shots):
    machine.run('plymouth-on')
    machine.reboot(shots, 'plymouth-on')
    if not machine.ok('grep -qw splash /proc/cmdline'):
        raise RuntimeError('splash is not on the kernel command line after the reboot')
    # The boot images are readable by root only.
    images = {'arch': 'lsinitcpio /boot/initramfs-linux.img', 'debian': 'lsinitramfs /boot/initrd.img-$(uname -r)'}
    if not machine.ok(f'sudo {images[machine.distro]} | grep -q plymouth'):
        raise RuntimeError('the boot image has no plymouth')
    if not machine.ok('test -n "$(systemctl show -P InvocationID plymouth-start.service)"'):
        raise RuntimeError('Plymouth did not start during the boot')


def plymouth_off(machine, shots):
    machine.run('plymouth-off')
    machine.reboot(shots, 'plymouth-off')
    if machine.ok('grep -qw splash /proc/cmdline'):
        raise RuntimeError('splash is still on the kernel command line')


def apparmor(machine, shots):
    enabled = 'grep -q Y /sys/module/apparmor/parameters/enabled'
    if not machine.ok(enabled):
        # Arch: the first run sets the kernel parameter; the tools come after a reboot.
        machine.run('apparmor')
        machine.reboot(shots, 'apparmor')
        if not machine.ok(enabled):
            raise RuntimeError('AppArmor is not enabled in the kernel after the reboot')
    machine.run('apparmor')
    if not machine.ok('systemctl is-enabled apparmor && systemctl is-active apparmor'):
        raise RuntimeError('the AppArmor service is not enabled and running')


def sddm(machine, shots):
    # SDDM, and a display for the installer's preview of the theme.
    install = {'arch': 'sudo pacman -S --noconfirm --needed sddm xorg-server-xvfb imagemagick >/dev/null',
               'debian': 'sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q sddm xvfb imagemagick >/dev/null'}
    machine.ssh(install[machine.distro])
    machine.ssh('setsid nohup Xvfb :99 -screen 0 1280x800x24 >/dev/null 2>&1 < /dev/null &')
    # The preview waits for its window to close. In front of the real greeter
    # on the PATH: run it, picture the display after 20 s, then end it the way
    # a closed window does. A preview that dies before then still fails.
    machine.ssh("sudo tee /usr/local/bin/sddm-greeter-qt6 >/dev/null <<'GREETER'\n"
                "#!/bin/sh\n"
                "/usr/bin/sddm-greeter-qt6 \"$@\" &\n"
                "pid=$!\n"
                "sleep 20\n"
                "kill -0 $pid 2>/dev/null || { wait $pid; exit $?; }\n"
                "import -display \"$DISPLAY\" -window root /tmp/preview.png 2>/dev/null || true\n"
                "echo shown > /tmp/preview\n"
                "kill $pid; wait $pid 2>/dev/null\n"
                "exit 0\n"
                "GREETER\n"
                "sudo chmod 755 /usr/local/bin/sddm-greeter-qt6")
    machine.run('sddm', environment='DISPLAY=:99')
    if not machine.ok('test -f /tmp/preview'):
        raise RuntimeError('the theme preview did not stay up')
    if shots and machine.ok('test -f /tmp/preview.png'):
        shots.mkdir(parents=True, exist_ok=True)
        machine.fetch_file('/tmp/preview.png', shots / 'sddm-preview.png')
    if not machine.ok('grep -q "^Current=silent" /etc/sddm.conf.d/zz-commander-theme.conf'):
        raise RuntimeError('the silent theme is not selected')
    if not machine.ok('test -f /usr/share/sddm/themes/silent/metadata.desktop'):
        raise RuntimeError('the theme is not installed')
    machine.reboot(shots, 'sddm')
    for _ in range(30):
        if machine.ok('systemctl is-active sddm'):
            break
        time.sleep(2)
    else:
        raise RuntimeError('SDDM is not running after the reboot')
    time.sleep(10)
    if shots:
        machine.screen(machine.work / 'greeter.ppm')
        size, count = colours(machine.work / 'greeter.ppm')
        from PIL import Image
        Image.open(machine.work / 'greeter.ppm').save(shots / 'sddm-greeter.png')
        if count < 60:
            raise RuntimeError(f'no graphical login screen was seen ({count} colours)')


CASES = {
    'arch': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
             ('plymouth-off', plymouth_off), ('apparmor', apparmor), ('sddm', sddm)],
    'debian': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
               ('plymouth-off', plymouth_off), ('apparmor', apparmor), ('sddm', sddm)],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--distro', choices=sorted(IMAGES), required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--shots', type=Path)
    parser.add_argument('--only', default='')
    parser.add_argument('--keep', action='store_true', help='leave the machine running, to look inside')
    parser.add_argument('--port', type=int, default=PORT, help='the local port for its SSH')
    options = parser.parse_args()
    options.work.mkdir(parents=True, exist_ok=True)
    only = [item for item in options.only.split(',') if item]
    machine = Machine(options.work, options.distro, options.port)
    entries, failures = {}, {}
    try:
        machine.start()
        machine.prepare(options.binary)
        for entry, case in CASES[options.distro]:
            if only and entry not in only:
                continue
            log(f'{entry}: running')
            started = time.time()
            try:
                case(machine, options.shots)
                entries[entry] = 'verified'
                log(f'{entry}: verified in {time.time() - started:.0f} s')
            except Exception as error:  # every case is reported, then the run fails
                failures[entry] = str(error)[-2000:]
                log(f'{entry}: FAILED: {error}')
                try:
                    machine.wait(patience=60)
                except RuntimeError:
                    log('the machine is gone; the remaining cases cannot run')
                    break
    finally:
        if not options.keep:
            machine.stop()
    options.out.write_text(json.dumps({'image': f'vm:{options.distro}', 'entries': entries, 'failures': failures},
                                      indent=1) + '\n')
    sys.exit(1 if failures else 0)


if __name__ == '__main__':
    main()
