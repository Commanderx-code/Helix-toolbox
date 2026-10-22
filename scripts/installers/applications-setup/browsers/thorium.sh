#!/bin/sh -e

. ../../common-script.sh

installThrorium() {
    if ! command_exists thorium-browser; then
        printf "%b\n" "${YELLOW}Installing Thorium Browser...${RC}"
        case "$PACKAGER" in
            apt-get|nala)
                # Thorium's repository is unsigned (its own list says trusted=yes), so
                # HTTPS is all that vouches for it; the line is written here, so what
                # the network serves cannot name other repositories or options.
                printf '%s\n' 'deb [trusted=yes arch=amd64] https://dl.thorium.rocks/debian/ stable main' |
                    "$ESCALATION_TOOL" tee /etc/apt/sources.list.d/thorium.list >/dev/null
                "$ESCALATION_TOOL" "$PACKAGER" update
                "$ESCALATION_TOOL" "$PACKAGER" install -y thorium-browser
                ;;
            zypper|dnf)
                latest_release=$(curl -s https://api.github.com/repos/Alex313031/Thorium/releases/latest)
                rpm_urls=$(echo "$latest_release" | grep -o 'https://[^"]*\.rpm')

                for feature in avx2 avx sse4 sse3; do
                    if grep -q "$feature" /proc/cpuinfo; then
                        printf "%b\n" "${GREEN}$(echo "$feature" | tr '[:lower:]' '[:upper:]') support detected, using $(echo "$feature" | tr '[:lower:]' '[:upper:]') version${RC}"
                        url=$(echo "$rpm_urls" | grep "$(echo "$feature" | tr '[:lower:]' '[:upper:]')" | head -n1)
                        break
                    fi
                done

                if [ -z "$url" ]; then
                    printf "%b\n" "${RED}Failed to find appropriate RPM package${RC}"
                    printf "%b\n" "${YELLOW}Available packages:${RC}"
                    echo "$rpm_urls"
                    exit 1
                fi

                printf "%b\n" "${YELLOW}Downloading Thorium Browser RPM from: $url${RC}"
                curl -L "$url" -o thorium-latest.rpm
                if [ "$PACKAGER" = "zypper" ]; then
                    "$ESCALATION_TOOL" rpm -i thorium-latest.rpm && rm thorium-latest.rpm
                else
                    "$ESCALATION_TOOL" "$PACKAGER" install -y thorium-latest.rpm && rm thorium-latest.rpm
                fi
                ;;
            pacman)
                "$AUR_HELPER" -S --needed --noconfirm thorium-browser-bin
                ;;
            *)
                printf "%b\n" "${RED}Unsupported package manager: ""$PACKAGER""${RC}"
                exit 1
                ;;
        esac
    else
        printf "%b\n" "${GREEN}Thorium Browser is already installed.${RC}"
    fi
}

checkEnv
installThrorium
