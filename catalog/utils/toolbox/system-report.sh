#!/bin/sh -e
# A read-only report, and the first script entry that runs inside HelixForge.
. "$HELIXFORGE_LIB/common-script.sh"
checkDistro
printf 'Distribution: %s\n' "$DTYPE"
printf 'Kernel:       %s\n' "$(uname -r)"
printf 'Architecture: %s\n' "$(uname -m)"
for manager in pacman apt-get dnf zypper apk xbps-install; do
    if command_exists "$manager"; then
        printf 'Package manager: %s\n' "$manager"
        break
    fi
done
printf 'Helpers:      %s\n' "$HELIXFORGE_LIB"
