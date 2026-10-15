#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
[ "$DTYPE" = fedora ] || { printf '%s\n' 'This entry supports Fedora only.'; exit 1; }
if command_exists getenforce; then getenforce; fi
confirm 'Install Fedora SELinux policy, management and diagnostic tools. This preserves the current enforcement mode and local policy. Disabled systems need a separate relabel/re-enable procedure.'
install_packages selinux-policy-targeted policycoreutils policycoreutils-python-utils libselinux-utils setroubleshoot-server
sestatus
printf '%s\n' 'SELinux tools are ready. Useful commands: sestatus, sudo semanage boolean -l, sudo ausearch -m AVC -ts recent.'
