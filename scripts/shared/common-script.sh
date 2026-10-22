#!/bin/sh -e
# Shared helpers for every catalog script. Sourced, never run. A script calls
# checkEnv once, then uses the variables and helpers below.
#
# Variables set by checkEnv:
#   ARCH             x86_64 or aarch64
#   ESCALATION_TOOL  sudo, doas, or env when already root
#   PACKAGER         the package manager command: nala, apt-get, dnf, pacman, zypper, apk, xbps-install or eopkg
#   DTYPE            the distribution ID from /etc/os-release, or unknown
#   AUR_HELPER       paru or yay on Arch, installed when missing unless AUR_HELPER_CHECKED is set

# shellcheck disable=SC2034
RC='\033[0m'
RED='\033[31m'
YELLOW='\033[33m'
CYAN='\033[36m'
GREEN='\033[32m'

# True when every named command is on the PATH, Flatpak exports included.
command_exists() {
    export PATH="$HOME/.local/share/flatpak/exports/bin:/var/lib/flatpak/exports/bin:$PATH"
    for name in "$@"; do
        command -v "$name" >/dev/null 2>&1 || return 1
    done
    return 0
}

checkArch() {
    case "$(uname -m)" in
        x86_64 | amd64) ARCH=x86_64 ;;
        aarch64 | arm64) ARCH=aarch64 ;;
        *)
            printf '%b\n' "${RED}Unsupported architecture: $(uname -m)${RC}"
            exit 1
            ;;
    esac
    printf '%b\n' "${CYAN}Architecture: ${ARCH}${RC}"
}

checkEscalationTool() {
    [ -z "$ESCALATION_TOOL_CHECKED" ] || return 0
    if [ "$(id -u)" = 0 ]; then
        # env runs the command as given, without re-parsing its arguments.
        ESCALATION_TOOL='env'
        printf '%b\n' "${CYAN}Running as root; no privilege escalation needed${RC}"
    elif command_exists sudo; then
        ESCALATION_TOOL=sudo
    elif command_exists doas; then
        ESCALATION_TOOL=doas
    else
        printf '%b\n' "${RED}Neither sudo nor doas is installed${RC}"
        exit 1
    fi
    [ "$ESCALATION_TOOL" = env ] || printf '%b\n' "${CYAN}Privilege escalation: ${ESCALATION_TOOL}${RC}"
    ESCALATION_TOOL_CHECKED=true
}

# Exit unless every command in the space-separated list exists.
checkCommandRequirements() {
    for name in $1; do
        if ! command_exists "$name"; then
            printf '%b\n' "${RED}This entry needs: $1${RC}"
            exit 1
        fi
    done
}

# Pick the first installed package manager from the space-separated list.
checkPackageManager() {
    for name in $1; do
        if command_exists "$name"; then
            PACKAGER=$name
            printf '%b\n' "${CYAN}Package manager: ${name}${RC}"
            break
        fi
    done
    if [ -z "$PACKAGER" ]; then
        printf '%b\n' "${RED}No supported package manager was found${RC}"
        exit 1
    fi
    # Alpine keeps its community repository commented out on a fresh install.
    if [ "$PACKAGER" = apk ] && grep -qE '^#.*community' /etc/apk/repositories; then
        "$ESCALATION_TOOL" sed -i '/community/s/^#//' /etc/apk/repositories
        "$ESCALATION_TOOL" "$PACKAGER" update
    fi
}

checkSuperUser() {
    for group in wheel sudo root; do
        case " $(id -nG) " in
            *" $group "*)
                SUGROUP=$group
                printf '%b\n' "${CYAN}Administrator group: ${group}${RC}"
                return 0
                ;;
        esac
    done
    printf '%b\n' "${RED}Your account is not in the wheel, sudo or root group${RC}"
    exit 1
}

checkCurrentDirectoryWritable() {
    GITPATH=$(dirname "$(realpath "$0")")
    if [ ! -w "$GITPATH" ]; then
        printf '%b\n' "${RED}Cannot write to ${GITPATH}${RC}"
        exit 1
    fi
}

checkDistro() {
    DTYPE=unknown
    if [ -f /etc/os-release ]; then
        # shellcheck disable=SC1091
        . /etc/os-release
        DTYPE=$ID
    fi
}

# On Arch, find paru or yay; install yay from the AUR when neither exists.
checkAURHelper() {
    [ "$PACKAGER" = pacman ] || return 0
    [ -z "$AUR_HELPER_CHECKED" ] || return 0
    for helper in paru yay; do
        if command_exists "$helper"; then
            AUR_HELPER=$helper
            printf '%b\n' "${CYAN}AUR helper: ${helper}${RC}"
            AUR_HELPER_CHECKED=true
            return 0
        fi
    done
    printf '%b\n' "${YELLOW}Installing yay as the AUR helper...${RC}"
    "$ESCALATION_TOOL" "$PACKAGER" -S --needed --noconfirm base-devel git
    build_dir=$(mktemp -d)
    trap 'rm -rf "$build_dir"' 0
    git clone https://aur.archlinux.org/yay-bin.git "$build_dir/yay-bin"
    (cd "$build_dir/yay-bin" && makepkg --noconfirm -si)
    rm -rf "$build_dir"
    trap - 0
    if ! command_exists yay; then
        printf '%b\n' "${RED}The AUR helper could not be installed${RC}"
        exit 1
    fi
    AUR_HELPER=yay
    AUR_HELPER_CHECKED=true
}

# Install Flatpak and the Flathub remote when missing.
checkFlatpak() {
    if ! command_exists flatpak; then
        printf '%b\n' "${YELLOW}Installing Flatpak...${RC}"
        case "$PACKAGER" in
            pacman) "$ESCALATION_TOOL" "$PACKAGER" -S --needed --noconfirm flatpak ;;
            apk) "$ESCALATION_TOOL" "$PACKAGER" add flatpak ;;
            xbps-install) "$ESCALATION_TOOL" "$PACKAGER" -Sy flatpak ;;
            *) "$ESCALATION_TOOL" "$PACKAGER" install -y flatpak ;;
        esac
        printf '%b\n' "${YELLOW}Applications installed with Flatpak appear after the next login${RC}"
    fi
    if ! flatpak remotes | grep -q flathub; then
        printf '%b\n' "${YELLOW}Adding the Flathub remote...${RC}"
        "$ESCALATION_TOOL" flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
    else
        printf '%b\n' "${CYAN}Flatpak is ready${RC}"
    fi
}

# Fetch a vendor's signing keys over HTTPS and keep only the pinned ones:
# pinnedKeys URL OUTPUT FINGERPRINT... writes exactly those keys to OUTPUT,
# binary when it ends in .gpg (an apt keyring) and armored otherwise (for
# rpm --import), and fails when the file lacks any of them. Whatever else the
# file holds stays behind.
pinnedKeys() {
    pinned_url=$1
    pinned_output=$2
    shift 2
    if ! command_exists gpg; then
        printf '%b\n' "${YELLOW}Installing GnuPG to check the signing key...${RC}"
        case "$PACKAGER" in
            apt-get | nala) "$ESCALATION_TOOL" "$PACKAGER" install -y gnupg ;;
            zypper) "$ESCALATION_TOOL" "$PACKAGER" --non-interactive install gpg2 ;;
            *) "$ESCALATION_TOOL" "$PACKAGER" install -y gnupg2 ;;
        esac
    fi
    pinned_work=$(mktemp -d)
    mkdir -m 700 "$pinned_work/gnupg"
    pinned_armor=--armor
    case "$pinned_output" in *.gpg) pinned_armor= ;; esac
    # shellcheck disable=SC2086 # $pinned_armor is empty or one option.
    if ! curl -fsSL --proto '=https' "$pinned_url" -o "$pinned_work/key.asc" ||
        ! GNUPGHOME="$pinned_work/gnupg" gpg --batch --quiet --import "$pinned_work/key.asc" ||
        ! GNUPGHOME="$pinned_work/gnupg" gpg --batch --quiet $pinned_armor --export "$@" > "$pinned_work/pinned" ||
        ! pinned_listing=$(GNUPGHOME="$pinned_work/gnupg" gpg --batch --quiet --show-keys --with-colons "$pinned_work/pinned"); then
        pinned_listing=
    fi
    pinned_found=$(printf '%s\n' "$pinned_listing" |
        awk -F: '$1 == "pub" { want = 1; next } want && $1 == "fpr" { print $10; want = 0 }' | sort)
    pinned_wanted=$(printf '%s\n' "$@" | sort)
    if [ -z "$pinned_listing" ] || [ "$pinned_found" != "$pinned_wanted" ]; then
        rm -rf -- "$pinned_work"
        printf '%b\n' "${RED}The signing key at $pinned_url is not the one expected; nothing was added.${RC}" >&2
        return 1
    fi
    "$ESCALATION_TOOL" install -D -m 644 "$pinned_work/pinned" "$pinned_output"
    rm -rf -- "$pinned_work"
}

checkEnv() {
    checkArch
    checkEscalationTool
    checkCommandRequirements "curl groups $ESCALATION_TOOL"
    checkPackageManager 'nala apt-get dnf pacman zypper apk xbps-install eopkg'
    checkCurrentDirectoryWritable
    checkSuperUser
    checkDistro
    checkAURHelper
}
