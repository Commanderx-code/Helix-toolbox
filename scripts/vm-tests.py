#!/usr/bin/env python3
"""Run Helix Toolbox's boot, login-screen, disk and kernel entries in a virtual machine.

  vm-tests.py --distro MACHINE --binary PATH --work DIR --out RESULT.json
              [--shots DIR] [--only ID,ID]

MACHINE is one of MACHINES below: a distribution's cloud image on BIOS or
UEFI firmware, the Arch live ISO with an empty disk, or an Arch machine that
says it is a Steam Deck. Boots it under QEMU with KVM, makes it boot like a
desktop installation (a user in the administrators' group, GRUB on the
screen with a visible menu), copies Helix Toolbox in, and for each case: runs
the entry through `helix-toolbox run`, answering its questions, reboots,
captures the screen while it boots, and checks what the entry should have
done (its configuration, the kernel command line, the boot images, services)
and that the machine came back. The result lists each entry as `verified` or
`failed` with why, as the reliability record expects; any failure makes the
script exit 1. Nothing here touches the machine running it beyond the work
folder.
"""
import argparse
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import platform
import re
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
    'fedora': {
        'url': 'https://download.fedoraproject.org/pub/fedora/linux/releases/44/Cloud/x86_64/images/'
               'Fedora-Cloud-Base-Generic-44-1.7.x86_64.qcow2',
        'sums': 'https://download.fedoraproject.org/pub/fedora/linux/releases/44/Cloud/x86_64/images/'
                'Fedora-Cloud-44-1.7-x86_64-CHECKSUM',
        'hash': 'sha256',
        'admins': 'wheel',
    },
    'opensuse': {
        'url': 'https://download.opensuse.org/tumbleweed/appliances/openSUSE-Tumbleweed-Minimal-VM.x86_64-Cloud.qcow2',
        'sums': 'https://download.opensuse.org/tumbleweed/appliances/'
                'openSUSE-Tumbleweed-Minimal-VM.x86_64-Cloud.qcow2.sha256',
        'hash': 'sha256',
        'admins': 'wheel',
    },
    # The same distributions for ARM64.
    'debian-arm64': {
        'url': 'https://cloud.debian.org/images/cloud/trixie/latest/debian-13-genericcloud-arm64.qcow2',
        'sums': 'https://cloud.debian.org/images/cloud/trixie/latest/SHA512SUMS',
        'hash': 'sha512',
        'admins': 'sudo',
    },
    'fedora-arm64': {
        'url': 'https://download.fedoraproject.org/pub/fedora/linux/releases/44/Cloud/aarch64/images/'
               'Fedora-Cloud-Base-Generic-44-1.7.aarch64.qcow2',
        'sums': 'https://download.fedoraproject.org/pub/fedora/linux/releases/44/Cloud/aarch64/images/'
                'Fedora-Cloud-44-1.7-aarch64-CHECKSUM',
        'hash': 'sha256',
        'admins': 'wheel',
    },
    'archiso': {
        'url': 'https://geo.mirror.pkgbuild.com/iso/latest/archlinux-x86_64.iso',
        'sums': 'https://geo.mirror.pkgbuild.com/iso/latest/sha256sums.txt',
        'hash': 'sha256',
    },
}

# The machines: an image, its firmware, and what else is plugged in. `family`
# is the distribution the cases check against.
MACHINES = {
    'arch': {'image': 'arch', 'family': 'arch', 'firmware': 'bios', 'usb': True},
    'debian': {'image': 'debian', 'family': 'debian', 'firmware': 'bios'},
    'fedora': {'image': 'fedora', 'family': 'fedora', 'firmware': 'bios'},
    'opensuse': {'image': 'opensuse', 'family': 'opensuse', 'firmware': 'bios'},
    'arch-themes': {'image': 'arch', 'family': 'arch', 'firmware': 'bios'},
    'arch-ctt-themes': {'image': 'arch', 'family': 'arch', 'firmware': 'bios'},
    'arch-bundle': {'image': 'arch', 'family': 'arch', 'firmware': 'bios'},
    'arch-uefi': {'image': 'arch', 'family': 'arch', 'firmware': 'uefi'},
    # The live ISO, as root, with an empty disk to install on.
    'arch-iso': {'image': 'archiso', 'family': 'arch', 'firmware': 'uefi', 'live': True, 'chipset': 'q35'},
    # Its firmware names Valve as the board's maker, as a Steam Deck's does.
    'steamdeck': {'image': 'arch', 'family': 'arch', 'firmware': 'bios', 'smbios': 'type=2,manufacturer=Valve'},
    # ARM64 machines, which boot through UEFI.
    # Debian's ARM kernel drives the UEFI framebuffer with efifb, which makes no
    # /dev/dri for SDDM to wait on; its own kernel has the virtio GPU driver.
    'debian-arm64': {'image': 'debian-arm64', 'family': 'debian', 'firmware': 'uefi', 'cpu': 'aarch64',
                     'display': 'virtio-gpu-pci'},
    'fedora-arm64': {'image': 'fedora-arm64', 'family': 'fedora', 'firmware': 'uefi', 'cpu': 'aarch64'},
}

# UEFI firmware, where Debian, Ubuntu, Fedora and Arch put it.
OVMF = [
    ('/usr/share/OVMF/OVMF_CODE_4M.fd', '/usr/share/OVMF/OVMF_VARS_4M.fd'),
    ('/usr/share/edk2/x64/OVMF_CODE.4m.fd', '/usr/share/edk2/x64/OVMF_VARS.4m.fd'),
    ('/usr/share/edk2/ovmf/OVMF_CODE.fd', '/usr/share/edk2/ovmf/OVMF_VARS.fd'),
]

# ARM64's UEFI firmware, where Debian and Ubuntu, then Fedora, put it.
AAVMF = [
    ('/usr/share/AAVMF/AAVMF_CODE.fd', '/usr/share/AAVMF/AAVMF_VARS.fd'),
    ('/usr/share/edk2/aarch64/QEMU_EFI-pflash.raw', '/usr/share/edk2/aarch64/vars-template-pflash.raw'),
]

PORT = 2222


def log(message):
    print(f'vm-tests: {message}', flush=True)


def firmware_options(kind, vars_file, cpu='x86_64'):
    """QEMU's options for BIOS or UEFI firmware, UEFI's variables kept in
    `vars_file` from one start to the next."""
    if kind != 'uefi':
        return []
    for code, template in AAVMF if cpu == 'aarch64' else OVMF:
        if Path(code).exists() and Path(template).exists():
            if not vars_file.exists():
                vars_file.write_bytes(Path(template).read_bytes())
            return ['-drive', f'if=pflash,format=raw,readonly=on,file={code}',
                    '-drive', f'if=pflash,format=raw,file={vars_file}']
    sys.exit('no UEFI firmware (OVMF or AAVMF) is installed')


ANSI = re.compile(r'\x1b\[[0-9;?]*[A-Za-z]|\x1b[()][A-Z0-9]|\r')


def ssh_options(key):
    return ['-q', '-i', str(key), '-o', 'StrictHostKeyChecking=no', '-o', 'UserKnownHostsFile=/dev/null']


class Machine:
    def __init__(self, work, name, port=PORT):
        self.port = port
        self.work = work
        self.name = name
        self.spec = MACHINES[name]
        self.distro = self.spec['family']
        self.live = self.spec.get('live', False)
        self.user = 'root' if self.live else 'tester'
        self.home = '/root' if self.live else '/home/tester'
        self.key = work / 'key'
        self.disk = work / 'disk.qcow2'
        self.vars = work / 'vars.fd'
        self.cpu = self.spec.get('cpu', 'x86_64')
        # Another processor than this one's is emulated, several times slower:
        # everything is given that much longer.
        self.native = self.cpu == platform.machine() and Path('/dev/kvm').exists()
        self.pace = 1 if self.native else 6
        # Unix socket paths are short: a private folder of its own in /tmp.
        self.monitor = Path(tempfile.mkdtemp(prefix='hf-vm-', dir='/tmp')) / 'monitor'

    # ----- the image and the machine -----

    def fetch(self):
        image = IMAGES[self.spec['image']]
        name = image['url'].rsplit('/', 1)[1]
        base = self.work / name
        if not base.exists():
            log(f'downloading {image["url"]}')
            partial = base.with_name(name + '.part')
            urllib.request.urlretrieve(image['url'], partial)
            sums = urllib.request.urlopen(image['sums'], timeout=60).read().decode()
            want = None
            for line in sums.splitlines():
                # "HASH  NAME", "HASH *NAME", or Fedora's "SHA256 (NAME) = HASH".
                tagged = re.fullmatch(r'\w+ \((.+)\) = ([0-9a-f]+)', line.strip())
                if tagged and tagged.group(1) == name:
                    want = tagged.group(2)
                elif line.split() and line.split()[-1].lstrip('*') == name:
                    want = line.split()[0]
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
        if self.live:
            # The live ISO logs in as root and has no sudo.
            user_data = ('#cloud-config\n'
                         'write_files:\n'
                         '  - path: /root/.ssh/authorized_keys\n'
                         '    permissions: "0600"\n'
                         f'    content: "{public}\\n"\n')
        else:
            user_data = ('#cloud-config\n'
                         'users:\n'
                         '  - name: tester\n'
                         f'    groups: [{IMAGES[self.spec["image"]]["admins"]}]\n'
                         '    sudo: ALL=(ALL) NOPASSWD:ALL\n'
                         '    shell: /bin/bash\n'
                         f'    ssh_authorized_keys: ["{public}"]\n')
        (folder / 'user-data').write_text(user_data)
        (folder / 'meta-data').write_text('instance-id: helix-toolbox-vm\nlocal-hostname: helix-toolbox-vm\n')
        iso = self.work / 'seed.iso'
        subprocess.run(['genisoimage', '-quiet', '-output', str(iso), '-volid', 'cidata', '-joliet', '-rock',
                        str(folder / 'user-data'), str(folder / 'meta-data')], check=True)
        return iso

    def launch(self, drives, serial='serial.log', network=True):
        self.monitor.unlink(missing_ok=True)
        self.serial = self.work / serial
        if self.cpu == 'aarch64':
            # QEMU's generic ARM board. Its screen is a plain framebuffer in
            # memory unless the machine names another, which UEFI hands to any
            # kernel; a virtio GPU's goes dark unless the kernel has that driver,
            # and cloud kernels do not.
            processor = (['-enable-kvm', '-cpu', 'host', '-smp', '2'] if self.native else
                         ['-accel', 'tcg,thread=multi', '-cpu', 'max,pauth-impdef=on', '-smp', '2'])
            command = ['qemu-system-aarch64', '-machine', 'virt', *processor,
                       '-device', self.spec.get('display', 'ramfb')]
        else:
            # q35: no floppy drive, which the installer's disk menu would list first.
            command = ['qemu-system-x86_64', '-enable-kvm', '-machine', self.spec.get('chipset', 'pc'), '-cpu', 'host',
                       '-smp', '2', '-vga', 'std']
        command += ['-m', '4096', *firmware_options(self.spec['firmware'], self.vars, self.cpu), *drives,
                    '-display', 'none', '-monitor', f'unix:{self.monitor},server,nowait',
                    '-serial', f'file:{self.work / serial}', '-daemonize', '-pidfile', str(self.work / 'qemu.pid')]
        if network:
            # Restricted: SSH still reaches it, but it reaches nothing.
            restrict = ',restrict=on' if network == 'restricted' else ''
            command += ['-nic', f'user,model=virtio-net-pci{restrict},hostfwd=tcp:127.0.0.1:{self.port}-:22']
        else:
            command += ['-nic', 'none']
        if self.spec.get('smbios'):
            command += ['-smbios', self.spec['smbios']]
        if self.spec.get('usb'):
            # A USB stick, for the bootable USB creator.
            usb = self.work / 'usb.img'
            command += ['-device', 'qemu-xhci', '-drive', f'if=none,id=stick,format=raw,file={usb}',
                        '-device', 'usb-storage,drive=stick']
        subprocess.run(command, check=True)

    def start(self):
        base = self.fetch()
        iso = self.seed()
        self.disk.unlink(missing_ok=True)
        self.vars.unlink(missing_ok=True)
        if self.spec.get('usb'):
            (self.work / 'usb.img').unlink(missing_ok=True)
            subprocess.run(['qemu-img', 'create', '-q', '-f', 'raw', str(self.work / 'usb.img'), '64M'], check=True)
        if self.live:
            subprocess.run(['qemu-img', 'create', '-q', '-f', 'qcow2', str(self.disk), '20G'], check=True)
            drives = ['-drive', f'file={self.disk},if=virtio,discard=unmap',
                      '-drive', f'file={base},media=cdrom,if=ide,index=0,readonly=on',
                      '-drive', f'file={iso},media=cdrom,if=ide,index=1,readonly=on']
        else:
            subprocess.run(['qemu-img', 'create', '-q', '-f', 'qcow2', '-F', 'qcow2', '-b', str(base), str(self.disk),
                            '16G'], check=True)
            # discard=unmap: the disk takes TRIM, as an SSD does.
            drives = ['-drive', f'file={self.disk},if=virtio,discard=unmap',
                      '-drive', f'file={iso},if=virtio,format=raw,readonly=on']
        self.drives = drives
        self.launch(drives)
        # The first boot runs cloud-init, which takes Debian about two minutes.
        self.wait(patience=600)

    def relaunch(self, network=True):
        """Power off and start again, with the network or without."""
        # Arch starts SSH only after cloud-init's network stage and a clock
        # synchronized over the network, neither of which comes without a
        # way out: neither while there is none.
        if network == 'restricted':
            self.ssh('sudo touch /etc/cloud/cloud-init.disabled && '
                     'sudo systemctl mask --quiet systemd-time-wait-sync.service')
        else:
            self.ssh('sudo rm -f /etc/cloud/cloud-init.disabled && '
                     'sudo systemctl unmask --quiet systemd-time-wait-sync.service')
        self.ssh('sudo systemctl poweroff', check=False)
        self.gone()
        self.launch(self.drives, network=network)
        self.wait()

    def stop(self):
        pid = self.work / 'qemu.pid'
        if pid.exists():
            subprocess.run(['kill', pid.read_text().strip()], check=False)

    def gone(self, patience=120):
        """Wait for QEMU to end, after a power-off."""
        pid = self.work / 'qemu.pid'
        deadline = time.time() + patience * self.pace
        while time.time() < deadline:
            if not pid.exists() or not Path(f'/proc/{pid.read_text().strip()}').exists():
                return
            time.sleep(1)
        self.stop()
        time.sleep(2)

    # ----- talking to it -----

    def ssh(self, command, stdin=None, check=True):
        result = subprocess.run(
            ['ssh', *ssh_options(self.key), '-o', 'ConnectTimeout=5', '-o', 'ServerAliveInterval=10',
             '-p', str(self.port), f'{self.user}@127.0.0.1', command],
            input=stdin, capture_output=True, text=True)
        if check and result.returncode != 0:
            raise RuntimeError(f'{command}: exit {result.returncode}\n{result.stdout[-3000:]}\n{result.stderr[-3000:]}')
        return result

    def ok(self, command):
        return self.ssh(command, check=False).returncode == 0

    def wait(self, patience=240):
        deadline = time.time() + patience * self.pace
        while time.time() < deadline:
            try:
                with socket.create_connection(('127.0.0.1', self.port), timeout=3):
                    pass
                if self.ok('true'):
                    return
            except OSError:
                pass
            time.sleep(2)
        # What it printed on its serial line, to say where it stopped.
        serial = getattr(self, 'serial', None)
        printed = serial.read_text(errors='replace') if serial and serial.exists() else ''
        tail = '\n'.join(ANSI.sub('', printed).splitlines()[-40:])
        raise RuntimeError('the machine did not come back' + (f'; its serial line ended:\n{tail}' if tail else ''))

    def copy(self, source, target):
        subprocess.run(['scp', *ssh_options(self.key), '-P', str(self.port), str(source),
                        f'{self.user}@127.0.0.1:{target}'], check=True)

    def fetch_file(self, source, target):
        subprocess.run(['scp', *ssh_options(self.key), '-P', str(self.port), f'{self.user}@127.0.0.1:{source}',
                        str(target)], check=False)

    def screen(self, path):
        with socket.socket(socket.AF_UNIX) as monitor:
            monitor.connect(str(self.monitor))
            monitor.sendall(f'screendump {path}\n'.encode())
            time.sleep(0.3)

    def boot_id(self):
        return self.ssh('cat /proc/sys/kernel/random/boot_id').stdout.strip()

    def watch(self, done, patience=240):
        """Capture the screen every half second until `done()` holds; the
        frames, oldest first. `done()` runs beside the captures: an SSH probe
        of a machine still booting can take seconds, as long as a boot menu
        stays up."""
        frames = []
        deadline = time.time() + patience * self.pace
        index = 0
        checking = None
        with concurrent.futures.ThreadPoolExecutor(1) as pool:
            while time.time() < deadline:
                frame = self.work / f'frame-{index:03}.ppm'
                try:
                    self.screen(frame)
                    frames.append(frame)
                except OSError:
                    pass
                index += 1
                time.sleep(0.5)
                if checking and checking.done():
                    if checking.result():
                        break
                    checking = None
                if checking is None and index > 20:
                    checking = pool.submit(done)
        return frames

    def reboot(self, shots, name):
        """Reboot, capturing the screen every half second until it is back;
        returns how many colors the most colorful early frame had."""
        before = self.boot_id()
        self.ssh('sudo systemctl reboot', check=False)
        frames = self.watch(lambda: self.ok('true'))
        self.wait()
        if self.boot_id() == before:
            raise RuntimeError('the machine did not reboot')
        return keep_frames(frames, shots, name, self.spec['firmware'])

    # ----- preparation -----

    def prepare(self, binary):
        """Make the cloud image boot like a desktop installation."""
        if self.live:
            self.copy(binary, 'helix-toolbox')
            self.ssh('chmod +x helix-toolbox && ./helix-toolbox --version')
            return
        if self.distro == 'arch':
            self.ssh('sudo pacman -Syu --noconfirm --needed git python >/dev/null')
            regenerate = 'sudo grub-mkconfig -o /boot/grub/grub.cfg'
        elif self.distro == 'fedora':
            self.ssh('sudo dnf install -y -q git python3 >/dev/null')
            regenerate = 'sudo grub2-mkconfig -o /boot/grub2/grub.cfg'
        elif self.distro == 'opensuse':
            self.ssh('sudo zypper -n -q install git python3 >/dev/null')
            regenerate = 'sudo grub2-mkconfig -o /boot/grub2/grub.cfg'
        else:
            self.ssh('sudo apt-get update -q >/dev/null && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q git python3 >/dev/null')
            regenerate = 'sudo update-grub'
            if self.cpu == 'aarch64':
                # Debian's cloud kernel for ARM64 has no graphics driver at
                # all, so no login screen can start; a desktop has the
                # distribution's own kernel.
                self.ssh('sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q linux-image-arm64 >/dev/null && '
                         "sudo DEBIAN_FRONTEND=noninteractive apt-get purge -y -q 'linux-image-*cloud*' >/dev/null")
                before = self.boot_id()
                self.ssh('sudo systemctl reboot', check=False)
                time.sleep(10)
                self.wait()
                if self.boot_id() == before or 'cloud' in self.ssh('uname -r').stdout:
                    raise RuntimeError('the machine did not start its new kernel')
        # The cloud images send GRUB and the kernel's console to the serial
        # line, which also keeps Plymouth's splash away, and hide the menu.
        self.ssh("sudo sed -i -e '/^GRUB_TERMINAL\\(_INPUT\\|_OUTPUT\\)\\{0,1\\}=/d' -e '/^GRUB_SERIAL_COMMAND=/d' "
                 "-e 's/^GRUB_TIMEOUT=.*/GRUB_TIMEOUT=5/' -e 's/^GRUB_TIMEOUT_STYLE=.*/GRUB_TIMEOUT_STYLE=menu/' "
                 "-e '/^GRUB_CMDLINE_LINUX/s/ *console=[^ \"]*//g' "
                 "/etc/default/grub && " + regenerate + ' >/dev/null 2>&1')
        # Debian's cloud drop-ins also hide the menu and add serial consoles.
        for drop_in in self.ssh('ls /etc/default/grub.d 2>/dev/null || true').stdout.split():
            self.ssh(f"sudo sed -i -e '/GRUB_TERMINAL/d' -e '/GRUB_SERIAL/d' -e '/GRUB_TIMEOUT/d' "
                     f"-e '/^GRUB_CMDLINE_LINUX/s/ *console=[^ \"]*//g' /etc/default/grub.d/{shlex.quote(drop_in)}")
        if self.distro == 'fedora':
            # Fedora keeps each kernel's arguments in its boot entry, and
            # hides the menu until a boot fails.
            self.ssh("for arg in $(tr ' ' '\\n' < /proc/cmdline | grep '^console='); do "
                     "sudo grubby --update-kernel=ALL --remove-args=\"$arg\"; done; "
                     "sudo grub2-editenv - unset menu_auto_hide")
        self.ssh(regenerate + ' >/dev/null 2>&1')
        # Fedora's cloud image starts no graphical session, so no login screen.
        self.ssh('sudo systemctl set-default graphical.target')
        self.copy(binary, 'helix-toolbox')
        self.ssh('chmod +x helix-toolbox && ./helix-toolbox --version')

    def run(self, entry, environment='', answers=()):
        """Run an entry, answering its questions as they come: `answers`
        first, as (text or pattern, answer) pairs matched at the end of what
        it has printed since the last answer, then the installers' APPLY, the
        package managers' [Y/n] and yes to [y/N] (dnf's end with a colon)."""
        # A terminal names itself.
        command = f'TERM=xterm-256color {environment} ./helix-toolbox run {shlex.quote(entry)} --yes'
        # In a terminal, as people run it: bash shows read -p's prompts only
        # to one.
        process = subprocess.Popen(
            ['ssh', '-tt', *ssh_options(self.key), '-o', 'ServerAliveInterval=10', '-p', str(self.port),
             f'{self.user}@127.0.0.1', command],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        # pacman's provider and group choices take their default.
        known = [(prompt.pattern if isinstance(prompt, re.Pattern) else re.escape(prompt), answer)
                 for prompt, answer in answers] + [
            (r'Type APPLY to continue:', 'APPLY'), (r'\[Y/n\]:?', 'y'), (r'\[y/N\]:?', 'y'),
            (r'\(default=1\):', ''), (r'\(default=all\):', '')]
        output = b''
        answered = 0
        deadline = time.time() + 2400 * self.pace
        while time.time() < deadline:
            ready, _, _ = select.select([process.stdout], [], [], 1)
            if ready:
                chunk = os.read(process.stdout.fileno(), 65536)
                if not chunk:
                    break
                output += chunk
                tail = ANSI.sub('', output[answered:].decode(errors='replace')).rstrip(' ')
                for pattern, answer in known:
                    if re.search(f'(?:{pattern})\\s*$', tail):
                        process.stdin.write(f'{answer}\n'.encode())
                        process.stdin.flush()
                        answered = len(output)
                        break
            elif process.poll() is not None:
                break
        process.wait(timeout=30)
        text = output.decode(errors='replace')
        if process.returncode != 0:
            raise RuntimeError(f'helix-toolbox run {entry} failed:\n{text[-4000:]}')
        return text


def pixels(image):
    # Pillow 12 renamed getdata; the runners' Pillow is older.
    return list(getattr(image, 'get_flattened_data', image.getdata)())


def colors(path):
    from PIL import Image
    image = Image.open(path).convert('RGB')
    return image.size, len(set(pixels(image.resize((64, 48)))))


def covered(path):
    """How much of the screen is not black: a themed menu's picture covers
    it, text on a console does not."""
    from PIL import Image
    every = pixels(Image.open(path).convert('RGB').resize((128, 96)))
    return sum(1 for pixel in every if sum(pixel) > 60) / len(every)


def keep_frames(frames, shots, name, firmware='bios'):
    """Keep the most colorful graphical frame and the last one as PNGs;
    return that frame's color count. Graphical, on BIOS: neither VGA text
    mode (720x400) nor the kernel's own console, which the last frame shows,
    so colored boot messages never count as a boot menu. UEFI shows the menu
    and the console at one size: there, mostly covered by a picture, which
    dark themes are not."""
    from PIL import Image
    measured = []
    for frame in frames:
        try:
            size, count = colors(frame)
            measured.append((size, count, covered(frame), frame))
        except Exception:
            continue
    if not measured:
        return 0
    console = measured[-1][0]
    graphical = [item for item in measured
                 if (item[2] >= 0.5 if firmware == 'uefi' else item[0] not in ((720, 400), console))]
    best = max(graphical or measured, key=lambda item: item[1])
    if shots:
        shots.mkdir(parents=True, exist_ok=True)
        Image.open(best[3]).save(shots / f'{name}-boot.png')
        Image.open(measured[-1][3]).save(shots / f'{name}-after.png')
        if not graphical:
            # Nothing graphical: a spread of what was on the screen, to look at.
            step = max(1, len(measured) // 8)
            for number, item in enumerate(measured[::step][:8]):
                Image.open(item[3]).save(shots / f'{name}-frame-{number}.png')
    for frame in frames:
        frame.unlink(missing_ok=True)
    return best[1] if graphical else 0


# ----- the cases -----

def garuda_repository(machine):
    """Garuda's own repository, as a Garuda machine has it: its packages are
    signed with the key chaotic-keyring carries, installed as Chaotic-AUR
    documents."""
    if machine.ok('grep -q "^\\[garuda\\]" /etc/pacman.conf'):
        return
    machine.ssh('sudo pacman-key --recv-key 3056513887B78AEB --keyserver keyserver.ubuntu.com >/dev/null 2>&1 && '
                'sudo pacman-key --lsign-key 3056513887B78AEB >/dev/null && '
                "sudo pacman -U --noconfirm 'https://cdn-mirror.chaotic.cx/chaotic-aur/chaotic-keyring.pkg.tar.zst' "
                '>/dev/null')
    machine.ssh("printf '\\n[garuda]\\nServer = https://builds.garudalinux.org/repos/$repo/$arch\\n' "
                '| sudo tee -a /etc/pacman.conf >/dev/null && sudo pacman -Sy >/dev/null')


def grub_theme(machine, shots, entry='grub-dracula'):
    """A GRUB theme entry: grub-NAME installs the NAME theme as
    commander-NAME-*; grub, the Garuda theme from its package."""
    if entry == 'grub':
        garuda_repository(machine)
        wanted = '/usr/share/grub/themes/garuda-dr460nized/theme.txt'
    else:
        wanted = f'/themes/commander-{entry.removeprefix("grub-")}-'
    # Tela and Vimix ask for the size of their pictures: the default.
    machine.run(entry, answers=[(re.compile(r'3\) 4K'), '')])
    theme = machine.ssh("sed -n 's/^GRUB_THEME=\"\\(.*\\)\"/\\1/p' /etc/default/grub").stdout.strip()
    if wanted not in theme or not machine.ok(f'sudo test -f {shlex.quote(theme)}'):
        raise RuntimeError(f'GRUB_THEME does not name the installed theme: {theme!r}')
    if not machine.ok('grep -q "^GRUB_TERMINAL_OUTPUT=\\"gfxterm\\"" /etc/default/grub'):
        raise RuntimeError('graphical output was not selected')
    shown = machine.reboot(shots, entry)
    if shown < 60:
        raise RuntimeError(f'no graphical boot menu was seen ({shown} colors at most)')


def grub_theme_case(entry):
    return lambda machine, shots: grub_theme(machine, shots, entry)


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
    images = {'arch': 'lsinitcpio /boot/initramfs-linux.img', 'debian': 'lsinitramfs /boot/initrd.img-$(uname -r)',
              'fedora': 'lsinitrd'}
    if not machine.ok(f'sudo {images[machine.distro]} | grep -q plymouth'):
        raise RuntimeError('the boot image has no plymouth')
    if not machine.ok('test -n "$(systemctl show -P InvocationID plymouth-start.service)"'):
        raise RuntimeError('Plymouth did not start during the boot')


def plymouth_off(machine, shots):
    machine.run('plymouth-off')
    machine.reboot(shots, 'plymouth-off')
    if machine.ok('grep -qw splash /proc/cmdline'):
        raise RuntimeError('splash is still on the kernel command line')


def splash_in_boot_image(machine, theme):
    """The theme is selected, in the boot image, and Plymouth started at the
    next boot."""
    if machine.ssh('plymouth-set-default-theme').stdout.strip() != theme:
        raise RuntimeError(f'{theme} is not the selected boot splash')
    images = {'arch': 'lsinitcpio /boot/initramfs-linux.img', 'debian': 'lsinitramfs /boot/initrd.img-$(uname -r)',
              'fedora': 'lsinitrd'}
    if not machine.ok(f'sudo {images[machine.distro]} | grep -q "plymouth/themes/{theme}/"'):
        raise RuntimeError(f'the boot image does not have the {theme} theme')


def plymouth_theme(machine, shots):
    # The splash is on (plymouth-on ran before); pick another installed theme.
    themes = machine.ssh('plymouth-set-default-theme --list').stdout.split()
    current = machine.ssh('plymouth-set-default-theme').stdout.strip()
    choice = next((theme for theme in ('spinfinity', 'solar', 'glow', 'fade-in', 'tribar', 'spinner')
                   if theme in themes and theme != current), None)
    if not choice:
        raise RuntimeError(f'no other graphical theme is installed: {themes}')
    machine.run('plymouth-theme', answers=[('Theme number:', str(themes.index(choice) + 1))])
    splash_in_boot_image(machine, choice)
    machine.reboot(shots, 'plymouth-theme')
    if not machine.ok('test -n "$(systemctl show -P InvocationID plymouth-start.service)"'):
        raise RuntimeError('Plymouth did not start during the boot')


def plymouth_dotfiles(machine, shots):
    """The theme dotfiles names comes from the KDE Store: without it the entry
    says where to get it and changes nothing; with a theme of that name
    installed, as the store's Get New would, it selects it."""
    theme = 'garuda-mac-style-purple'
    before = machine.ssh('plymouth-set-default-theme').stdout.strip()
    try:
        machine.run('plymouth-dotfiles')
    except RuntimeError as error:
        if 'KDE Store' not in str(error):
            raise
    else:
        raise RuntimeError(f'it ran without the {theme} theme installed')
    if machine.ssh('plymouth-set-default-theme').stdout.strip() != before:
        raise RuntimeError('the boot splash changed although the theme was missing')
    # A copy of the spinner theme under that name.
    themes = '/usr/share/plymouth/themes'
    machine.ssh(f'sudo cp -r {themes}/spinner {themes}/{theme} && cd {themes}/{theme} && '
                f'sudo mv spinner.plymouth {theme}.plymouth && sudo sed -i "s/spinner/{theme}/g" {theme}.plymouth')
    machine.run('plymouth-dotfiles')
    splash_in_boot_image(machine, theme)
    machine.reboot(shots, 'plymouth-dotfiles')
    if not machine.ok('test -n "$(systemctl show -P InvocationID plymouth-start.service)"'):
        raise RuntimeError('Plymouth did not start during the boot')


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
    # SDDM, and a display for the installer's preview of the theme. Debian's
    # SDDM takes any X server, and Xvfb counts: name the real one.
    install = {'arch': 'sudo pacman -S --noconfirm --needed sddm xorg-server-xvfb imagemagick >/dev/null',
               'debian': 'sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -q sddm xserver-xorg xvfb imagemagick >/dev/null',
               'fedora': 'sudo dnf install -y -q sddm xorg-x11-server-Xvfb ImageMagick >/dev/null'}
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
    for _ in range(30 * machine.pace):
        if machine.ok('systemctl is-active sddm'):
            break
        time.sleep(2)
    else:
        raise RuntimeError('SDDM is not running after the reboot')
    # The greeter draws in software, which emulation slows down.
    time.sleep(10 * machine.pace)
    if shots:
        machine.screen(machine.work / 'greeter.ppm')
        size, count = colors(machine.work / 'greeter.ppm')
        from PIL import Image
        Image.open(machine.work / 'greeter.ppm').save(shots / 'sddm-greeter.png')
        if count < 60:
            # What SDDM and the display said, to tell why.
            said = machine.ssh('ls /dev/dri; ls -l /usr/bin/X; sudo journalctl -b -u sddm --no-pager | tail -n 15',
                               check=False).stdout
            raise RuntimeError(f'no graphical login screen was seen ({count} colors)\n{said}')


def ssd_trim(machine, shots):
    # The machine's disk takes TRIM (discard=unmap), as an SSD does.
    output = ANSI.sub('', machine.run('enable-ssd-trim'))
    if not re.search(r'^/: .* trimmed on ', output, re.M):
        raise RuntimeError(f'the first trim did not trim the root file system:\n{output[-2000:]}')
    if not machine.ok('systemctl is-enabled fstrim.timer && systemctl is-active fstrim.timer'):
        raise RuntimeError('fstrim.timer is not enabled and running')
    machine.reboot(shots, 'enable-ssd-trim')
    if not machine.ok('systemctl is-active fstrim.timer'):
        raise RuntimeError('fstrim.timer is not running after the reboot')
    if not machine.ok('systemctl list-timers fstrim.timer --no-legend | grep -q fstrim.service'):
        raise RuntimeError('no trim is scheduled')


# UEFI firmware copies the console to the serial line itself; GRUB taking
# the line over there stalled the first boot.
USB_MENU = """if [ "$grub_platform" = pc ]; then
    serial --unit=0 --speed=115200
    terminal_output --append serial
fi
echo hf-usb-booted
sleep 600
"""


def bootable_usb(machine, shots):
    # A small image that boots on BIOS and UEFI and says so on the serial line.
    machine.ssh('sudo pacman -S --noconfirm --needed libisoburn mtools >/dev/null')
    machine.ssh('mkdir -p usb-test/boot/grub && cat > usb-test/boot/grub/grub.cfg', stdin=USB_MENU)
    machine.ssh('grub-mkrescue -o hf-test.iso usb-test >/dev/null 2>&1')
    machine.run('bootable-usb-creator', answers=[
        ('Select option (1-2):', '2'), ('the already downloaded ISO file:', f'{machine.home}/hf-test.iso'),
        ('(e.g. /dev/sdX):', '/dev/sda'), ('(y/N):', 'y')])
    # The stick is a file here: the image must be on it, then boot from it.
    image = machine.work / 'hf-test.iso'
    machine.fetch_file('hf-test.iso', image)
    written = (machine.work / 'usb.img').read_bytes()[:image.stat().st_size]
    if written != image.read_bytes():
        raise RuntimeError('the stick does not hold the image')
    for kind in ('bios', 'uefi'):
        # It boots in seconds; on a runner slowed by its neighbors the
        # firmware has taken minutes, so a stick that does not come up gets
        # a second start before it counts as not booting.
        for attempt in (1, 2):
            log_file = machine.work / f'usb-{kind}.log'
            log_file.unlink(missing_ok=True)
            command = ['qemu-system-x86_64', '-enable-kvm', '-m', '512', '-display', 'none', '-nic', 'none',
                       '-drive', f'file={machine.work / "usb.img"},format=raw,if=none,id=stick,readonly=on,file.locking=off',
                       '-device', 'qemu-xhci', '-device', 'usb-storage,drive=stick,bootindex=0',
                       '-serial', f'file:{log_file}']
            command[1:1] = firmware_options(kind, machine.work / f'usb-vars-{kind}.fd')
            with subprocess.Popen(command) as qemu:
                deadline = time.time() + 180
                while time.time() < deadline and b'hf-usb-booted' not in (
                        log_file.read_bytes() if log_file.exists() else b''):
                    time.sleep(1)
                qemu.kill()
            said = log_file.read_bytes() if log_file.exists() else b''
            if b'hf-usb-booted' in said:
                break
            if attempt == 1:
                log(f'the stick did not boot on {kind.upper()} firmware in 180 s; starting it again')
        else:
            tail = ANSI.sub('', said[-600:].decode(errors='replace')).strip()
            raise RuntimeError(f'the stick did not boot on {kind.upper()} firmware, twice; the serial line ended:\n{tail}')


# Run by the installed system once it is up, to report its state where the
# test can read it: the serial line.
SERVER_CHECK = """#!/bin/sh
exec >/dev/ttyS0 2>&1
echo "hf-check state=$(systemctl is-system-running --wait)"
echo "hf-check failed=$(systemctl --failed --plain --no-legend | cut -d' ' -f1 | tr '\\n' ' ')"
echo "hf-check groups=$(id -nG forge)"
echo "hf-check network=$(systemctl is-active NetworkManager)"
echo "hf-check root=$(findmnt -no FSTYPE,OPTIONS /)"
echo "hf-check swap=$(swapon --show=NAME --noheadings)"
echo "hf-check host=$(cat /etc/hostname)"
echo "hf-check kernel=$(uname -r)"
echo "hf-check done"
"""

SERVER_UNIT = """[Unit]
Description=Report the installed system's state on the serial line
[Service]
Type=simple
ExecStart=/usr/local/bin/hf-check
[Install]
WantedBy=multi-user.target
"""

MENU = re.compile(r'arrow keys and Enter:(?:\n[> ] [^\n]*)+')


def server_setup(machine, shots):
    """Install Arch from the live ISO on the empty disk, then boot it."""
    # The live ISO fills pacman's keyring at boot, in pacman-init; SSH can
    # answer before that ends, and a package checked before then has no
    # trusted signature. Starting the service again waits for it.
    machine.ssh('systemctl start pacman-init.service')
    output = machine.run('arch-server-setup', answers=[
        ('Please enter username:', 'forge'), ('Please enter password:', 'helix-toolbox-test'),
        ('Please re-enter password:', 'helix-toolbox-test'), ('Please name your machine:', 'forge-server'),
        # The disk, btrfs and the keyboard layout: the first of each.
        (MENU, ''), ('(Y/n)', 'y'), ('(e.g. de_DE.UTF-8):', '')])
    if '/dev/vda selected' not in output:
        raise RuntimeError('the installer did not choose the empty disk')
    installed = {
        'the user has a password': "grep -q '^forge:\\$' /mnt/etc/shadow",
        'the administrators need their password for sudo': "grep -q '^%wheel ALL=(ALL:ALL) ALL' /mnt/etc/sudoers",
        'GRUB has a menu': 'test -s /mnt/boot/grub/grub.cfg',
        'the root file system is in fstab': "grep -q 'subvol=/@[[:space:],]' /mnt/etc/fstab",
    }
    for what, check in installed.items():
        if not machine.ok(check):
            raise RuntimeError(f'after the installation, not true: {what}')
    machine.ssh('cat > /mnt/usr/local/bin/hf-check && chmod 755 /mnt/usr/local/bin/hf-check', stdin=SERVER_CHECK)
    machine.ssh('cat > /mnt/etc/systemd/system/hf-check.service && '
                'ln -s /etc/systemd/system/hf-check.service /mnt/etc/systemd/system/multi-user.target.wants/',
                stdin=SERVER_UNIT)
    machine.ssh('sync; umount -R /mnt; swapoff -a; systemctl poweroff', check=False)
    machine.gone()
    # The installed system alone: the disk, with the firmware's variables
    # grub-install wrote.
    serial = machine.work / 'installed.log'
    serial.unlink(missing_ok=True)
    machine.launch(['-drive', f'file={machine.disk},if=virtio,discard=unmap'], serial='installed.log')
    reported = lambda: b'hf-check done' in (serial.read_bytes() if serial.exists() else b'')
    frames = machine.watch(reported, patience=300)
    keep_frames(frames, shots, 'arch-server-setup', 'uefi')
    if not reported():
        raise RuntimeError('the installed system did not start')
    report = dict(line.split('=', 1) for line in ANSI.sub('', serial.read_text(errors='replace')).splitlines()
                  if line.startswith('hf-check ') and '=' in line)
    report = {key.removeprefix('hf-check '): value.strip() for key, value in report.items()}
    expected = {
        'state': lambda value: value == 'running',
        'groups': lambda value: 'wheel' in value.split(),
        'network': lambda value: value == 'active',
        'root': lambda value: value.startswith('btrfs') and 'subvol=/@' in value,
        'swap': lambda value: value == '/opt/swap/swapfile',
        'host': lambda value: value == 'forge-server',
        'kernel': lambda value: value.endswith('-lts'),
    }
    wrong = {key: report.get(key) for key, good in expected.items() if not good(report.get(key, ''))}
    if wrong:
        raise RuntimeError(f'the installed system is not as set up: {wrong}; failed units: {report.get("failed")}')


def selinux(machine, shots):
    # Fedora's cloud image enforces SELinux; the entry must keep it so.
    if machine.ssh('getenforce').stdout.strip() != 'Enforcing':
        raise RuntimeError('SELinux is not enforcing before the run')
    machine.run('selinux')
    packages = 'selinux-policy-targeted policycoreutils policycoreutils-python-utils libselinux-utils setroubleshoot-server'
    if not machine.ok(f'rpm -q {packages}'):
        raise RuntimeError('the SELinux tools are not installed')
    if machine.ssh('getenforce').stdout.strip() != 'Enforcing':
        raise RuntimeError('SELinux stopped enforcing')
    machine.reboot(shots, 'selinux')
    if machine.ssh('getenforce').stdout.strip() != 'Enforcing':
        raise RuntimeError('SELinux is not enforcing after the reboot')
    if not machine.ok('sudo semanage boolean -l >/dev/null'):
        raise RuntimeError('semanage does not work')


def linux_neptune(machine, shots):
    if not machine.ok('grep -qx Valve /sys/devices/virtual/dmi/id/board_vendor'):
        raise RuntimeError('the machine does not look like a Steam Deck')
    machine.run('linux-neptune-for-steamdeck')
    installed = {
        'a linux-neptune kernel is installed': "pacman -Qq | grep -q '^linux-neptune-[0-9]*$'",
        'its kernel is in /boot': 'ls /boot/vmlinuz-linux-neptune*',
        'GRUB offers it': 'sudo grep -q "vmlinuz-linux-neptune" /boot/grub/grub.cfg',
        'the speaker firmware is in place': 'ls /usr/lib/firmware/cirrus/cs35l41-dsp1-spk-prot.*',
    }
    for what, check in installed.items():
        if not machine.ok(check):
            raise RuntimeError(f'after the run, not true: {what}')
    # Boot it: the next boot only, by its menu entry's id.
    menu = machine.ssh('sudo cat /boot/grub/grub.cfg').stdout
    submenu = re.search(r"^submenu .* \$menuentry_id_option '([^']+)'", menu, re.M)
    entry = re.search(r"menuentry '[^']*linux-neptune[^']*' .* \$menuentry_id_option '([^']+)'", menu)
    if not entry:
        raise RuntimeError('GRUB has no menu entry for linux-neptune')
    target = f'{submenu.group(1)}>{entry.group(1)}' if submenu and entry.start() > submenu.start() else entry.group(1)
    machine.ssh(f'sudo grub-reboot {shlex.quote(target)}')
    machine.reboot(shots, 'linux-neptune-for-steamdeck')
    kernel = machine.ssh('uname -r').stdout.strip()
    if 'neptune' not in kernel:
        raise RuntimeError(f'the machine came back on {kernel}, not linux-neptune')
    # And on whatever GRUB now boots first.
    machine.reboot(None, 'linux-neptune-default')


def flatpak_bundle(machine, shots):
    """A Flatpak installed here goes into a bundle with its runtimes, and
    installs from it with no network: through the Flathub remote the machine
    has, and, only on trust, with the bundle's copy of its key."""
    app = 'org.flycast.Flycast'
    machine.ssh('sudo pacman -S --noconfirm --needed flatpak >/dev/null')
    machine.ssh('curl -fsSLo flathub.flatpakrepo https://dl.flathub.org/repo/flathub.flatpakrepo')
    machine.ssh('sudo flatpak remote-add --if-not-exists flathub flathub.flatpakrepo')
    machine.ssh(f'sudo flatpak install -y --noninteractive flathub {app} >/dev/null')
    refused = machine.ssh('./helix-toolbox bundle create bundle flycast', check=False)
    if refused.returncode == 0 or '--collection-id=org.flathub.Stable flathub' not in refused.stderr:
        raise RuntimeError(f'a remote without a collection ID was not named: {refused.stderr[-500:]}')
    machine.ssh('sudo flatpak remote-modify --collection-id=org.flathub.Stable flathub')
    made = machine.ssh('./helix-toolbox bundle create bundle flycast').stdout
    if f'Offline: Flycast (the Flatpak {app})' not in made:
        raise RuntimeError(f'Flycast was not bundled as a Flatpak:\n{made}')
    machine.ssh('./helix-toolbox bundle verify bundle')
    # The other machine: Flathub added as its file adds it, without a
    # collection ID, and nothing installed; then no network.
    machine.ssh('sudo flatpak uninstall -y --noninteractive --all >/dev/null && sudo flatpak remote-delete flathub'
                ' && sudo flatpak remote-add flathub flathub.flatpakrepo')
    machine.relaunch(network='restricted')
    try:
        if machine.ok('curl -fsS --max-time 10 -o /dev/null https://dl.flathub.org/repo/'):
            raise RuntimeError('the machine still reaches Flathub')
        machine.ssh('./helix-toolbox bundle install bundle --yes')
        machine.ssh(f'flatpak info {app} >/dev/null && flatpak run --command=true {app}')
        if 'org.flathub.Stable' not in machine.ssh('flatpak remotes --columns=name,collection').stdout:
            raise RuntimeError('the remote was not given its collection ID')
        # No remote at all: the bundle's key, only when trusted.
        machine.ssh('sudo flatpak uninstall -y --noninteractive --all >/dev/null && sudo flatpak remote-delete flathub')
        refused = machine.ssh('./helix-toolbox bundle install bundle --yes', check=False)
        if refused.returncode == 0 or 'no flathub Flatpak remote' not in refused.stderr:
            raise RuntimeError(f'a missing remote was added on the bundle\'s word: {refused.stderr[-500:]}')
        machine.ssh('./helix-toolbox bundle install bundle --trust-bundle --yes')
        machine.ssh(f'flatpak info {app} >/dev/null && flatpak run --command=true {app}')
    finally:
        machine.relaunch()


def zypper_bundle(machine, shots):
    """zypper downloads a bundle as the user, without root, and it installs
    with no network."""
    machine.ssh('sudo zypper -n -q rm -u htop >/dev/null 2>&1 || true')
    made = machine.ssh('./helix-toolbox bundle create bundle htop && ./helix-toolbox bundle create complete htop --complete'
                       ' && ./helix-toolbox bundle verify complete').stdout
    if 'zypper downloads htop with the dependencies this machine lacks, as you, without root.' not in made:
        raise RuntimeError(f'zypper did not make the bundle:\n{made[-1500:]}')
    files = lambda folder: int(machine.ssh(f'ls {folder}/packages | wc -l').stdout)
    if not files('complete') > files('bundle') >= 1:
        raise RuntimeError('a complete bundle does not hold more than one for this machine')
    machine.relaunch(network='restricted')
    try:
        if machine.ok('curl -fsS --max-time 10 -o /dev/null https://download.opensuse.org/'):
            raise RuntimeError('the machine still reaches openSUSE')
        machine.ssh('./helix-toolbox bundle install complete --yes < /dev/null')
        machine.ssh('rpm -q htop && htop --version')
    finally:
        machine.relaunch()


def boot_with_limine(machine):
    """Boot with Limine instead of GRUB, as CachyOS installs it: Limine as
    the EFI partition's boot loader, and the kernel and its boot image beside
    limine.conf. A word added to the kernel command line shows Limine booted
    it."""
    if machine.ok('grep -qw hf_limine /proc/cmdline'):
        return
    machine.ssh('sudo pacman -S --noconfirm --needed limine >/dev/null')
    arguments = [arg for arg in machine.ssh('cat /proc/cmdline').stdout.split() if not arg.startswith('BOOT_IMAGE=')]
    conf = ('timeout: 5\n\n'
            '/Arch Linux\n'
            '    protocol: linux\n'
            '    path: boot():/vmlinuz-linux\n'
            f'    cmdline: {" ".join(arguments)} hf_limine\n'
            '    module_path: boot():/initramfs-linux.img\n')
    machine.ssh('sudo cp /boot/vmlinuz-linux /boot/initramfs-linux.img /efi/ && sudo tee /efi/limine.conf >/dev/null '
                '&& sudo cp /usr/share/limine/BOOTX64.EFI /efi/EFI/BOOT/BOOTX64.EFI', stdin=conf)
    machine.reboot(None, 'limine')
    if not machine.ok('grep -qw hf_limine /proc/cmdline'):
        raise RuntimeError('the machine did not boot through Limine')


def limine_look(machine, shots, entry):
    boot_with_limine(machine)
    before = machine.ssh('sudo cat /efi/limine.conf').stdout
    machine.run(entry)
    after = machine.ssh('sudo cat /efi/limine.conf').stdout
    if 'wallpaper: boot():/dotfiles-splash.png' not in after:
        raise RuntimeError(f'the wallpaper is not in limine.conf:\n{after}')
    if not machine.ok('sudo test -s /efi/dotfiles-splash.png'):
        raise RuntimeError('the wallpaper was not copied beside limine.conf')
    entries = before[before.index('/Arch Linux'):]
    if entries not in after:
        raise RuntimeError(f'the boot entry changed:\n{after}')
    # Above the look's own block, only what was there before, outside any
    # previous look's block.
    marker = '# Boot menu appearance from dotfiles'
    if after.count(marker) != 1:
        raise RuntimeError(f'limine.conf does not have one look:\n{after}')
    kept = {line for line in before.split(marker)[0].splitlines() if line.strip()}
    left = [line for line in after.split(marker)[0].splitlines() if line.strip() and line not in kept]
    if left:
        raise RuntimeError(f'settings from elsewhere were left above the look: {left}')
    shown = machine.reboot(shots, entry)
    if not machine.ok('grep -qw hf_limine /proc/cmdline'):
        raise RuntimeError('the machine did not boot through Limine after the change')
    if shown < 60:
        raise RuntimeError(f'no wallpaper was seen on the boot menu ({shown} colors at most)')


def limine_cachyos(machine, shots):
    limine_look(machine, shots, 'limine-cachyos')


def limine_garuda(machine, shots):
    limine_look(machine, shots, 'limine-garuda')


CASES = {
    'arch': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
             ('plymouth-theme', plymouth_theme), ('plymouth-dotfiles', plymouth_dotfiles),
             ('plymouth-off', plymouth_off), ('apparmor', apparmor), ('sddm', sddm), ('enable-ssd-trim', ssd_trim),
             ('bootable-usb-creator', bootable_usb)],
    # Flatpak's runtimes take as long to download as the other cases take to
    # run, so the Flatpak bundle has a machine of its own.
    'arch-bundle': [('bundle-flatpak', flatpak_bundle)],
    # Every other GRUB theme, Garuda's from its own repository; the Chris
    # Titus Tech themes on a second machine, so the two halves run at once.
    'arch-themes': [(entry, grub_theme_case(entry)) for entry in (
        'grub-cachyos', 'grub-catppuccin', 'grub-tela', 'grub-vimix', 'grub')],
    'arch-ctt-themes': [(entry, grub_theme_case(entry)) for entry in (
        'grub-ctt-bsol', 'grub-ctt-cyberpunk', 'grub-ctt-cyberre', 'grub-ctt-dedsec', 'grub-ctt-fallout',
        'grub-ctt-minegrub', 'grub-ctt-shodan')],
    'debian': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
               ('plymouth-off', plymouth_off), ('apparmor', apparmor), ('sddm', sddm), ('enable-ssd-trim', ssd_trim)],
    'fedora': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
               ('plymouth-off', plymouth_off), ('selinux', selinux), ('sddm', sddm), ('enable-ssd-trim', ssd_trim)],
    'arch-uefi': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('limine-cachyos', limine_cachyos),
                  ('limine-garuda', limine_garuda)],
    'opensuse': [('bundle-zypper', zypper_bundle)],
    'arch-iso': [('arch-server-setup', server_setup)],
    'steamdeck': [('linux-neptune-for-steamdeck', linux_neptune)],
    # What also applies on ARM64 boards and servers that boot through UEFI.
    'debian-arm64': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
                     ('plymouth-off', plymouth_off), ('apparmor', apparmor), ('sddm', sddm),
                     ('enable-ssd-trim', ssd_trim)],
    'fedora-arm64': [('grub-dracula', grub_theme), ('grub-default', grub_default), ('plymouth-on', plymouth_on),
                     ('plymouth-off', plymouth_off), ('selinux', selinux), ('sddm', sddm),
                     ('enable-ssd-trim', ssd_trim)],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n\n')[0])
    parser.add_argument('--distro', choices=sorted(MACHINES), required=True, help='the machine')
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
                # What the machine printed, for a person to read.
                if options.shots and getattr(machine, 'serial', None) and machine.serial.exists():
                    options.shots.mkdir(parents=True, exist_ok=True)
                    (options.shots / f'{entry}-serial.log').write_bytes(machine.serial.read_bytes())
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
