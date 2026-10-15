#!/usr/bin/env python3
# Commander Linutil distro PNG launcher
"""Render the current distro PNG with a terminal-compatible protocol."""
import os
from pathlib import Path
import re
import shlex
import sys

MARKER = b'Commander Linutil distro PNG launcher'


def read_os_release(path=Path('/etc/os-release')):
    result = {}
    try:
        for line in path.read_text().splitlines():
            key, separator, value = line.partition('=')
            if separator and key in ('ID', 'LOGO'):
                tokens = shlex.split(value, comments=True)
                if tokens:
                    result[key] = tokens[0]
    except (OSError, ValueError):
        pass
    return result


def distro_logo(info, logos, system=Path('/usr/share')):
    distro = info.get('ID', '').lower()
    if not re.fullmatch(r'[a-z0-9_-]+', distro):
        distro = 'linux'
    bundled = logos / (distro + '.png')
    if bundled.is_file():
        return bundled
    if distro in ('opensuse-leap', 'opensuse-tumbleweed'):
        return logos / 'opensuse.png'
    # Prefer a distribution-provided PNG over guessing from ID_LIKE.
    icon = info.get('LOGO', '')
    if re.fullmatch(r'[a-zA-Z0-9_-]+', icon):
        candidates = [system / 'pixmaps' / (icon + '.png')]
        for size in (512, 256, 128, 64, 48):
            for category in ('apps', 'places'):
                candidates.append(system / f'icons/hicolor/{size}x{size}' / category / (icon + '.png'))
        for candidate in candidates:
            if candidate.is_file():
                return candidate
    return logos / 'linux.png'


def image_protocol(env):
    program = env.get('TERM_PROGRAM', '').lower()
    term = env.get('TERM', '').lower()
    if env.get('KONSOLE_VERSION') or program in ('iterm.app', 'wezterm'):
        return 'iterm'
    if env.get('KITTY_WINDOW_ID') or 'kitty' in term or 'ghostty' in term or program == 'ghostty':
        return 'kitty' if env.get('SSH_CONNECTION') else 'kitty-direct'
    if term.startswith(('foot', 'mlterm')) or 'sixel' in term:
        return 'sixel'
    return 'none'


def real_fastfetch(path):
    for directory in path.split(os.pathsep):
        candidate = Path(directory or '.') / 'fastfetch'
        if candidate.is_file() and os.access(candidate, os.X_OK):
            try:
                with candidate.open('rb') as handle:
                    if MARKER in handle.read(256):
                        continue
            except OSError:
                continue
            return str(candidate.resolve())
    raise ValueError('Fastfetch is not installed. Install the fastfetch package, then rerun this command.')


def logo_arguments(arguments, env, logos):
    # Explicit config/logo options belong to the caller.
    overrides = ('--config', '-c', '--logo', '-l', '--logo-type', '--kitty', '--kitty-direct', '--iterm', '--sixel', '--chafa', '--raw')
    if any(arg.split('=', 1)[0] in overrides for arg in arguments):
        return []
    protocol = image_protocol(env)
    logo = distro_logo(read_os_release(), logos)
    if protocol == 'none' or not logo.is_file():
        return ['--logo-type', 'none']
    return ['--logo', str(logo), '--logo-type', protocol, '--logo-width', '24']


def main():
    binary = real_fastfetch(os.environ.get('PATH', os.defpath))
    config = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config')))
    arguments = sys.argv[1:]
    defaults = logo_arguments(arguments, os.environ, config / 'fastfetch/commander-logos')
    os.execv(binary, [binary, *defaults, *arguments])


if __name__ == '__main__':
    try:
        main()
    except (ValueError, OSError) as error:
        sys.exit(str(error))
