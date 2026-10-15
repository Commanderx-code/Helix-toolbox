"""Generate backed-up launchers without modifying Resolve's bundled libraries."""
import os
from pathlib import Path
import shlex
import sys
import tempfile

from configure import deploy, check_destination

# System GLib/GIO preloading is the documented ArchWiki/Davincibox workaround.
# Qt must use X11/XWayland; avoid inherited desktop Qt plugin paths.
NATIVE = '''#!/bin/sh -e
unset QT_PLUGIN_PATH QT_QPA_PLATFORM_PLUGIN_PATH
export QT_QPA_PLATFORM=xcb
libs=''
for name in libglib-2.0.so.0 libgobject-2.0.so.0 libgio-2.0.so.0 libgmodule-2.0.so.0 libgthread-2.0.so.0 libgdk_pixbuf-2.0.so.0; do
    found=false
    for dir in /usr/lib64 /usr/lib; do
        if [ -f "$dir/$name" ]; then
            libs="${libs:+$libs:}$dir/$name"
            found=true
            break
        fi
    done
    if [ "$found" != true ]; then
        printf 'Missing system library: %s\\n' "$name" >&2
        exit 1
    fi
done
export LD_PRELOAD="$libs${LD_PRELOAD:+:$LD_PRELOAD}"
if [ "${1:-}" = --check ]; then
    dependencies=$(LC_ALL=C ldd /opt/resolve/bin/resolve)
    printf '%s\\n' "$dependencies"
    if printf '%s\\n' "$dependencies" | grep -q 'not found'; then
        printf '%s\\n' 'Missing Resolve libraries. Install their distro packages before launching.' >&2
        exit 1
    fi
    exit 0
fi
exec /opt/resolve/bin/resolve "$@"
'''


def desktop_quote(value):
    # Desktop Exec escaping differs from shell quoting; % is a field code.
    value = value.replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%')
    return '"' + value + '"'


def destinations(home=None, data=None):
    home = Path(home or Path.home())
    data = Path(data or os.environ.get('XDG_DATA_HOME', home / '.local/share'))
    return (home / '.local/bin/commander-resolve',
            data / 'applications/commander-resolve.desktop',
            data / 'helixforge/resolve-runtime.sh')


def install(mode, home=None, data=None):
    if mode not in ('native', 'container'):
        raise ValueError('Unknown installation mode')
    launcher, desktop, runtime = destinations(home, data)
    for destination in (launcher, desktop, runtime):
        check_destination(destination)
    with tempfile.TemporaryDirectory() as tmp:
        source = Path(tmp) / 'file'
        source.write_text(NATIVE)
        source.chmod(0o755)
        deploy(source, runtime)
        command = ('exec sh ' + shlex.quote(str(runtime)) + ' "$@"') if mode == 'native' else (
            'exec distrobox enter --name commander-resolve -- sh ' + shlex.quote(str(runtime)) + ' "$@"')
        source.write_text('#!/bin/sh -e\n' + ('export DBX_CONTAINER_MANAGER=podman\n' if mode == 'container' else '') + command + '\n')
        deploy(source, launcher)
        source.write_text('[Desktop Entry]\nType=Application\nName=DaVinci Resolve (Commander)\n'
                          'Comment=DaVinci Resolve with Linux compatibility libraries\n'
                          'Exec=' + desktop_quote(str(launcher)) + '\nIcon=applications-multimedia\n'
                          'Terminal=false\nCategories=AudioVideo;Video;\n')
        source.chmod(0o644)
        deploy(source, desktop)


if __name__ == '__main__':
    if sys.argv[1] == '--runtime':
        print(NATIVE, end='')
    elif sys.argv[1] == '--check':
        for path in destinations():
            check_destination(path)
    else:
        install(sys.argv[1])
