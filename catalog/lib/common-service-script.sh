#!/bin/sh -e
# Service helpers for systemd, OpenRC and runit. Sourced after common-script.sh;
# sets INIT_MANAGER to systemctl, rc-service or sv.

checkInitManager() {
    for name in $1; do
        if command_exists "$name"; then
            INIT_MANAGER=$name
            printf '%b\n' "${CYAN}Init system: ${name}${RC}"
            break
        fi
    done
    if [ -z "$INIT_MANAGER" ]; then
        printf '%b\n' "${RED}No supported init system was found${RC}"
        exit 1
    fi
}

startService() {
    case "$INIT_MANAGER" in
        systemctl | sv) "$ESCALATION_TOOL" "$INIT_MANAGER" start "$1" ;;
        rc-service) "$ESCALATION_TOOL" "$INIT_MANAGER" "$1" start ;;
    esac
}

stopService() {
    case "$INIT_MANAGER" in
        systemctl | sv) "$ESCALATION_TOOL" "$INIT_MANAGER" stop "$1" ;;
        rc-service) "$ESCALATION_TOOL" "$INIT_MANAGER" "$1" stop ;;
    esac
}

enableService() {
    case "$INIT_MANAGER" in
        systemctl) "$ESCALATION_TOOL" "$INIT_MANAGER" enable "$1" ;;
        rc-service) "$ESCALATION_TOOL" rc-update add "$1" ;;
        sv)
            if [ -d /etc/service ]; then
                "$ESCALATION_TOOL" ln -sf "/etc/sv/$1" /etc/service/
            else
                "$ESCALATION_TOOL" ln -sf "/etc/sv/$1" /var/service/
            fi
            ;;
    esac
}

disableService() {
    case "$INIT_MANAGER" in
        systemctl) "$ESCALATION_TOOL" "$INIT_MANAGER" disable "$1" ;;
        rc-service) "$ESCALATION_TOOL" rc-update del "$1" ;;
        sv) "$ESCALATION_TOOL" rm -f "/etc/service/$1" "/var/service/$1" ;;
    esac
}

startAndEnableService() {
    case "$INIT_MANAGER" in
        systemctl) "$ESCALATION_TOOL" "$INIT_MANAGER" enable --now "$1" ;;
        *)
            enableService "$1"
            startService "$1"
            ;;
    esac
}

isServiceActive() {
    case "$INIT_MANAGER" in
        systemctl) "$ESCALATION_TOOL" "$INIT_MANAGER" is-active --quiet "$1" ;;
        rc-service) "$ESCALATION_TOOL" "$INIT_MANAGER" "$1" status --quiet ;;
        sv) "$ESCALATION_TOOL" "$INIT_MANAGER" status "$1" >/dev/null 2>&1 ;;
    esac
}

checkInitManager 'systemctl rc-service sv'
