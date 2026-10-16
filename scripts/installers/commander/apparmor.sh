#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
# shellcheck source=scripts/installers/commander/kernel-cmdline.sh
. ./kernel-cmdline.sh
commander_init
. "$HELIXFORGE_LIB/common-service-script.sh"
if [ "$DTYPE" = fedora ]; then
    printf '%s\n' 'Use the Fedora SELinux entry on this system.' >&2
    exit 1
fi
case "$PACKAGER" in
    pacman | apt-get | nala) ;;
    *) printf '%s\n' 'AppArmor setup currently supports Arch and Debian/Ubuntu families.'; exit 1 ;;
esac
if [ ! -r /sys/module/apparmor/parameters/enabled ] || ! grep -q '^Y' /sys/module/apparmor/parameters/enabled; then
    printf '%s\n' 'AppArmor is not enabled in the running kernel.'
    # Arch kernels build AppArmor in but start it only when the lsm= parameter
    # names it, the same parameter the dotfiles boot settings carry.
    if [ "$PACKAGER" = pacman ] && { [ -f /etc/default/grub ] || [ -f /etc/default/limine ]; }; then
        checkCommandRequirements 'python3'
        lsm='lsm=landlock,lockdown,yama,integrity,apparmor,bpf'
        cmdline_setup 'AppArmor needs the lsm= kernel parameter; add it in your boot loader yourself, reboot, then rerun this entry.'
        confirm "Add $lsm to the kernel command line in $cmdline_file (replacing any lsm= setting) and regenerate the boot menu, so the kernel starts AppArmor at the next boot. $cmdline_file and the boot menu are backed up first. Reboot, then run this entry again to install the tools and enable the service."
        trap cmdline_cleanup 0
        trap 'exit 130' INT
        trap 'exit 143' TERM
        cmdline_apply set "$lsm"
        printf '%s\n' 'Reboot, then run this entry again to install the AppArmor tools and enable the service.'
        exit 0
    fi
    printf '%s\n' 'Enable it using your distro boot configuration and reboot, then rerun this entry. No boot settings were changed.'
    exit 1
fi
confirm 'Install AppArmor tools and distro-provided profiles, then enable and start the AppArmor service. Existing enabled profiles may begin enforcing restrictions.'
case "$PACKAGER" in
    pacman) install_packages apparmor ;;
    *) install_packages apparmor apparmor-utils apparmor-profiles ;;
esac
startAndEnableService apparmor
# Right after the package is installed, systemd can still hold the answer it
# had before about AppArmor, and skip the service; asked again, it starts.
if [ "${INIT_MANAGER:-}" = systemctl ] && ! isServiceActive apparmor; then
    "$ESCALATION_TOOL" systemctl daemon-reexec
    "$ESCALATION_TOOL" systemctl start apparmor
fi
"$ESCALATION_TOOL" aa-status
