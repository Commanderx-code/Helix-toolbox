#!/bin/sh -e

# Move PKGBUILD and .SRCINFO to a published release: its version, and the
# SHA-256 sums of its Arch packages from the release's SHA256SUMS.
# usage: packaging/aur/update.sh VERSION
# .SRCINFO is written with makepkg, so run it on Arch or with makepkg
# installed.

version=$1
case "$version" in
    *[!0-9.]* | '' | .* | *.)
        echo "usage: packaging/aur/update.sh VERSION (a release version, such as 1.25.18)" >&2
        exit 2
        ;;
esac
cd "$(dirname -- "$0")"

sums=$(curl -fsSL "https://github.com/Commanderx-code/Helix-toolbox/releases/download/v$version/SHA256SUMS")
sum() {
    printf '%s\n' "$sums" | awk -v file="$1" '$2 == file { print $1 }' | grep . ||
        { echo "no $1 in the v$version SHA256SUMS" >&2; exit 1; }
}
cli_x86_64=$(sum "helix-toolbox-$version-1-x86_64.pkg.tar.zst")
gui_x86_64=$(sum "helix-toolbox-gui-$version-1-x86_64.pkg.tar.zst")
cli_aarch64=$(sum "helix-toolbox-$version-1-aarch64.pkg.tar.zst")
gui_aarch64=$(sum "helix-toolbox-gui-$version-1-aarch64.pkg.tar.zst")

awk -v version="$version" \
    -v cx="$cli_x86_64" -v gx="$gui_x86_64" -v ca="$cli_aarch64" -v ga="$gui_aarch64" '
    /^pkgver=/ { print "pkgver=" version; next }
    /^pkgrel=/ { print "pkgrel=1"; next }
    /^sha256sums_x86_64=/ { print "sha256sums_x86_64=(\x27" cx "\x27"; getline; print "                   \x27" gx "\x27)"; next }
    /^sha256sums_aarch64=/ { print "sha256sums_aarch64=(\x27" ca "\x27"; getline; print "                    \x27" ga "\x27)"; next }
    { print }
' PKGBUILD > PKGBUILD.new
mv PKGBUILD.new PKGBUILD

if command -v makepkg >/dev/null 2>&1; then
    makepkg --printsrcinfo > .SRCINFO.new
    mv .SRCINFO.new .SRCINFO
else
    echo "makepkg is not installed: .SRCINFO is unchanged; run makepkg --printsrcinfo > .SRCINFO on Arch" >&2
fi
git diff --stat -- PKGBUILD .SRCINFO 2>/dev/null || true
