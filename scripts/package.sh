#!/bin/sh -e

# Make the Arch, Debian and RPM packages of a release build.
# usage: scripts/package.sh VERSION BINARY OUTDIR
# Needs nfpm on the PATH. The binary writes its own shell completions and
# manual pages, so they always match its commands; it runs here, so the
# packages are for this machine's architecture.

version=$1
binary=$2
out=$3
if [ -z "$version" ] || [ -z "$binary" ] || [ -z "$out" ]; then
    echo "usage: scripts/package.sh VERSION BINARY OUTDIR" >&2
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
# packaging/nfpm.yaml reads the staged files from here.
stage=target/package-stage
rm -rf "$stage"
mkdir -p "$stage"

cp -- "$binary" "$stage/helixforge"
chmod 0755 "$stage/helixforge"
mkdir -p "$stage/completions" "$stage/man"
"$stage/helixforge" completions bash > "$stage/completions/helixforge.bash"
"$stage/helixforge" completions zsh > "$stage/completions/_helixforge"
"$stage/helixforge" completions fish > "$stage/completions/helixforge.fish"
"$stage/helixforge" manpages "$stage/man"
gzip -9n "$stage"/man/*.1

for packager in archlinux deb rpm; do
    HELIXFORGE_VERSION=$version HELIXFORGE_ARCH=$arch \
        nfpm package --config packaging/nfpm.yaml --packager "$packager" --target "$out"
done
ls -1 "$out"
