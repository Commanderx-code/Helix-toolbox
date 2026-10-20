#!/bin/sh -e

# Make the Arch package of the window from the binary Tauri built.
# usage: scripts/package-gui.sh VERSION BINARY OUTDIR
# Needs nfpm on the PATH. Tauri makes the Debian package, the RPM and the
# AppImage itself and has no Arch bundler, so nfpm packages the same binary
# with the desktop entry and the icons (packaging/nfpm-gui.yaml). The
# package is for this machine's architecture.

version=$1
binary=$2
out=$3
if [ -z "$version" ] || [ -z "$binary" ] || [ -z "$out" ]; then
    echo "usage: scripts/package-gui.sh VERSION BINARY OUTDIR" >&2
    exit 2
fi
case "$version" in
    *[!0-9.]* | '' | .* | *.) echo "not a release version: $version" >&2; exit 2 ;;
esac

case "$(uname -m)" in
    x86_64) arch=amd64 ;;
    aarch64) arch=arm64 ;;
    *) echo "no packages for $(uname -m)" >&2; exit 2 ;;
esac

out=$(mkdir -p -- "$out" && cd -- "$out" && pwd)
binary=$(cd -- "$(dirname -- "$binary")" && pwd)/$(basename -- "$binary")
cd "$(dirname -- "$0")/.."
# packaging/nfpm-gui.yaml reads the staged binary from here.
stage=target/package-gui-stage
rm -rf "$stage"
mkdir -p "$stage"
cp -- "$binary" "$stage/helix-toolbox-gui"
chmod 0755 "$stage/helix-toolbox-gui"

HELIX_TOOLBOX_VERSION=$version HELIX_TOOLBOX_ARCH=$arch \
    nfpm package --config packaging/nfpm-gui.yaml --packager archlinux --target "$out"
ls -1 "$out"
