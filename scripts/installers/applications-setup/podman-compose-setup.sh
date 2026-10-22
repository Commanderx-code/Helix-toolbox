#!/bin/sh -e

. ../common-script.sh

installPodmanCompose() {
    if ! command_exists podman-compose; then
        printf "%b\n" "${YELLOW}Installing Podman Compose...${RC}"
        case "$PACKAGER" in
            apt-get|nala|dnf)
                "$ESCALATION_TOOL" "$PACKAGER" install -y podman-compose
                ;;
            pacman)
                "$ESCALATION_TOOL" "$PACKAGER" -S --noconfirm --needed podman-compose
                ;;
            apk)
                "$ESCALATION_TOOL" "$PACKAGER" add podman-compose
                ;;
            xbps-install)
                "$ESCALATION_TOOL" "$PACKAGER" -Sy podman-compose
                ;;
            zypper)
                if [ "$ID" = "opensuse-leap" ]; then
                    # The build service project's signing key, pinned; only this repository is refreshed.
                    key_dir=$(mktemp -d)
                    pinnedKeys "https://download.opensuse.org/repositories/devel:/languages:/python/$VERSION_ID/repodata/repomd.xml.key" \
                        "$key_dir/python.asc" 172CCD6B0CF49B7388F1C61127163A4EEDF0D733
                    "$ESCALATION_TOOL" rpm --import "$key_dir/python.asc"
                    rm -rf -- "$key_dir"
                    "$ESCALATION_TOOL" "$PACKAGER" addrepo "https://download.opensuse.org/repositories/devel:languages:python/$VERSION_ID/devel:languages:python.repo"
                    "$ESCALATION_TOOL" "$PACKAGER" refresh devel_languages_python
                    "$ESCALATION_TOOL" "$PACKAGER" install -y podman-compose
                elif [ "$ID" = "opensuse-tumbleweed" ]; then
                    "$ESCALATION_TOOL" "$PACKAGER" install -y podman-compose
                else
                    printf "%b\n" "${RED}Unsupported openSUSE distro: ${ID}${RC}"
                    exit 1
                fi
                ;;
            *)
                printf "%b\n" "${RED}Unsupported package manager: ${PACKAGER}${RC}"
                exit 1
                ;;
        esac
    else
        printf "%b\n" "${GREEN}Podman Compose is already installed.${RC}"
    fi
}

checkEnv
installPodmanCompose
