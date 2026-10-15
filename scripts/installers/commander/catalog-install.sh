#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh

commander_init
checkCommandRequirements 'python3'
source_name=${1:?Missing source}
identity=${2:?Missing tool ID}
confirm "Install $identity from $source_name with its declared distro packages and config-file backups? Symlink-managed settings are preserved."
command_exists git || install_packages git
fetch_source "$source_name"
# Validate every source and destination before package installation or config writes.
packages=$(python3 ./catalog.py packages "$source_name" "$identity" "$source_dir" "$PACKAGER")
if [ -n "$packages" ]; then
    # The catalog validator only accepts individual package names, never options.
    set -f
    # shellcheck disable=SC2086
    set -- $packages
    install_packages "$@"
fi
python3 ./catalog.py install "$source_name" "$identity" "$source_dir" "$PACKAGER"
