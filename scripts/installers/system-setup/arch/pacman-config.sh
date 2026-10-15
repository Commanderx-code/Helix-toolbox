#!/bin/sh -e

. ../../common-script.sh

# Turn on pacman's colour, Pac-Man progress bar, package lists in columns,
# parallel downloads and multilib, and set MAKEFLAGS to every core. Each
# setting changes only when it is off, so running this again changes
# nothing; repeated ILoveCandy lines that older versions added are removed.
# Each file is edited as a copy, backed up, then replaced in one write.

PACMAN_CONF=/etc/pacman.conf
MAKEPKG_CONF=/etc/makepkg.conf

report() {
    printf '  %-18s %s\n' "$1" "$2"
}

# A plain option in [options]: uncomment it, or add it after [options].
enableOption() {
    option=$1
    file=$2
    if grep -q "^${option}\$" "$file"; then
        report "$option" "already on"
    elif grep -q "^#${option}\$" "$file"; then
        sed -i "s/^#${option}\$/${option}/" "$file"
        CHANGED=1
        report "$option" "turned on"
    else
        sed -i "/^\[options\]/a ${option}" "$file"
        CHANGED=1
        report "$option" "added"
    fi
}

enableCandy() {
    file=$1
    count=$(grep -c '^ILoveCandy$' "$file" || true)
    if [ "$count" -eq 0 ]; then
        sed -i '/^Color$/a ILoveCandy' "$file"
        CHANGED=1
        report ILoveCandy "added"
    elif [ "$count" -gt 1 ]; then
        awk '!/^ILoveCandy$/ || !seen++' "$file" > "$file.new"
        mv "$file.new" "$file"
        CHANGED=1
        report ILoveCandy "already on; removed $((count - 1)) repeated line(s)"
    else
        report ILoveCandy "already on"
    fi
}

enableParallelDownloads() {
    file=$1
    if grep -q '^ParallelDownloads' "$file"; then
        report ParallelDownloads "already on ($(sed -n 's/^ParallelDownloads *= *//p' "$file" | head -n 1))"
    elif grep -q '^#ParallelDownloads' "$file"; then
        sed -i 's/^#ParallelDownloads/ParallelDownloads/' "$file"
        CHANGED=1
        report ParallelDownloads "turned on ($(sed -n 's/^ParallelDownloads *= *//p' "$file" | head -n 1))"
    else
        sed -i '/^\[options\]/a ParallelDownloads = 5' "$file"
        CHANGED=1
        report ParallelDownloads "added (5)"
    fi
}

enableMultilib() {
    file=$1
    if grep -q '^\[multilib\]' "$file"; then
        report multilib "already on"
    elif grep -q '^#\[multilib\]' "$file"; then
        sed -i '/^#\[multilib\]/,/^#Include/ s/^#//' "$file"
        CHANGED=1
        report multilib "turned on"
    else
        printf '\n[multilib]\nInclude = /etc/pacman.d/mirrorlist\n' >> "$file"
        CHANGED=1
        report multilib "added"
    fi
}

# MAKEFLAGS of the plain -jN form follows the core count; custom flags stay.
setMakeflags() {
    file=$1
    cores=$(nproc)
    if grep -q "^MAKEFLAGS=\"-j${cores}\"\$" "$file"; then
        report MAKEFLAGS "already -j${cores}"
    elif grep -q '^MAKEFLAGS="-j[0-9]*"$' "$file"; then
        sed -i "s/^MAKEFLAGS=\"-j[0-9]*\"\$/MAKEFLAGS=\"-j${cores}\"/" "$file"
        CHANGED=1
        report MAKEFLAGS "set to -j${cores}"
    elif grep -q '^MAKEFLAGS=' "$file"; then
        report MAKEFLAGS "kept your own ($(sed -n 's/^MAKEFLAGS=//p' "$file" | head -n 1))"
    elif grep -q '^#MAKEFLAGS=' "$file"; then
        sed -i "0,/^#MAKEFLAGS=.*/s//MAKEFLAGS=\"-j${cores}\"/" "$file"
        CHANGED=1
        report MAKEFLAGS "set to -j${cores}"
    else
        printf 'MAKEFLAGS="-j%s"\n' "$cores" >> "$file"
        CHANGED=1
        report MAKEFLAGS "added (-j${cores})"
    fi
}

# Replace a file with its edited copy, after a backup, if a step changed it
# (CHANGED; cmp is not on every minimal system).
applyChanges() {
    conf=$1
    copy=$2
    if [ -z "$CHANGED" ]; then
        printf '%s is unchanged.\n\n' "$conf"
        return
    fi
    backup="$conf.helixforge-backup-$(date +%Y%m%d%H%M%S)"
    "$ESCALATION_TOOL" cp -a "$conf" "$backup"
    "$ESCALATION_TOOL" tee "$conf" < "$copy" > /dev/null
    printf 'Updated %s (backup: %s).\n\n' "$conf" "$backup"
}

checkEnv
for conf in "$PACMAN_CONF" "$MAKEPKG_CONF"; do
    if [ ! -f "$conf" ]; then
        printf "%b\n" "${RED}${conf} not found.${RC}"
        exit 1
    fi
done
work=$(mktemp -d)
trap 'rm -rf "$work"' 0

CHANGED=
cp "$PACMAN_CONF" "$work/pacman.conf"
printf '%s\n' "$PACMAN_CONF:"
enableOption Color "$work/pacman.conf"
enableCandy "$work/pacman.conf"
enableOption VerbosePkgLists "$work/pacman.conf"
enableParallelDownloads "$work/pacman.conf"
enableMultilib "$work/pacman.conf"
applyChanges "$PACMAN_CONF" "$work/pacman.conf"

CHANGED=
cp "$MAKEPKG_CONF" "$work/makepkg.conf"
printf '%s\n' "$MAKEPKG_CONF:"
setMakeflags "$work/makepkg.conf"
applyChanges "$MAKEPKG_CONF" "$work/makepkg.conf"

printf "%b\n" "${GREEN}Done. The pacman settings show the next time pacman runs: colour, the Pac-Man progress bar and package lists in columns.${RC}"
