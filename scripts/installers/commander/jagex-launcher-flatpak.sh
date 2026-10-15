#!/bin/sh -e
# The Jagex Launcher as a Flatpak, from nmlynch94's community repository
# (https://github.com/nmlynch94/com.jagexlauncher.JagexLauncher), installed for
# this user the way its install-jagex-launcher-repo.sh does, without piping that
# script into bash. Neither the Flatpak nor this entry is affiliated with Jagex.

# shellcheck source=scripts/shared/common-script.sh
. "$HELIXFORGE_LIB/common-script.sh"

RUNTIME=25.08
REPOSITORY=https://nmlynch94.github.io/com.jagexlauncher.JagexLauncher/JagexLauncher.flatpakrepo
APP=com.jagexlauncher.JagexLauncher

if [ "$(id -u)" = 0 ]; then
    printf '%s\n' 'Run this as your normal user, not root; the launcher is installed for your account.' >&2
    exit 1
fi
if [ "$(uname -m)" != x86_64 ]; then
    printf '%s\n' 'The Jagex Launcher Flatpak is for x86_64 only.' >&2
    exit 1
fi
if ! command_exists flatpak; then
    printf '%s\n' 'Flatpak is not installed. Install it first (Applications, Flatpak / Flathub), then run this again.' >&2
    exit 1
fi

flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
flatpak remote-add --user --if-not-exists JLauncher "$REPOSITORY"
# The launcher is 32-bit Windows software under WINE: it needs the 32-bit
# runtime extensions, which Flatpak does not install by itself.
flatpak install --user -y flathub \
    "org.freedesktop.Platform//$RUNTIME" \
    "org.freedesktop.Platform.Compat.i386/x86_64/$RUNTIME" \
    "org.freedesktop.Platform.GL32.default/x86_64/$RUNTIME"

# The NVIDIA driver's own libraries, matching the running driver.
if [ -r /proc/driver/nvidia/version ]; then
    driver=$(grep '^NVRM version' /proc/driver/nvidia/version | grep -oE '[0-9]+\.[0-9]+(\.[0-9]+)?' | head -n 1 | tr . -)
    if [ -n "$driver" ]; then
        flatpak install --user -y flathub \
            "org.freedesktop.Platform.GL.nvidia-$driver/x86_64" \
            "org.freedesktop.Platform.GL32.nvidia-$driver/x86_64" ||
            printf 'Flathub has no libraries for NVIDIA driver %s yet; the launcher may not start until it does (flatpak update installs them then).\n' "$driver"
    fi
fi

flatpak install --user -y JLauncher "$APP"
flatpak info --user "$APP" > /dev/null
printf '%s\n' 'Installed. Start the Jagex Launcher from your application menu or with: flatpak run com.jagexlauncher.JagexLauncher' \
    'The first start sets it up and can take a minute or two; click Update, then start it again. flatpak update keeps it current.'
