#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
checkCommandRequirements 'python3'
component=${1:-}
case "$component" in
    nvim | fastfetch | starship | konsole | ghostty) ;;
    *) printf '%s\n' 'Choose a component from the Dotfiles submenu.' >&2; exit 1 ;;
esac
if [ "$component" = fastfetch ]; then
    confirm 'Install your Fastfetch configuration, automatic distro PNG launcher and shell shortcut with backups. Fastfetch will be installed if missing, together with ImageMagick image support.'
    # shellcheck source=scripts/installers/commander/fastfetch-support.sh
    . ./fastfetch-support.sh
    install_fastfetch_support
else
    confirm "Install your $component configuration with backups. Install the corresponding application separately. Shell setup is managed through Myfish; this does not activate workstation backup services."
fi
command_exists git || install_packages git
fetch_source dotfiles
python3 ./configure.py dotfiles "$source_dir" "$component"
