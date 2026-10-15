#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
checkCommandRequirements 'python3'
variant=${1:-commander}
case "$variant" in
    commander | cachyos | catppuccin | dracula | tela | vimix | default | ctt-cyberre | ctt-cyberpunk | ctt-shodan | ctt-fallout | ctt-dedsec | ctt-minegrub | ctt-bsol) ;;
    *) printf '%s\n' 'Unknown GRUB theme.' >&2; exit 1 ;;
esac
[ -f /etc/default/grub ] || { printf '%s\n' 'An existing GRUB installation is required.'; exit 1; }
# The Commander theme is the Garuda dr460nized theme the dotfiles GRUB settings
# name, a package in the Garuda repositories rather than files in the repository.
garuda_theme_package=grub-theme-garuda-dr460nized
garuda_theme=/usr/share/grub/themes/garuda-dr460nized
if [ "$variant" = commander ] && ! { [ "$PACKAGER" = pacman ] && pacman -Si "$garuda_theme_package" >/dev/null 2>&1; }; then
    printf 'The Commander theme is the %s package from the Garuda repositories, which this system does not have. Pick one of the other GRUB themes.\n' "$garuda_theme_package" >&2
    exit 1
fi
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
    if grep -Eq '^[[:space:]]*(export[[:space:]]+)?GRUB_(THEME|BACKGROUND|TERMINAL_OUTPUT|TERMINAL)=' "$fragment"; then
        printf 'Appearance settings in %s may override this installer. Adjust that drop-in first.\n' "$fragment" >&2
        exit 1
    fi
done
resolution=1080p
case "$variant" in
    tela | vimix)
        printf '%s\n' 'Theme asset size: 1) 1080p (default)  2) 2K  3) 4K'
        read -r answer
        case "$answer" in
            '' | 1) resolution=1080p ;;
            2) resolution=2k ;;
            3) resolution=4k ;;
            *) printf '%s\n' 'Cancelled.'; exit 1 ;;
        esac
        ;;
esac
if [ "$variant" = default ]; then
    confirm 'Restore plain GRUB: disable custom theme/background and select console output. Preserve kernel arguments, disk settings and boot entries. Back up defaults and the boot menu first.'
elif [ "$variant" = commander ]; then
    confirm "Install the $garuda_theme_package package and select its theme with graphical output. Preserve kernel arguments and disk settings. Back up defaults and the boot menu before regenerating."
    install_packages "$garuda_theme_package"
else
    confirm "Install the $variant GRUB theme with graphical output. Preserve kernel arguments and disk settings. Back up defaults, theme and boot menu before regenerating."
    command_exists git || install_packages git
    case "$variant" in
        cachyos) fetch_source dotfiles ;;
        catppuccin) fetch_source grub-catppuccin ;;
        dracula) fetch_source grub-dracula ;;
        tela | vimix) fetch_source grub-vince ;;
        ctt-*) fetch_source grub-ctt ;;
    esac
fi
work=$(mktemp -d)
new_menu=''
config_pending=false
defaults_backup=''
theme_pending=false
theme=''
cleanup_grub() {
    if [ "$config_pending" = true ]; then
        "$ESCALATION_TOOL" cp -a -- "$defaults_backup" /etc/default/grub
        printf '%s\n' 'GRUB generation failed or was interrupted. Previous defaults restored; existing boot menu preserved.' >&2
    fi
    if [ "$theme_pending" = true ]; then
        "$ESCALATION_TOOL" rm -rf -- "$theme"
    fi
    if [ -n "$new_menu" ]; then
        "$ESCALATION_TOOL" rm -f -- "$new_menu" "$new_menu.new"
    fi
    rm -rf "$work"
}
trap cleanup_grub 0
trap 'exit 130' INT
trap 'exit 143' TERM
if [ "$variant" = default ]; then
    python3 ./configure.py grub /etc/default/grub '' '' > "$work/defaults"
elif [ "$variant" = commander ]; then
    [ -f "$garuda_theme/theme.txt" ] || { printf 'The package did not provide %s/theme.txt; no changes made.\n' "$garuda_theme" >&2; exit 1; }
    python3 ./configure.py grub /etc/default/grub "$garuda_theme/theme.txt" '' > "$work/defaults"
else
    theme="$(dirname "$output")/themes/commander-$variant-$(date +%Y%m%d%H%M%S)-$$"
    python3 ./configure.py grub-prepare "$source_dir" "$work/theme" "$variant" "$resolution"
    python3 ./configure.py grub /etc/default/grub "$theme/theme.txt" '' > "$work/defaults"
    "$ESCALATION_TOOL" mkdir -p "$theme"
    theme_pending=true
    "$ESCALATION_TOOL" cp -R "$work/theme/." "$theme/"
fi
backup_system /etc/default/grub
defaults_backup=$backup
backup_system "$output"
new_menu=$("$ESCALATION_TOOL" mktemp "$(dirname "$output")/.commander-grub.XXXXXX")
config_pending=true
"$ESCALATION_TOOL" install -m 644 "$work/defaults" /etc/default/grub
"$ESCALATION_TOOL" "$generator" -o "$new_menu"
"$ESCALATION_TOOL" "$checker" "$new_menu"
"$ESCALATION_TOOL" mv -f -- "$new_menu" "$output"
config_pending=false
theme_pending=false
printf '%s\n' 'GRUB appearance updated. Reboot when ready. Existing theme assets and backups have been retained.'
