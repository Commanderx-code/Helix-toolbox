#!/bin/sh -e

# Drive a debug build of the window through the setup wizard, the command
# palette, every screen, a run and a saved profile, against the fixture
# catalog, and check what the drive script reports.
# usage: scripts/gui-smoke.sh BINARY
# BINARY is a debug build with the frontend in it (cargo build --features
# tauri/custom-protocol in apps/helix-toolbox-gui/src-tauri, after npm run
# build). It needs a display: in CI, xvfb-run -a scripts/gui-smoke.sh …;
# by hand, a virtual session such as kwin_wayland --virtual --xwayland.
# Nothing on the machine is read from the user's home or changed: the
# window gets scratch folders for its home, settings, state and cache.

binary=${1:?usage: scripts/gui-smoke.sh BINARY}
binary=$(cd -- "$(dirname -- "$binary")" && pwd)/$(basename -- "$binary")
cd "$(dirname -- "$0")/.."

work=$(mktemp -d)
trap 'rm -rf "$work"' 0
# The fixture catalog plus the wizard's workstations; the fixtures
# themselves have none, so the terminal interface's tests see no wizard.
cp -r tests/fixtures "$work/catalog"
cp apps/helix-toolbox-gui/tests/drive/setup.toml "$work/catalog/catalog/setup.toml"
mkdir -p "$work/home" "$work/config" "$work/state" "$work/data" "$work/cache"
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" \
    XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
# The marker fixture writes here, which shows the run really ran.
export HELIX_TOOLBOX_TEST_OUT="$work/marker.txt"
# WebKitGTK on a virtual display: no GPU, no compositor.
export GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1

status=0
timeout 240 "$binary" --catalog-dir "$work/catalog" \
    --drive apps/helix-toolbox-gui/tests/drive/smoke.js 2> "$work/stderr" || status=$?
grep '^drive: ' "$work/stderr" || true
if [ "$status" -ne 0 ]; then
    echo "the window exited with status $status" >&2
    grep -v '^drive: ' "$work/stderr" | tail -40 >&2
    exit 1
fi
if ! grep -qx 'drive: DONE' "$work/stderr"; then
    echo "the drive script did not finish, or a step failed" >&2
    grep -v '^drive: ' "$work/stderr" | tail -40 >&2
    exit 1
fi
# What the steps left behind: the marker the run wrote, the profile the
# plan was saved as, and the mark that the wizard was offered.
if ! grep -q '^marker from ' "$work/marker.txt" 2>/dev/null; then
    echo "the run did not write the marker file" >&2
    exit 1
fi
if ! ls "$work/config/helix-toolbox/profiles/"*.toml >/dev/null 2>&1; then
    echo "no profile was saved in $work/config/helix-toolbox/profiles" >&2
    exit 1
fi
if ! grep -qx 'set_up = true' "$work/config/helix-toolbox/settings.toml" 2>/dev/null; then
    echo "the wizard was not remembered as offered" >&2
    exit 1
fi
echo "the window's smoke test passed"
