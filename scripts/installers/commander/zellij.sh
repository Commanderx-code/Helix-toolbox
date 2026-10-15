#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh

commander_init
checkCommandRequirements 'python3'
confirm 'Install Zellij if missing and create a Tokyo Night Storm config in locked mode only if no config exists. Existing settings and shell startup files stay unchanged.'
if ! command_exists zellij; then
    case "$PACKAGER" in
        apt-get | nala)
            "$ESCALATION_TOOL" "$PACKAGER" update
            candidate=$(LC_ALL=C apt-cache policy zellij | awk '/Candidate:/ {print $2; exit}')
            if [ -z "$candidate" ] || [ "$candidate" = '(none)' ]; then
                printf '%s\n' 'Zellij is unavailable in your enabled repositories. Install it through Home Manager or a trusted upstream package, then rerun. No configuration was changed.' >&2
                exit 1
            fi
            ;;
    esac
    install_packages zellij
fi
command_exists zellij || { printf '%s\n' 'Installation did not provide the zellij command.' >&2; exit 1; }
python3 ./zellij-config.py
zellij setup --check
printf '%s\n' 'Run zellij to start, or zellij --session work for a named session.' 'With the new preset: Ctrl+G unlocks/locks controls; when unlocked, Ctrl+O then d detaches.' 'No automatic startup was added. Existing configurations keep their own shortcuts.'
