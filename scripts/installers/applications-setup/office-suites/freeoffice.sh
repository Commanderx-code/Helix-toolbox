#!/bin/sh -e

. ../../common-script.sh

installFreeOffice() {
    if ! command_exists softmaker-freeoffice-2024 freeoffice softmaker; then
        printf "%b\n" "${YELLOW}Installing Free Office...${RC}"
        case "$PACKAGER" in
        apt-get|nala)
            curl -O https://www.softmaker.net/down/softmaker-freeoffice-2024_1218-01_amd64.deb
            "$ESCALATION_TOOL" "$PACKAGER" install -y ./softmaker-freeoffice-2024_1218-01_amd64.deb
            "$ESCALATION_TOOL" rm ./softmaker-freeoffice-2024_1218-01_amd64.deb
            ;;
        zypper)
            # SoftMaker's signing key, pinned; only this repository is refreshed.
            key_dir=$(mktemp -d)
            pinnedKeys https://shop.softmaker.com/repo/rpm/softmaker-repo.asc "$key_dir/softmaker.asc" CC7D2EDF4808EFFA0E00FC723413DA98AA3E7F5E
            "$ESCALATION_TOOL" rpm --import "$key_dir/softmaker.asc"
            rm -rf -- "$key_dir"
            "$ESCALATION_TOOL" "$PACKAGER" addrepo -f https://shop.softmaker.com/repo/rpm SoftMaker
            "$ESCALATION_TOOL" "$PACKAGER" refresh SoftMaker
            "$ESCALATION_TOOL" "$PACKAGER" --non-interactive install softmaker-freeoffice-2024
            ;;
        pacman)
            "$AUR_HELPER" -S --needed --noconfirm freeoffice
            ;;
        dnf)
            "$ESCALATION_TOOL" curl -O -qO /etc/yum.repos.d/softmaker.repo https://shop.softmaker.com/repo/softmaker.repo
            "$ESCALATION_TOOL" "$PACKAGER" install -y softmaker-freeoffice-2024
            ;;
        *)
            printf "%b\n" "${RED}Unsupported package manager: ""$PACKAGER""${RC}"
            exit 1
            ;;
        esac
    else
        printf "%b\n" "${GREEN}Free Office is already installed.${RC}"
    fi
}

checkEnv
installFreeOffice
