#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh

tool=${1:?Missing tool}
case "$tool" in
    netwatch | tfm | cassette) ;;
    *) printf '%s\n' 'Unknown terminal tool.' >&2; exit 1 ;;
esac
[ "$(id -u)" != 0 ] || { printf '%s\n' 'Run this installer as your normal user.' >&2; exit 1; }
[ "$(uname -s)-$(uname -m)" = Linux-x86_64 ] || {
    printf '%s\n' 'These pinned releases require Linux x86_64.' >&2; exit 1;
}
checkCommandRequirements 'nix git'
if command_exists "$tool"; then
    printf '%s is already available at %s. Update it through its existing package manager (hm-rebuild on the Commander workstation).\n' "$tool" "$(command -v "$tool")"
    exit 0
fi
confirm "Install the pinned $tool package from tested dotfiles using your Nix profile? Cassette includes librespot; Spotify login is separate."
fetch_source dotfiles
nix --extra-experimental-features 'nix-command flakes' profile install \
    "path:$source_dir?dir=home-manager#$tool"
printf 'Installed %s. Open a new terminal if its command is not on PATH yet.\n' "$tool"
if [ "$tool" = cassette ]; then
    printf '%s\n' 'See dotfiles/configs/terminal-tools/README.md for local Spotify Client ID and sign-in setup. Never commit login data.'
fi
