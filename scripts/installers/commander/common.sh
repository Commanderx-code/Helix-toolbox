#!/bin/sh -e

# Run from scripts/installers/commander, where HelixForge unpacks it.
# shellcheck source=scripts/shared/common-script.sh
. "$HELIXFORGE_LIB/common-script.sh"

commander_init() {
    if [ "$(id -u)" = 0 ]; then
        printf '%s\n' 'Run Commander Linutil as your normal user, not root.' >&2
        exit 1
    fi
    # These installers use native packages and must not bootstrap an AUR helper.
    AUR_HELPER_CHECKED=true
    checkEnv
    case "$PACKAGER" in
        apt-get | nala | dnf | pacman) ;;
        *) printf 'Unsupported package manager: %s\n' "$PACKAGER" >&2; exit 1 ;;
    esac
    if [ -e /run/ostree-booted ]; then
        printf '%s\n' 'Atomic/OSTree systems are not supported by these installers yet.' >&2
        exit 1
    fi
}

confirm() {
    printf '\n%s\nType APPLY to continue: ' "$1"
    read -r answer
    [ "$answer" = APPLY ] || { printf '%s\n' 'Canceled.'; exit 1; }
}

install_packages() {
    case "$PACKAGER" in
        pacman) "$ESCALATION_TOOL" pacman -S --needed "$@" ;;
        *) "$ESCALATION_TOOL" "$PACKAGER" install "$@" ;;
    esac
}

fetch_source() {
    checkCommandRequirements 'git'
    source_repo="Commanderx-code/$1"
    case "$1" in
        davincibox) source_repo=zelikos/davincibox; revision=09e9fa7b296e27db6e888e392423181e8d7bfc2a ;;
        grub-catppuccin) source_repo=catppuccin/grub; revision=0a37ab19f654e77129b409fed371891c01ffd0b9 ;;
        grub-dracula) source_repo=dracula/grub; revision=0e721d99dbf0d5d6c4fd489b88248365b7a60d12 ;;
        grub-ctt) source_repo=ChrisTitusTech/bootloader-themes; revision=e3d1b5a3fce186000b628aaa12062cb31701842a ;;
        silentsddm) source_repo=uiriansan/SilentSDDM; revision=d477613b07ca13729af20cb4699b308ac7348d2f ;;
        grub-vince) source_repo=vinceliuice/grub2-themes; revision=4c5a77125b93f833edc9bf7b14a899faa8ac79c6 ;;
        Myfish) revision=ed2ae2086265ecf3e2c92103f7e29ad9ab36d4ed ;;
        dotfiles) revision=7fc9d3090f35d30fc8e18bc85226f6fa856f8b3b ;;
        *) exit 1 ;;
    esac
    source_base="${XDG_DATA_HOME:-$HOME/.local/share}/helixforge/sources"
    mkdir -p "$source_base"
    source_dir="$source_base/$1-$revision"
    if [ ! -e "$source_dir" ]; then
        staging=$(mktemp -d "$source_base/.fetch.XXXXXX")
        trap 'rm -rf "$staging"' 0
        git -C "$staging" init -q
        git -C "$staging" remote add origin "https://github.com/$source_repo.git"
        git -C "$staging" fetch --depth=1 origin "$revision"
        git -C "$staging" checkout --detach FETCH_HEAD
        mv "$staging" "$source_dir"
        trap - 0
    fi
    [ "$(git -C "$source_dir" rev-parse HEAD)" = "$revision" ] || {
        printf '%s\n' 'Cached source revision mismatch.' >&2; exit 1;
    }
    [ -z "$(git -C "$source_dir" status --porcelain --untracked-files=all)" ] || {
        printf 'Cached source has edits: %s. Move it aside to fetch a fresh copy.\n' "$source_dir" >&2
        exit 1
    }
}

backup_system() {
    if "$ESCALATION_TOOL" test -e "$1" || "$ESCALATION_TOOL" test -L "$1"; then
        backup="$1.commander-backup-$(date +%Y%m%d%H%M%S)-$$"
        "$ESCALATION_TOOL" cp -a -- "$1" "$backup"
        printf 'Backup: %s\n' "$backup"
    fi
}
