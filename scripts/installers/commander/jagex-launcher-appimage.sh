#!/bin/sh -e
# Jagex's official Linux launcher, a beta AppImage (Old School RuneScape for
# now), kept in ~/.local/share/helixforge/appimages with a jagex-launcher
# command and a menu entry. Running this again replaces it with the latest.

# shellcheck source=scripts/shared/common-script.sh
. "$HELIXFORGE_LIB/common-script.sh"

URL=https://rs-launcher-updates.runescape.com/production/linux/x64/latest/jagex-launcher-beta-linux-x86_64.AppImage
data=${XDG_DATA_HOME:-$HOME/.local/share}
directory="$data/helixforge/appimages"
appimage="$directory/jagex-launcher.AppImage"
command="$HOME/.local/bin/jagex-launcher"
launcher="$data/applications/jagex-launcher.desktop"
icon="$directory/jagex-launcher.png"

if [ "$(id -u)" = 0 ]; then
    printf '%s\n' 'Run this as your normal user, not root; the launcher is installed for your account.' >&2
    exit 1
fi
if [ "$(uname -m)" != x86_64 ]; then
    printf '%s\n' 'The Jagex Launcher AppImage is for x86_64 only.' >&2
    exit 1
fi
checkCommandRequirements 'curl'

# The AppImage carries its own FUSE library but needs the system's fusermount.
if ! command_exists fusermount3 && ! command_exists fusermount; then
    checkEscalationTool
    printf '%s\n' 'Installing fuse3, which AppImages need to start.'
    if command_exists pacman; then
        "$ESCALATION_TOOL" pacman -S --needed fuse3
    elif command_exists apt-get; then
        "$ESCALATION_TOOL" apt-get install -y fuse3
    elif command_exists dnf; then
        "$ESCALATION_TOOL" dnf install -y fuse3
    elif command_exists zypper; then
        "$ESCALATION_TOOL" zypper install -y fuse3
    else
        printf '%s\n' 'Install fuse3 with your package manager, then run this again.' >&2
        exit 1
    fi
fi

mkdir -p "$directory" "$HOME/.local/bin" "$data/applications"
staging=$(mktemp "$directory/.download.XXXXXX")
extract=$(mktemp -d)
trap 'rm -rf "$staging" "$extract"' 0
printf 'Downloading %s\n' "$URL"
curl -fL --proto '=https' --tlsv1.2 -o "$staging" "$URL"

# Keep it only if it is an AppImage: an ELF program marked AppImage type 2.
magic() { dd if="$staging" bs=1 skip="$1" count="$2" 2> /dev/null | od -An -tx1 | tr -d ' \n'; }
if [ "$(magic 0 4)" != 7f454c46 ] || [ "$(magic 8 3)" != 414902 ]; then
    printf '%s\n' 'The download is not an AppImage; nothing was changed.' >&2
    exit 1
fi
chmod 755 "$staging"
mv -f "$staging" "$appimage"
ln -sfn "$appimage" "$command"

# Its icon, extracted without running the launcher.
if (cd "$extract" && "$appimage" --appimage-extract .DirIcon > /dev/null 2>&1); then
    found="$extract/squashfs-root/.DirIcon"
    if [ -L "$found" ]; then
        target=$(readlink "$found")
        (cd "$extract" && "$appimage" --appimage-extract "$target" > /dev/null 2>&1) || true
        found="$extract/squashfs-root/$target"
    fi
    if [ -f "$found" ]; then
        cp -f "$found" "$icon"
    fi
fi

cat > "$launcher" << DESKTOP
[Desktop Entry]
Type=Application
Name=Jagex Launcher
Comment=Play Old School RuneScape (Jagex's Linux beta)
Exec=$appimage
Icon=$icon
Categories=Game;
Terminal=false
DESKTOP
if command_exists update-desktop-database; then
    update-desktop-database "$data/applications" > /dev/null 2>&1 || true
fi

printf 'Installed: %s\n' "$appimage"
printf '%s\n' 'Start it from your application menu or with jagex-launcher (~/.local/bin must be on your PATH).' \
    "To remove it, delete $appimage, $command, $launcher and $icon."
