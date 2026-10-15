#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
checkCommandRequirements 'python3'
confirm 'Install your Topgrade configuration, adjusted for this distro. Existing config is backed up. Updates run only if you choose RUN afterward.'
command_exists git || install_packages git
fetch_source dotfiles
export PATH="$HOME/.cargo/bin:$PATH"
if ! command_exists topgrade; then
    case "$PACKAGER" in
        pacman) install_packages topgrade ;;
        dnf) install_packages cargo openssl-devel pkgconf-pkg-config; cargo install --locked topgrade-rs ;;
        apt-get | nala) install_packages cargo libssl-dev pkg-config; cargo install --locked topgrade-rs ;;
    esac
fi
python3 ./configure.py topgrade "$source_dir" "$DTYPE"
printf '%s\n' 'Type RUN to run Topgrade now, or press Enter to finish installation:'
read -r answer
if [ "$answer" = RUN ]; then
    exec topgrade --config "${XDG_CONFIG_HOME:-$HOME/.config}/topgrade.toml"
fi
