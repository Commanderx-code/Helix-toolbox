#!/usr/bin/env python3
"""Create an optional Zellij preset without replacing existing configuration."""
import os
from pathlib import Path

from configure import check_destination


PRESET = '''// Start manually with zellij; inherit your current login shell.
// Ctrl+G unlocks controls. No shell startup integration is installed.
theme "tokyo-night-storm"
default_mode "locked"
'''


def install(config):
    target = config / 'zellij/config.kdl'
    if target.exists() or target.is_symlink():
        print(f'Preserving existing Zellij config: {target}')
        return
    check_destination(target)
    target.parent.mkdir(parents=True, exist_ok=True)
    # Exclusive creation also protects against a concurrent installer.
    with target.open('x') as stream:
        stream.write(PRESET)
    print(f'Installed: {target}')


if __name__ == '__main__':
    install(Path(os.environ.get('XDG_CONFIG_HOME') or Path.home() / '.config'))
