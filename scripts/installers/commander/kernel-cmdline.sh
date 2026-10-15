#!/bin/sh -e
# Shared kernel command line transaction, sourced after common.sh. It edits
# GRUB_CMDLINE_LINUX_DEFAULT in /etc/default/grub and regenerates the GRUB
# menu, or, on a Limine machine whose entries the Limine entry tool manages
# (CachyOS), KERNEL_CMDLINE[default] in /etc/default/limine followed by the
# tool's regeneration, which also rebuilds the boot images.
#
# cmdline_setup MESSAGE        picks the boot loader and refuses drop-ins that
#                              override the file; MESSAGE explains what to do
#                              when neither boot loader is found.
# cmdline_apply ACTION WORD...  add, remove or set words, writing the file only
#                              when the generated menu passes its checks.
# cmdline_cleanup              restores the previous file after a failure;
#                              callers run it from their exit trap.

cmdline_backend=''
cmdline_file=''
cmdline_rebuilt_initramfs=false
cmdline_work=''
cmdline_new_menu=''
cmdline_pending=false
cmdline_backup=''

cmdline_setup() {
    # The grub package leaves /etc/default/grub behind even where Limine boots,
    # so a machine with the Limine entry tool is a Limine machine.
    if [ -f /etc/default/limine ] && { command_exists limine-mkinitcpio || command_exists limine-dracut; }; then
        cmdline_backend=limine
        cmdline_file=/etc/default/limine
        if command_exists limine-mkinitcpio; then
            regenerate=limine-mkinitcpio
        else
            regenerate=limine-dracut
        fi
    elif [ -f /etc/default/grub ]; then
        cmdline_backend=grub
        cmdline_file=/etc/default/grub
        if [ "$DTYPE" = fedora ]; then
            generator=grub2-mkconfig
            checker=grub2-script-check
            output=/boot/grub2/grub.cfg
        elif command_exists grub-mkconfig && [ -d /boot/grub ]; then
            generator=grub-mkconfig
            checker=grub-script-check
            output=/boot/grub/grub.cfg
        else
            printf '%s\n' 'Unsupported GRUB layout; no changes made.' >&2
            exit 1
        fi
        checkCommandRequirements "$generator $checker"
        # Debian drop-ins may override /etc/default/grub. Do not silently report success.
        for fragment in /etc/default/grub.d/*.cfg; do
            [ -f "$fragment" ] || continue
            if grep -Eq '^[[:space:]]*(export[[:space:]]+)?GRUB_CMDLINE_LINUX(_DEFAULT)?=' "$fragment"; then
                printf 'Kernel arguments in %s override /etc/default/grub. Adjust that drop-in first; no changes made.\n' "$fragment" >&2
                exit 1
            fi
        done
    elif [ -f /etc/default/limine ]; then
        printf '%s\n' 'The Limine entry tool (limine-mkinitcpio-hook or limine-dracut-support) is needed to carry kernel arguments into the boot entries; no changes made.' >&2
        exit 1
    else
        printf '%s\n' "$1" >&2
        exit 1
    fi
    cmdline_work=$(mktemp -d)
}

cmdline_cleanup() {
    if [ "$cmdline_pending" = true ]; then
        "$ESCALATION_TOOL" cp -a -- "$cmdline_backup" "$cmdline_file"
        printf 'Boot menu generation failed or was interrupted. Previous %s restored; existing boot menu preserved.\n' "$cmdline_file" >&2
    fi
    if [ -n "$cmdline_new_menu" ]; then
        "$ESCALATION_TOOL" rm -f -- "$cmdline_new_menu"
    fi
    if [ -n "$cmdline_work" ]; then
        rm -rf "$cmdline_work"
    fi
}

cmdline_apply() {
    if [ "$cmdline_backend" = limine ]; then
        python3 ./configure.py limine-cmdline "$cmdline_file" "$@" > "$cmdline_work/defaults"
    else
        python3 ./configure.py grub-cmdline "$cmdline_file" "$@" > "$cmdline_work/defaults"
    fi
    if cmp -s "$cmdline_work/defaults" "$cmdline_file"; then
        printf 'Kernel arguments already as requested; %s unchanged.\n' "$cmdline_file"
        return 0
    fi
    backup_system "$cmdline_file"
    # backup_system, from common.sh, leaves the copy's path in backup.
    # shellcheck disable=SC2154
    cmdline_backup=$backup
    cmdline_pending=true
    "$ESCALATION_TOOL" install -m 644 "$cmdline_work/defaults" "$cmdline_file"
    if [ "$cmdline_backend" = limine ]; then
        # The entry tool rewrites the entries from the new parameters and rebuilds
        # the boot images on the way.
        "$ESCALATION_TOOL" "$regenerate"
        # Read by plymouth.sh, which otherwise rebuilds the boot images itself.
        # shellcheck disable=SC2034
        cmdline_rebuilt_initramfs=true
    else
        backup_system "$output"
        cmdline_new_menu=$("$ESCALATION_TOOL" mktemp "$(dirname "$output")/.commander-grub.XXXXXX")
        "$ESCALATION_TOOL" "$generator" -o "$cmdline_new_menu"
        "$ESCALATION_TOOL" "$checker" "$cmdline_new_menu"
        "$ESCALATION_TOOL" mv -f -- "$cmdline_new_menu" "$output"
        cmdline_new_menu=''
    fi
    cmdline_pending=false
}
