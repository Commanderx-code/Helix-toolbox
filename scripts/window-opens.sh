#!/bin/sh -e

# Start the window and wait for it to show itself, then close it: the test
# of the window's packages and AppImage, whose release builds take no drive
# script (scripts/gui-smoke.sh drives a debug build).
# usage: xvfb-run -a scripts/window-opens.sh COMMAND [ARG...]
# Needs xdotool. The window counts as open when its "Helix Toolbox" window
# is mapped and the program is still running a few seconds later.

if [ $# -eq 0 ]; then
    echo "usage: scripts/window-opens.sh COMMAND [ARG...]" >&2
    exit 2
fi
log=$(mktemp)
trap 'rm -f "$log"' 0
# WebKitGTK on a virtual display: no GPU, no compositor.
GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1 \
    "$@" > "$log" 2>&1 &
pid=$!

fail() {
    echo "$1" >&2
    sed 's/^/  /' "$log" >&2
    kill "$pid" 2>/dev/null || true
    exit 1
}

# Up to a minute: WebKitGTK starts slowly without a GPU.
waited=0
until xdotool search --onlyvisible --name '^Helix Toolbox$' > /dev/null 2>&1; do
    kill -0 "$pid" 2>/dev/null || fail "the window ended before it showed itself:"
    [ "$waited" -lt 60 ] || fail "no Helix Toolbox window within 60 s:"
    sleep 1
    waited=$((waited + 1))
done
sleep 5
kill -0 "$pid" 2>/dev/null || fail "the window ended within 5 s of showing itself:"
kill "$pid"
wait "$pid" 2>/dev/null || true
echo "the window opened after ${waited} s: $*"
