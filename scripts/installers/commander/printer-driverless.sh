#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
# shellcheck source=scripts/shared/common-service-script.sh
. "$HELIXFORGE_LIB/common-service-script.sh"
commander_init
checkInitManager 'systemctl'
[ -d /run/systemd/system ] || { printf '%s\n' 'This setup requires a running systemd installation.' >&2; exit 1; }
confirm 'Install CUPS and local network discovery, then enable their services? This supports IPP Everywhere printers; older models may still need vendor drivers.'
case "$PACKAGER" in
    pacman) install_packages cups avahi ;;
    apt-get | nala) install_packages cups cups-client cups-ipp-utils avahi-daemon avahi-utils ;;
    dnf) install_packages cups cups-client cups-ipptool avahi avahi-tools ;;
    *) printf '%s\n' 'Unsupported package manager.' >&2; exit 1 ;;
esac
startAndEnableService cups
startAndEnableService avahi-daemon
printf '%s\n' 'Discovered IPP printers (printer must be powered on and reachable on your network):'
if command_exists ippfind; then
    ippfind --timeout 5 --print || printf '%s\n' 'No printer discovered. You can enter its IPP URI manually.'
fi
printf '%s\n' 'Use an ipp:// or ipps:// URI from discovery or your printer settings.'
printf '%s' 'Printer URI (Enter finishes without adding a queue): '
read -r uri
[ -n "$uri" ] || exit 0
case "$uri" in ipp://?* | ipps://?*) ;; *) printf '%s\n' 'An IPP or IPPS URI is required.' >&2; exit 1 ;; esac
printf '%s' 'New queue name (letters, digits, underscore or hyphen): '
read -r queue
case "$queue" in '' | -* | *[!A-Za-z0-9_-]*) printf '%s\n' 'Invalid queue name.' >&2; exit 1 ;; esac
if lpstat -p "$queue" >/dev/null 2>&1; then
    printf '%s\n' 'That queue already exists. Choose a new name or manage it in CUPS; existing settings were preserved.' >&2
    exit 1
fi
confirm "Create printer queue $queue for $uri using IPP Everywhere? The queue will not be shared."
"$ESCALATION_TOOL" lpadmin -p "$queue" -E -v "$uri" -m everywhere -o printer-is-shared=false
lpstat -p "$queue"
printf '%s\n' 'Printer added. Manage options or print a test page from your desktop printer settings or http://localhost:631.'
