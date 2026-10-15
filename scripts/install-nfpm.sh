#!/bin/sh -e

# Put a pinned nfpm, checked against its published SHA-256 sum, in DIR.
# usage: scripts/install-nfpm.sh DIR

version=2.47.0
sum=0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783
directory=${1:?usage: scripts/install-nfpm.sh DIR}

work=$(mktemp -d)
trap 'rm -rf "$work"' 0
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    -o "$work/nfpm.tar.gz" \
    "https://github.com/goreleaser/nfpm/releases/download/v$version/nfpm_${version}_Linux_x86_64.tar.gz"
echo "$sum  $work/nfpm.tar.gz" | sha256sum --check --quiet -
tar -xzf "$work/nfpm.tar.gz" -C "$work" nfpm
mkdir -p "$directory"
install -m 0755 "$work/nfpm" "$directory/nfpm"
"$directory/nfpm" --version | grep -q "$version"
