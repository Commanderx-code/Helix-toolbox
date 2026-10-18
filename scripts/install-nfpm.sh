#!/bin/sh -e

# Put a pinned nfpm, checked against its published SHA-256 sum, in DIR.
# usage: scripts/install-nfpm.sh DIR

version=2.47.0
directory=${1:?usage: scripts/install-nfpm.sh DIR}
# The build for this machine, from the release's checksums.txt.
case "$(uname -m)" in
    x86_64) build=x86_64 sum=0660ca602b2d2d2ae4781a06c692b3eeb9d437ffea05b831d76e41f4a3188783 ;;
    aarch64) build=arm64 sum=1c0f5f2999b9a974bfb04fdb0cc3306096de530ac5dbb25d739cc5f5219c919c ;;
    *) echo "no pinned nfpm for $(uname -m)" >&2; exit 1 ;;
esac

work=$(mktemp -d)
trap 'rm -rf "$work"' 0
curl --fail --silent --show-error --location --proto '=https' --tlsv1.2 \
    -o "$work/nfpm.tar.gz" \
    "https://github.com/goreleaser/nfpm/releases/download/v$version/nfpm_${version}_Linux_${build}.tar.gz"
echo "$sum  $work/nfpm.tar.gz" | sha256sum --check --quiet -
tar -xzf "$work/nfpm.tar.gz" -C "$work" nfpm
mkdir -p "$directory"
install -m 0755 "$work/nfpm" "$directory/nfpm"
"$directory/nfpm" --version | grep -q "$version"
