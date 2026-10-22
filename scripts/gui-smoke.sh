#!/bin/sh -e

# Drive a debug build of the window through the setup wizard, the command
# palette, the theme, the keyboard, every screen, a run with its change
# preview, the removal of a package entry, a saved profile exported,
# deleted and imported again, the offline bundles screen, a migration
# exported and one read, a scratch repository's diff, stage and task
# reviewed, a bug report and the update notifications turned on and off,
# against the fixture catalog, and check what the drive script reports.
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
# It answers the Services screen's two lists with a few units of its own,
# and the Performance screen with a boot, which systemd-analyze's stand-in
# says waited for the network.
cat > "$work/bin/systemctl" <<EOF
#!/bin/sh
echo "\$*" >> "$work/systemctl.log"
case "\$*" in
    *list-unit-files*) printf '%s' '[{"unit_file":"cups.service","state":"disabled","preset":"disabled"},{"unit_file":"fstrim.timer","state":"enabled","preset":"enabled"}]' ;;
    *list-units*) printf '%s' '[{"unit":"cups.service","load":"loaded","active":"inactive","sub":"dead","description":"CUPS Scheduler"},{"unit":"fstrim.timer","load":"loaded","active":"active","sub":"waiting","description":"Discard unused blocks once a week"}]' ;;
    show\ --property=*) printf 'InitRDTimestampMonotonic=1065390\nUserspaceTimestampMonotonic=3727009\nFinishTimestampMonotonic=12121346\n' ;;
esac
EOF
cat > "$work/bin/systemd-analyze" <<'EOF'
#!/bin/sh
case "$1" in
    blame) printf '      4.344s NetworkManager-wait-online.service\n      4.183s plocate-updatedb.service\n' ;;
    critical-chain) printf 'graphical.target @8.394s\n`-NetworkManager-wait-online.service @1.003s +4.344s\n' ;;
esac
EOF
printf '#!/bin/sh\necho "$*" >> "%s/xdg-open.log"\n' "$work" > "$work/bin/xdg-open"
printf '#!/bin/sh\necho none\nexit 1\n' > "$work/bin/systemd-detect-virt"
chmod +x "$work/bin/systemctl" "$work/bin/systemd-analyze" "$work/bin/xdg-open" "$work/bin/systemd-detect-virt"
export PATH="$work/bin:$PATH"
mkdir -p "$work/home" "$work/config" "$work/state" "$work/data" "$work/cache"
export HOME="$work/home"
# A repository for the Repositories screen, under one of the folders it
# looks in by default: a commit and a new file.
repo="$work/home/projects/sample"
mkdir -p "$repo"
git_here() {
    GIT_CONFIG_GLOBAL=/dev/null GIT_AUTHOR_NAME=Smoke GIT_AUTHOR_EMAIL=smoke@example.com \
        GIT_COMMITTER_NAME=Smoke GIT_COMMITTER_EMAIL=smoke@example.com git -C "$repo" "$@"
}
git_here init -q -b main
printf 'one\n' > "$repo/one.txt"
git_here add one.txt
git_here commit -q -m First
printf 'new\n' > "$repo/new.txt"
export XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" \
    XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache"
# A migration made on Fedora, for the Migration screen to read: a fixture
# entry it installs, one this catalog lacks, a Flatpak and a package.
mkdir -p "$work/config/helix-toolbox"
printf '%s\n' 'version = 1' 'from = "Fedora Linux 43"' 'family = "fedora"' \
    'architecture = "x86_64"' 'made = 1792000000' \
    'entries = ["marker", "not-in-this-catalog"]' \
    'flatpaks = ["com.example.Tool"]' 'packages = ["htop"]' \
    > "$work/config/helix-toolbox/from-fedora.toml"
# A stand-in restic for the Restic screen: one snapshot of the home folder
# with a file in it, found by a search; restores and tests are only
# reviewed. The repository and its password are named as restic's own
# variables and a KWallet entry would name them.
cat > "$work/bin/restic" <<EOF
#!/bin/sh
echo "\$*" >> "$work/restic.log"
snapshot=b6f77f6d00000000000000000000000000000000000000000000000000000000
case "\$*" in
    *' snapshots '*) printf '[{"id":"%s","short_id":"b6f77f6d","time":"2026-10-21T22:00:04+02:00","hostname":"smoke","paths":["%s"]}]\n' "\$snapshot" "$work/home" ;;
    *' ls '*) printf '{"struct_type":"snapshot","id":"%s"}\n{"name":"home","type":"dir","path":"%s","struct_type":"node"}\n{"name":"notes.txt","type":"file","path":"%s/notes.txt","size":6,"struct_type":"node"}\n' "\$snapshot" "$work/home" "$work/home" ;;
    *' find '*) printf '[{"matches":[{"type":"file","path":"%s/notes.txt","size":6}],"hits":1,"snapshot":"%s"}]\n' "$work/home" "\$snapshot" ;;
    'restore --help') echo '      --overwrite behavior   (always|if-changed|if-newer|never)'; echo '      --verify               verify restored files' ;;
esac
EOF
chmod +x "$work/bin/restic"
printf 'hello\n' > "$work/home/notes.txt"
unset RESTIC_PASSWORD RESTIC_PASSWORD_FILE RESTIC_PASSWORD_COMMAND
export RESTIC_REPOSITORY="$work/repository" RESTIC_WALLET=smoke \
    RESTIC_WALLET_ENTRY=restic RESTIC_WALLET_FOLDER=Passwords
# Configurations for the Configs screen, and a stand-in ghostty that checks
# Ghostty's, rejecting a "bogus" field.
mkdir -p "$work/config/ghostty" "$work/config/fastfetch"
printf 'font-size = 12\n' > "$work/config/ghostty/config"
printf '{\n    "modules": ["title", "os"]\n}\n' > "$work/config/fastfetch/config.jsonc"
cat > "$work/bin/ghostty" <<'EOF'
#!/bin/sh
[ "$1" = +validate-config ] || exit 2
if grep -q bogus "${2#--config-file=}"; then
    echo "unknown field: bogus"
    exit 1
fi
EOF
chmod +x "$work/bin/ghostty"
# A stand-in home-manager for the Home Manager screen, with a flake where
# Home Manager looks by itself: two generations, the earlier one not in
# this machine's store; builds and switches are only reviewed.
cat > "$work/bin/home-manager" <<EOF
#!/bin/sh
echo "\$*" >> "$work/home-manager.log"
case "\$1" in
    --version) echo 26.11-pre ;;
    generations) printf '%s\n' '2026-10-20 12:01 : id 201 -> /nix/store/aaa-home-manager-generation (current)' \
        '2026-10-16 13:23 : id 200 -> /nix/store/no-such-home-manager-generation' ;;
esac
EOF
chmod +x "$work/bin/home-manager"
mkdir -p "$work/home/.config/home-manager"
printf '{ }\n' > "$work/home/.config/home-manager/flake.nix"
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
if ! head -n 1 "$work/config/helix-toolbox/migration.toml" 2>/dev/null | grep -q '^# A Helix Toolbox migration'; then
    echo "this machine's migration was not exported" >&2
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
# The timed backup was started and stopped, and its files are gone; restic
# was only asked to read.
if ! grep -qx -- '--user restart helix-toolbox-restic.timer' "$work/systemctl.log" \
    || [ -e "$work/config/systemd/user/helix-toolbox-restic.timer" ]; then
    echo "the timed backup was not turned on and off" >&2
    exit 1
fi
if grep -E ' (backup|restore [0-9a-f]|dump|check)( |$)' "$work/restic.log"; then
    echo "restic was asked to change something" >&2
    exit 1
fi
# Ghostty's configuration was saved, and the one before kept beside it.
if ! grep -qx 'font-size = 14' "$work/config/ghostty/config"; then
    echo "the Ghostty configuration was not saved" >&2
    exit 1
fi
set -- "$work/config/ghostty/config.helix-toolbox-backup-"*
if ! grep -qx 'font-size = 12' "$1" 2>/dev/null; then
    echo "the Ghostty configuration before the save was not kept" >&2
    exit 1
fi
# Home Manager was only asked to read.
if grep -E "^(build|switch)" "$work/home-manager.log" 2>/dev/null; then
    echo "home-manager was asked to build or switch" >&2
    exit 1
fi
# The hardware report's issue went to the browser, filled in.
if ! grep -q '^https://github.com/Commanderx-code/Helix-toolbox/issues/new?template=hardware_report.yml&.*records=' "$work/xdg-open.log" 2>/dev/null; then
    echo "the hardware report's issue was not opened" >&2
    exit 1
fi
# The repository's stage was reviewed and canceled, and its task kept.
if [ -n "$(git_here diff --cached --name-only)" ]; then
    echo "something was staged in the scratch repository" >&2
    exit 1
fi
if ! grep -q 'echo smoke' "$work/config/helix-toolbox/workspaces.toml" 2>/dev/null; then
    echo "the task was not kept in workspaces.toml" >&2
    exit 1
fi
echo "the window's smoke test passed"
