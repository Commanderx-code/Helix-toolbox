#!/bin/sh -e
# shellcheck source=core/tabs/common-script.sh
. ../../common-script.sh

# Check the distro before environment setup or package operations.
ID='' ID_LIKE=''
. /etc/os-release
case " $ID $ID_LIKE " in
    *" debian "* | *" ubuntu "*) ;;
    *) printf '%s\n' 'This installer supports Debian, Ubuntu and their derivatives.' >&2; exit 1 ;;
esac
[ "$(id -u)" != 0 ] || { printf '%s\n' 'Run as your normal user so the alias belongs to your account.' >&2; exit 1; }
checkEnv
checkCommandRequirements 'apt-get apt-cache python3'
login_shell=${SHELL##*/}
python3 ../../commander/configure.py nala-check "$HOME" "$login_shell"
printf '%s\n' 'Install Nala from your distribution repositories and make apt an interactive alias for nala in your login shell.' 'Existing shell configuration is backed up. APT stays installed. Use sudo nala for elevated commands.'
printf '%s' 'Type APPLY to continue: '
read -r answer
[ "$answer" = APPLY ] || { printf '%s\n' 'Cancelled.'; exit 1; }
if ! command_exists nala; then
    "$ESCALATION_TOOL" apt-get update
    candidate=$(LC_ALL=C apt-cache policy nala | awk '/Candidate:/ {print $2; exit}')
    if [ -z "$candidate" ] || [ "$candidate" = '(none)' ]; then
        printf '%s\n' 'Nala is unavailable in your enabled repositories. On Ubuntu/Zorin, check that Universe is enabled and your OS release provides Nala. No shell aliases were changed.' >&2
        exit 1
    fi
    "$ESCALATION_TOOL" apt-get install nala
fi
command_exists nala || { printf '%s\n' 'Nala installation did not provide the nala command.' >&2; exit 1; }
python3 ../../commander/configure.py nala-alias "$HOME" "$login_shell"
printf '%s\n' 'Open a new terminal to use apt as an alias for nala.' 'Examples: apt list --installed; sudo nala install PACKAGE.' 'For original APT behavior, use /usr/bin/apt. sudo apt also continues to use APT.'
