#!/bin/sh -e
# CachyOS's gaming meta-packages. On CachyOS: cachyos-gaming-meta or
# cachyos-gaming-applications from its repositories. On other Arch-based
# systems: the same packages from Arch, multilib and Chaotic-AUR or the AUR,
# with Arch's wine in place of wine-cachyos-opt (an AUR build of WINE).
#
# Usage: cachyos-gaming.sh meta|applications

# shellcheck source=scripts/shared/common-script.sh
. "$HELIXFORGE_LIB/common-script.sh"

# What cachyos-gaming-meta depends on, as Arch packages.
META_PACKAGES='alsa-plugins giflib glfw gst-plugins-base-libs lib32-alsa-plugins lib32-gtk3 lib32-libjpeg-turbo lib32-libva lib32-ocl-icd libjpeg-turbo libva libxslt mpg123 ocl-icd openal umu-launcher protontricks ttf-liberation winetricks vulkan-tools'
META_AUR='proton-cachyos-slr'
# What cachyos-gaming-applications adds.
APPLICATION_PACKAGES='gamescope goverlay lib32-mangohud lutris mangohud steam wqy-zenhei'
APPLICATION_AUR='heroic-games-launcher-bin faugus-launcher'

case "${1:-}" in
    meta) cachyos='cachyos-gaming-meta' ;;
    applications) cachyos='cachyos-gaming-applications' ;;
    *) printf '%s\n' 'Usage: cachyos-gaming.sh meta|applications' >&2; exit 2 ;;
esac

if [ "$(id -u)" = 0 ]; then
    printf '%s\n' 'Run this as your normal user, not root.' >&2
    exit 1
fi
if ! command_exists pacman; then
    printf '%s\n' "CachyOS's gaming packages are for Arch-based systems with pacman." >&2
    exit 1
fi
checkEscalationTool

if pacman -Si "$cachyos" > /dev/null 2>&1; then
    printf 'Installing %s from the CachyOS repositories.\n' "$cachyos"
    "$ESCALATION_TOOL" pacman -S --needed "$cachyos"
    exit 0
fi

# Everything needed is checked before anything is installed.
if ! pacman -Si lib32-gtk3 > /dev/null 2>&1; then
    printf '%s\n' 'These packages need the [multilib] repository. Enable it in /etc/pacman.conf, run sudo pacman -Sy, then run this again.' >&2
    exit 1
fi
helper=
for name in paru yay; do
    if command_exists "$name"; then
        helper=$name
        break
    fi
done
if [ -z "$helper" ]; then
    printf '%s\n' 'Proton CachyOS comes from Chaotic-AUR or the AUR, which needs paru or yay. Install one, then run this again.' >&2
    exit 1
fi

packages=$META_PACKAGES
aur=$META_AUR
if [ "$1" = applications ]; then
    packages="$packages $APPLICATION_PACKAGES"
    aur="$aur $APPLICATION_AUR"
fi
# WINE Staging replaces wine; keep it rather than swap it back.
if pacman -Q wine-staging > /dev/null 2>&1; then
    printf '%s\n' 'WINE Staging is installed; keeping it in place of wine.'
else
    packages="$packages wine"
fi

printf '%s is not in your repositories, so the same packages are installed from Arch and %s:\n' "$cachyos" "$helper"
# shellcheck disable=SC2086 # the lists are words
printf '  %s\n' $packages $aur
# shellcheck disable=SC2086
"$ESCALATION_TOOL" pacman -S --needed $packages
# shellcheck disable=SC2086
"$helper" -S --needed $aur

missing=
for name in $packages $aur; do
    pacman -Q "$name" > /dev/null 2>&1 || missing="$missing $name"
done
if [ -n "$missing" ]; then
    printf 'Not installed:%s\n' "$missing" >&2
    exit 1
fi
# shellcheck disable=SC2086
printf 'Installed and verified: %s\n' "$(printf '%s ' $packages $aur)"
