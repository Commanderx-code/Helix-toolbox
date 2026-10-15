#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
confirm 'Install Myfish from its pinned source. Its guided installer will ask for your shell and confirm configuration changes. Fedora should use the native backend to keep SELinux enabled.'
command_exists git || install_packages git
fetch_source Myfish
cd "$source_dir"
exec bash ./install.sh --apply "$@"
