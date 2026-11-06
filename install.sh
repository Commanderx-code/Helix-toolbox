#!/bin/sh -e

# Install Helix Toolbox and its window from the latest published release:
# the package for this machine's package manager (pacman, apt, zypper or dnf)
# and processor (x86_64 or aarch64), each checked against the release's
# published SHA-256 sum before the package manager sees it.
#
#   curl -fsSL https://raw.githubusercontent.com/Commanderx-code/Helix-toolbox/main/install.sh | sh
#   ... | sh -s -- --no-window    the command line and terminal interface only
#   ... | sh -s -- --yes          do not ask the package manager to confirm
#
# The whole script is one block, so a partial download runs nothing.
{
# Colours only on a terminal, as the escape characters themselves.
rc=''
red=''
green=''
if [ -t 1 ]; then
    rc=$(printf '\033[0m')
    red=$(printf '\033[0;31m')
    green=$(printf '\033[0;32m')
fi

check() {
    exit_code=$1
    message=$2
    if [ "$exit_code" -ne 0 ]; then
        printf '%sERROR: %s%s\n' "$red" "$message" "$rc"
        exit 1
    fi
    unset exit_code
    unset message
}

window=1
assume_yes=
for argument in "$@"; do
    case "$argument" in
        --no-window) window= ;;
        -y | --yes) assume_yes=1 ;;
        -h | --help)
            printf 'Usage: install.sh [--no-window] [--yes]\n'
            printf 'Installs Helix Toolbox, and its window unless --no-window, from the latest release.\n'
            exit 0
            ;;
        *) check 1 "Unknown option $argument; see --help" ;;
    esac
done

case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 deb_arch=amd64 ;;
    aarch64 | arm64) arch=aarch64 deb_arch=arm64 ;;
    *) check 1 "Releases are built for Linux x86_64 and aarch64 only; docs/INSTALL.md shows how to build from source" ;;
esac

# zypper before dnf: openSUSE can carry dnf, Fedora does not carry zypper.
if command -v pacman >/dev/null 2>&1; then
    manager=pacman
    cli_pattern="helix-toolbox-[0-9][^ /]*-$arch\\.pkg\\.tar\\.zst"
    gui_pattern="helix-toolbox-gui-[0-9][^ /]*-$arch\\.pkg\\.tar\\.zst"
elif command -v apt-get >/dev/null 2>&1; then
    manager=apt
    cli_pattern="helix-toolbox_[0-9][^ /]*_$deb_arch\\.deb"
    gui_pattern="helix-toolbox-gui_[0-9][^ /]*_$deb_arch\\.deb"
elif command -v zypper >/dev/null 2>&1; then
    manager=zypper
    cli_pattern="helix-toolbox-[0-9][^ /]*\\.$arch\\.rpm"
    gui_pattern="helix-toolbox-gui-[0-9][^ /]*\\.$arch\\.rpm"
elif command -v dnf >/dev/null 2>&1; then
    manager=dnf
    cli_pattern="helix-toolbox-[0-9][^ /]*\\.$arch\\.rpm"
    gui_pattern="helix-toolbox-gui-[0-9][^ /]*\\.$arch\\.rpm"
else
    check 1 "No pacman, apt, zypper or dnf here; docs/INSTALL.md lists the other ways to install"
fi

if [ "$(id -u)" -eq 0 ]; then
    as_root=
elif command -v sudo >/dev/null 2>&1; then
    as_root=sudo
elif command -v doas >/dev/null 2>&1; then
    as_root=doas
else
    check 1 "Installing needs root, and neither sudo nor doas is here"
fi

# Piped to sh, standard input is this script, not the keyboard: the package
# manager asks its questions on the terminal, or is told not to ask.
if [ -z "$assume_yes" ] && (exec </dev/tty) 2>/dev/null; then
    answer_from=/dev/tty
else
    assume_yes=1
    answer_from=/dev/null
fi

base=${HELIX_TOOLBOX_RELEASE:-https://github.com/Commanderx-code/Helix-toolbox/releases/latest/download}
work=$(mktemp -d) ||
    check $? "Creating the temporary directory"
trap 'rm -rf "$work"' 0
# apt reads local packages as its own unprivileged user.
chmod 755 "$work" ||
    check $? "Opening the temporary directory to the package manager"

download() {
    curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 -o "$work/$1" "$base/$1"
}

download SHA256SUMS ||
    check $? "Downloading the Helix Toolbox checksums"

# Prints the one published file whose name matches the pattern.
published() {
    matches=$(grep -E "^[0-9a-f]{64} [ *]?$1\$" "$work/SHA256SUMS" | cut -c65- | sed 's/^[ *]*//')
    if [ -z "$matches" ] || [ "$(printf '%s\n' "$matches" | wc -l)" -ne 1 ]; then
        return 1
    fi
    printf '%s\n' "$matches"
}

fetch() {
    name=$1
    download "$name" ||
        check $? "Downloading $name"
    expected=$(grep -E "^[0-9a-f]{64} [ *]?$name\$" "$work/SHA256SUMS" | cut -c1-64)
    actual=$(sha256sum -- "$work/$name" | cut -c1-64)
    if [ -z "$expected" ] || [ "$actual" != "$expected" ]; then
        check 1 "$name does not match its published checksum"
    fi
    chmod 644 "$work/$name" ||
        check $? "Opening $name to the package manager"
}

cli=$(published "$cli_pattern") ||
    check $? "Finding the Helix Toolbox package for $manager on $arch in the release"
fetch "$cli"
set -- "$work/$cli"
if [ -n "$window" ]; then
    gui=$(published "$gui_pattern") ||
        check $? "Finding the window's package for $manager on $arch in the release"
    fetch "$gui"
    set -- "$@" "$work/$gui"
fi

printf 'Installing %s%s\n' "$cli" "${gui:+ and $gui}"
case "$manager" in
    pacman) $as_root pacman -U --needed ${assume_yes:+--noconfirm} "$@" <"$answer_from" ;;
    apt) $as_root apt-get install ${assume_yes:+-y} "$@" <"$answer_from" ;;
    zypper) $as_root zypper ${assume_yes:+--non-interactive} install --allow-unsigned-rpm "$@" <"$answer_from" ;;
    dnf) $as_root dnf install ${assume_yes:+-y} "$@" <"$answer_from" ;;
esac ||
    check $? "Installing with $manager"

if [ -n "$window" ]; then
    printf '%sHelix Toolbox is installed: open it from the menu, or run helix-toolbox-gui or helix-toolbox.%s\n' "$green" "$rc"
else
    printf '%sHelix Toolbox is installed: run helix-toolbox.%s\n' "$green" "$rc"
fi
} # End of wrapping
