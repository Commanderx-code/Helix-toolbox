#!/bin/sh -e
# shellcheck source=scripts/installers/commander/common.sh
. ./common.sh
commander_init
checkCommandRequirements 'python3'
look=${1:-cachyos}
case "$look" in
    cachyos) look_name='CachyOS' ;;
    garuda) look_name='Garuda dr460nized' ;;
    *) printf '%s\n' 'Unknown Limine look.' >&2; exit 1 ;;
esac
# limine.conf lives on the boot partition: where /etc/default/limine says, or
# under the usual mount points. It may be readable by root only.
esp=''
if [ -f /etc/default/limine ]; then
    esp=$(sed -n 's/^ESP_PATH=["'\'']\{0,1\}\([^"'\'' ]*\).*/\1/p' /etc/default/limine | tail -n 1)
fi
conf=''
for dir in ${esp:+"$esp"} /boot /efi /boot/efi /boot/limine /boot/EFI/limine /boot/efi/limine; do
    if "$ESCALATION_TOOL" test -f "$dir/limine.conf"; then
        conf="$dir/limine.conf"
        break
    fi
done
if [ -z "$conf" ]; then
    printf '%s\n' 'limine.conf was not found under /boot, /efi or /boot/efi; this machine does not seem to boot with Limine. No changes made.' >&2
    exit 1
fi
wallpaper=dotfiles-splash.png
confirm "Install the $look_name boot menu look from dotfiles into $conf: its wallpaper, copied beside it as $wallpaper, with its colors and a two-second timeout merged into the global settings. Boot entries and kernel arguments are not changed; the previous file is backed up."
command_exists git || install_packages git
fetch_source dotfiles
look_dir="$source_dir/limine/themes/$look"
if [ ! -f "$look_dir/limine.conf" ] || [ ! -f "$look_dir/splash.png" ]; then
    printf '%s\n' 'The dotfiles source has no Limine settings for this look.' >&2
    exit 1
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' 0
"$ESCALATION_TOOL" cp -- "$conf" "$work/current"
"$ESCALATION_TOOL" chmod 644 "$work/current"
python3 ./configure.py limine-appearance "$work/current" "$look_dir/limine.conf" > "$work/limine.conf"
backup_system "$conf"
"$ESCALATION_TOOL" install -m 644 -- "$look_dir/splash.png" "$(dirname "$conf")/$wallpaper"
"$ESCALATION_TOOL" install -m 644 -- "$work/limine.conf" "$conf"
printf 'Limine boot menu updated in %s with the %s look. It shows at the next reboot.\n' "$conf" "$look_name"
