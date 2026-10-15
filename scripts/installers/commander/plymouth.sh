#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
# shellcheck source=scripts/installers/commander/kernel-cmdline.sh
. ./kernel-cmdline.sh
commander_init
checkCommandRequirements 'python3'
action=${1:-theme}
case "$action" in
    theme | dotfiles | on | off) ;;
    *) printf '%s\n' 'Unknown boot splash action.' >&2; exit 1 ;;
esac

# The splash is read from the initramfs, which each distro rebuilds differently.
initramfs_tool() {
    if command_exists dracut-rebuild; then
        printf 'dracut-rebuild'
    elif command_exists mkinitcpio; then
        printf 'mkinitcpio -P'
    elif command_exists update-initramfs; then
        printf 'update-initramfs -u -k all'
    elif command_exists dracut; then
        printf 'dracut --force --regenerate-all'
    fi
}
tool=$(initramfs_tool)
if [ -z "$tool" ]; then
    printf '%s\n' 'No initramfs tool (dracut, mkinitcpio or update-initramfs) was found; no changes made.' >&2
    exit 1
fi
rebuild_initramfs() {
    # The tool and its arguments are a fixed string chosen above.
    # shellcheck disable=SC2086
    "$ESCALATION_TOOL" $tool
}
select_theme() {
    "$ESCALATION_TOOL" plymouth-set-default-theme "$1"
    rebuild_initramfs
    printf 'Boot splash %s shows from the next reboot.\n' "$1"
}

if [ "$action" = theme ]; then
    checkCommandRequirements 'plymouth-set-default-theme'
    current=$(plymouth-set-default-theme)
    themes=$(plymouth-set-default-theme --list)
    if [ -z "$themes" ]; then
        printf '%s\n' 'No Plymouth themes are installed.' >&2
        exit 1
    fi
    printf 'Current boot splash: %s\nInstalled themes:\n' "$current"
    printf '%s\n' "$themes" | nl -w2 -s') '
    printf 'Theme number: '
    read -r answer
    case "$answer" in
        '' | *[!0-9]*) printf '%s\n' 'Cancelled.'; exit 1 ;;
    esac
    chosen=$(printf '%s\n' "$themes" | sed -n "${answer}p")
    [ -n "$chosen" ] || { printf '%s\n' 'Cancelled.'; exit 1; }
    confirm "Select the '$chosen' boot splash and rebuild the boot images with '$tool'. Kernel arguments and packages are not changed."
    select_theme "$chosen"
    exit 0
fi

if [ "$action" = dotfiles ]; then
    # The theme named in dotfiles' plymouth/theme, found the way restore-plymouth
    # finds it: already installed, kept in the repository, or a distro package.
    checkCommandRequirements 'plymouth-set-default-theme'
    command_exists git || install_packages git
    fetch_source dotfiles
    theme=$(head -n 1 "$source_dir/plymouth/theme")
    case "$theme" in
        '' | *[!A-Za-z0-9._-]*) printf '%s\n' 'dotfiles names no usable Plymouth theme.' >&2; exit 1 ;;
    esac
    themes_dir=/usr/share/plymouth/themes
    origin=''
    if [ -f "$themes_dir/$theme/$theme.plymouth" ]; then
        origin='already installed'
    elif [ -f "$source_dir/plymouth/themes/$theme/$theme.plymouth" ]; then
        origin='copied from dotfiles'
    elif [ "$PACKAGER" = pacman ] && pacman -Si "plymouth-theme-$theme" >/dev/null 2>&1; then
        origin="installed from the plymouth-theme-$theme package"
    else
        printf 'The %s theme is not installed, not kept in dotfiles and not a package here. Fetch it from the KDE Store (System Settings > Boot Splash Screen > Get New) and run this entry again, or pick an installed theme with the Boot Splash Theme entry. No changes made.\n' "$theme" >&2
        exit 1
    fi
    if [ "$(plymouth-set-default-theme)" = "$theme" ]; then
        printf 'Boot splash already current: %s. The Boot Splash Theme entry rebuilds the boot images if the splash has not appeared yet.\n' "$theme"
        exit 0
    fi
    confirm "Select the '$theme' boot splash ($origin) and rebuild the boot images with '$tool'. Kernel arguments are not changed."
    case "$origin" in
        copied*) "$ESCALATION_TOOL" cp -R --no-preserve=all -- "$source_dir/plymouth/themes/$theme" "$themes_dir/" ;;
        installed*) install_packages "plymouth-theme-$theme" ;;
    esac
    select_theme "$theme"
    exit 0
fi

# Turning the splash on or off means editing the kernel command line, which
# this installer does through /etc/default/grub or, with the Limine entry tool,
# /etc/default/limine.
cmdline_setup 'Boot splash changes need GRUB with /etc/default/grub, or Limine with the entry tool and /etc/default/limine. With systemd-boot or hand-written Limine entries, add or remove "splash" on the kernel command line yourself; no changes made.'
hook_file=''
if [ "$tool" = 'mkinitcpio -P' ] && [ -f /etc/mkinitcpio.conf ]; then
    hook_file=/etc/mkinitcpio.conf
    for fragment in /etc/mkinitcpio.conf.d/*.conf; do
        [ -f "$fragment" ] || continue
        if grep -Eq '^[[:space:]]*HOOKS=' "$fragment"; then
            printf 'The HOOKS in %s override /etc/mkinitcpio.conf. Add the plymouth hook there yourself; no changes made.\n' "$fragment" >&2
            exit 1
        fi
    done
fi
trap cmdline_cleanup 0
trap 'exit 130' INT
trap 'exit 143' TERM

if [ "$action" = on ]; then
    case "$PACKAGER" in
        apt-get | nala) packages='plymouth plymouth-themes' ;;
        *) packages=plymouth ;;
    esac
    hook_note=''
    [ -n "$hook_file" ] && hook_note=", add the plymouth hook to $hook_file"
    confirm "Install $packages, add 'splash' to the kernel command line in $cmdline_file$hook_note, regenerate the boot menu and rebuild the boot images with '$tool'. $cmdline_file, the boot menu and $hook_file are backed up first."
    # shellcheck disable=SC2086
    install_packages $packages
    if [ -n "$hook_file" ]; then
        python3 ./configure.py mkinitcpio-hooks "$hook_file" add > "$cmdline_work/mkinitcpio.conf"
        if ! cmp -s "$cmdline_work/mkinitcpio.conf" "$hook_file"; then
            backup_system "$hook_file"
            "$ESCALATION_TOOL" install -m 644 "$cmdline_work/mkinitcpio.conf" "$hook_file"
        fi
    fi
    cmdline_apply add splash
    if [ "$DTYPE" = fedora ] && command_exists grubby; then
        # Fedora's boot entries carry their own arguments, which grubby maintains.
        "$ESCALATION_TOOL" grubby --update-kernel=ALL --args=splash
    fi
    # The Limine entry tool has already rebuilt the boot images with the new arguments.
    [ "$cmdline_rebuilt_initramfs" = true ] || rebuild_initramfs
    printf '%s\n' 'Boot splash enabled from the next reboot. Pick a theme with the Boot Splash Theme entry.'
else
    confirm "Remove 'splash' and 'rhgb' from the kernel command line in $cmdline_file and regenerate the boot menu, so the system boots with text output. Plymouth, its themes and its boot image hook stay installed. $cmdline_file and the boot menu are backed up first."
    cmdline_apply remove splash rhgb
    if [ "$DTYPE" = fedora ] && command_exists grubby; then
        "$ESCALATION_TOOL" grubby --update-kernel=ALL --remove-args='splash rhgb'
    fi
    printf '%s\n' 'Boot splash disabled from the next reboot.'
fi
