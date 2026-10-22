#!/bin/sh -e

# Check that every package the catalog names for this family exists in this
# distribution's repositories, except the gaps tests/package-gaps.txt lists
# for IMAGE, and that each listed gap is still one. Run in the container,
# after its package database is fresh.
# usage: scripts/check-package-names.sh IMAGE

image=${1:?usage: scripts/check-package-names.sh IMAGE}
here=$(cd "$(dirname "$0")" && pwd)
gaps=$(sed -n "s|^$image ||p" "$here/../tests/package-gaps.txt")

case "$image" in
    archlinux:*) key=arch ;;
    debian:* | ubuntu:*) key=debian ;;
    fedora:*) key=fedora ;;
    # The script entries' [detect] names too, as this check always had.
    opensuse/*) key="suse --detect" ;;
    *) echo "no package family for $image" >&2; exit 2 ;;
esac

exists() {
    case "$image" in
        archlinux:*) pacman -Si "$1" > /dev/null 2>&1 ;;
        debian:* | ubuntu:*) apt-cache show --no-all-versions "$1" 2> /dev/null | grep -q '^Package:' ;;
        fedora:*) dnf info -q "$1" > /dev/null 2>&1 ;;
        opensuse/*) zypper --non-interactive --quiet search --match-exact --type package "$1" > /dev/null 2>&1 ;;
    esac
}

failed=0
# shellcheck disable=SC2086
for name in $(python3 -B "$here/package-names.py" $key); do
    gap=false
    for listed in $gaps; do
        [ "$listed" = "$name" ] && gap=true
    done
    if exists "$name"; then
        if $gap; then
            echo "$image has $name now; take it out of tests/package-gaps.txt" >&2
            failed=1
        fi
    elif ! $gap; then
        echo "$image has no package $name" >&2
        failed=1
    fi
done
exit "$failed"
