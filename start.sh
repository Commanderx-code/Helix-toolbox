#!/bin/sh -e

# Run HelixForge: from source when this is a checkout with Cargo, otherwise
# the latest published Linux release for this machine (x86_64 or aarch64),
# checked against its published SHA-256 sum. Prevent execution if only
# partially downloaded.
{
rc='\033[0m'
red='\033[0;31m'

check() {
    exit_code=$1
    message=$2
    if [ "$exit_code" -ne 0 ]; then
        printf '%sERROR: %s%s\n' "$red" "$message" "$rc"
        exit 1
    fi
    unset exit_code
    unset message
}

SCRIPT_DIR=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
if [ -f "$SCRIPT_DIR/Cargo.toml" ] && command -v cargo >/dev/null 2>&1; then
    cd "$SCRIPT_DIR"
    check $? "Entering the HelixForge directory"
    cargo run --locked -p helixforge -- "$@"
    check $? "Running HelixForge from source"
    exit 0
fi

case "$(uname -m)" in
    x86_64|amd64) asset=helixforge-linux-x86_64 ;;
    aarch64|arm64) asset=helixforge-linux-aarch64 ;;
    *) check 1 "Releases are built for Linux x86_64 and aarch64 only; build from a checkout with cargo on this machine" ;;
esac

base=${HELIXFORGE_RELEASE:-https://github.com/Commanderx-code/Helix-Forge/releases/latest/download}
work=$(mktemp -d)
check $? "Creating the temporary directory"
trap 'rm -rf "$work"' 0

curl --fail --silent --location --proto '=https' --tlsv1.2 -o "$work/SHA256SUMS" "$base/SHA256SUMS"
check $? "Downloading the HelixForge checksums"
curl --fail --silent --location --proto '=https' --tlsv1.2 -o "$work/$asset" "$base/$asset"
check $? "Downloading HelixForge"

expected=$(grep -E "^[0-9a-f]{64} [ *]?$asset\$" "$work/SHA256SUMS" | cut -c1-64)
if [ -z "$expected" ]; then
    check 1 "Finding $asset in the published checksums"
fi
actual=$(sha256sum -- "$work/$asset" | cut -c1-64)
if [ "$actual" != "$expected" ]; then
    check 1 "The downloaded HelixForge does not match its published checksum"
fi

chmod +x "$work/$asset"
check $? "Making HelixForge executable"

"$work/$asset" "$@"
check $? "Executing HelixForge"
} # End of wrapping
