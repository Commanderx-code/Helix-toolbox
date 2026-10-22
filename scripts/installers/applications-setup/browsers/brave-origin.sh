#!/bin/sh -e

. ../../common-script.sh

installBraveOrigin() {
    if ! command_exists brave-origin && ! command_exists com.brave.Browser; then
        printf "%b\n" "${YELLOW}Installing Brave Origin...${RC}"
        if command_exists rpm-ostree; then
            "$ESCALATION_TOOL" curl -fsSLo /etc/yum.repos.d/brave-browser.repo https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
            "$ESCALATION_TOOL" rpm-ostree install brave-origin
        else
            case "$PACKAGER" in
                apt-get|nala)
                    "$ESCALATION_TOOL" curl -fsSLo /usr/share/keyrings/brave-browser-archive-keyring.gpg https://brave-browser-apt-release.s3.brave.com/brave-browser-archive-keyring.gpg
                    "$ESCALATION_TOOL" curl -fsSLo /etc/apt/sources.list.d/brave-browser-release.sources https://brave-browser-apt-release.s3.brave.com/brave-browser.sources
                    "$ESCALATION_TOOL" "$PACKAGER" update
                    "$ESCALATION_TOOL" "$PACKAGER" install -y brave-origin
                    ;;
                dnf)
                    "$ESCALATION_TOOL" "$PACKAGER" install -y dnf-plugins-core
                    if command_exists dnf5; then
                        "$ESCALATION_TOOL" dnf5 config-manager addrepo --from-repofile=https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
                    else
                        "$ESCALATION_TOOL" "$PACKAGER" config-manager --add-repo https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
                    fi
                    "$ESCALATION_TOOL" "$PACKAGER" install -y brave-origin
                    ;;
                zypper)
                    # Brave's signing keys, pinned; only this repository is refreshed.
                    key_dir=$(mktemp -d)
                    for brave_key in DBF1A116C220B8C7164F98230686B78420038257 47D32A74E9A9E013A4B4926C68D513D36A73CD96 \
                        B2A3DCA350E67256740DF904DE4EC67BE4B0DCA0; do
                        pinnedKeys https://brave-browser-rpm-release.s3.brave.com/brave-core.asc "$key_dir/$brave_key.asc" "$brave_key"
                        "$ESCALATION_TOOL" rpm --import "$key_dir/$brave_key.asc"
                    done
                    rm -rf -- "$key_dir"
                    "$ESCALATION_TOOL" "$PACKAGER" addrepo https://brave-browser-rpm-release.s3.brave.com/brave-browser.repo
                    "$ESCALATION_TOOL" "$PACKAGER" refresh brave-browser
                    "$ESCALATION_TOOL" "$PACKAGER" install -y brave-origin
                    ;;
                pacman)
                    checkAURHelper
                    "$AUR_HELPER" -S --needed --noconfirm brave-origin-bin
                    ;;
                *)
                    curl -fsS https://dl.brave.com/install.sh | FLAVOR=origin sh
                    ;;
            esac
        fi
    else
        printf "%b\n" "${GREEN}Brave Origin is already installed.${RC}"
    fi
}

uninstallBraveOrigin() {
    if command_exists brave-origin || command_exists com.brave.Browser; then
        printf "%b\n" "${YELLOW}Uninstalling Brave Origin...${RC}"
        if command_exists rpm-ostree; then
            "$ESCALATION_TOOL" rpm-ostree uninstall brave-origin
        else
            case "$PACKAGER" in
                apt-get|nala)
                    "$ESCALATION_TOOL" "$PACKAGER" purge --autoremove -y brave-origin
                    "$ESCALATION_TOOL" rm -f /etc/apt/sources.list.d/brave-browser-release.sources
                    "$ESCALATION_TOOL" rm -f /usr/share/keyrings/brave-browser-archive-keyring.gpg
                    ;;
                dnf)
                    "$ESCALATION_TOOL" "$PACKAGER" remove -y brave-origin
                    "$ESCALATION_TOOL" rm -f /etc/yum.repos.d/brave-browser.repo
                    ;;
                zypper)
                    "$ESCALATION_TOOL" "$PACKAGER" --non-interactive remove brave-origin
                    ;;
                pacman)
                    "$ESCALATION_TOOL" "$PACKAGER" -Rns --noconfirm brave-origin-bin
                    ;;
                *)
                    printf "%b\n" "${YELLOW}Automatic uninstall not supported for your distro.${RC}"
                    printf "%b\n" "${YELLOW}Please use your package manager to remove brave-origin manually.${RC}"
                    ;;
            esac
        fi
    else
        printf "%b\n" "${GREEN}Brave Origin is not installed.${RC}"
    fi
}

main() {
    printf "%b\n" "${YELLOW}Do you want to Install or Uninstall Brave Origin?${RC}"
    printf "%b\n" "1. ${YELLOW}Install Brave Origin${RC}"
    printf "%b\n" "2. ${YELLOW}Uninstall Brave Origin${RC}"
    printf "%b" "Enter your choice [1-2]: "
    read -r CHOICE
    case "$CHOICE" in
        1) installBraveOrigin ;;
        2) uninstallBraveOrigin ;;
        *) printf "%b\n" "${RED}Invalid choice.${RC}" && exit 1 ;;
    esac
}

checkEnv
main
