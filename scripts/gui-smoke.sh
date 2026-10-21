#!/bin/sh -e

# Drive a debug build of the window through the setup wizard, the command
# palette, the theme, the keyboard, every screen, a run with its change
# preview, the removal of a package entry, a saved profile exported,
# deleted and imported again, the offline bundles screen, a bug report and
# the update notifications turned on and off, against the fixture catalog, and check what the drive script reports.
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
# The fixture catalog plus the wizard's workstations, a removable package
# entry and a Profiles area for the profile the plan is saved as; the
# fixtures themselves have none of them, so the terminal interface's tests
# see no wizard and the same entries.
cp -r tests/fixtures "$work/catalog"
cp apps/helix-toolbox-gui/tests/drive/setup.toml "$work/catalog/catalog/setup.toml"
cp apps/helix-toolbox-gui/tests/drive/removable.toml "$work/catalog/catalog/tools/removable.toml"
printf 'tabs = ["tools", "profiles"]\n' > "$work/catalog/catalog/tabs.toml"
mkdir "$work/catalog/catalog/profiles"
printf 'name = "Profiles"\n' > "$work/catalog/catalog/profiles/tab.toml"
# A stand-in command line that only prints a report, for the bug report,
# a systemctl that only writes down what the update notifications ask of
# systemd, an xdg-open that writes down the link instead of opening a
# browser, and a real machine for the hardware report, as the workflow's
# runners are virtual.
mkdir "$work/bin"
cp apps/helix-toolbox-gui/tests/drive/helix-toolbox "$work/bin/helix-toolbox"
printf '#!/bin/sh\necho "$*" >> "%s/systemctl.log"\n' "$work" > "$work/bin/systemctl"
printf '#!/bin/sh\necho "$*" >> "%s/xdg-open.log"\n' "$work" > "$work/bin/xdg-open"
printf '#!/bin/sh\necho none\nexit 1\n' > "$work/bin/systemd-detect-virt"
chmod +x "$work/bin/systemctl" "$work/bin/xdg-open" "$work/bin/systemd-detect-virt"
export PATH="$work/bin:$PATH"
mkdir -p "$work/home" "$work/config" "$work/state" "$work/data" "$work/cache"
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" \
    XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
# The marker fixture writes here, which shows the run really ran.
export HELIX_TOOLBOX_TEST_OUT="$work/marker.txt"
# WebKitGTK on a virtual display: no GPU, no compositor.
export GDK_BACKEND=x11 WEBKIT_DISABLE_DMABUF_RENDERER=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
# A file chooser stays on the virtual display: through the desktop portal
# it would open on the real one.
unset GTK_USE_PORTAL

# axe-core, from npm ci, goes in front of the drive script, which checks
# each screen with it; CI must have it.
drive=apps/helix-toolbox-gui/tests/drive/smoke.js
axe=apps/helix-toolbox-gui/node_modules/axe-core/axe.min.js
if [ -f "$axe" ]; then
    cat "$axe" "$drive" > "$work/drive.js"
    drive=$work/drive.js
elif [ -n "${CI:-}" ]; then
    echo "$axe is missing; run npm ci in apps/helix-toolbox-gui" >&2
    exit 1
fi

status=0
timeout 240 "$binary" --catalog-dir "$work/catalog" \
    --drive "$drive" 2> "$work/stderr" || status=$?
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
# plan was saved as (imported again after it was deleted) and its export,
# the mark that the wizard was offered and the theme chosen last.
if ! grep -q '^marker from ' "$work/marker.txt" 2>/dev/null; then
    echo "the run did not write the marker file" >&2
    exit 1
fi
if ! ls "$work/config/helix-toolbox/profiles/"*.toml >/dev/null 2>&1; then
    echo "no profile was saved in $work/config/helix-toolbox/profiles" >&2
    exit 1
fi
if ! grep -q '^name = "Smoke profile"' "$work/config/helix-toolbox/exported.toml" 2>/dev/null; then
    echo "the profile was not exported" >&2
    exit 1
fi
if ! grep -qx 'set_up = true' "$work/config/helix-toolbox/settings.toml" 2>/dev/null; then
    echo "the wizard was not remembered as offered" >&2
    exit 1
fi
if ! grep -qx 'window_theme = "system"' "$work/config/helix-toolbox/settings.toml"; then
    echo "the theme was not saved" >&2
    exit 1
fi
# The notifications' timer was started and stopped, and its files are gone.
if ! grep -qx -- '--user start helix-toolbox-updates.timer' "$work/systemctl.log" 2>/dev/null \
    || ! grep -qx -- '--user stop helix-toolbox-updates.timer' "$work/systemctl.log"; then
    echo "the update notifications' timer was not started and stopped" >&2
    exit 1
fi
if [ -e "$work/config/systemd/user/helix-toolbox-updates.timer" ]; then
    echo "turning the update notifications off left the timer behind" >&2
    exit 1
fi
# The hardware report's issue went to the browser, filled in.
if ! grep -q '^https://github.com/Commanderx-code/Helix-toolbox/issues/new?template=hardware_report.yml&.*records=' "$work/xdg-open.log" 2>/dev/null; then
    echo "the hardware report's issue was not opened" >&2
    exit 1
fi
echo "the window's smoke test passed"
