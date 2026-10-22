#!/bin/sh -e

# Install PACKAGES on a GitHub Ubuntu runner, giving up on a stalled or
# trickling mirror after a few minutes instead of waiting out the job.
# usage: scripts/ci-apt-install.sh PACKAGE...

[ "$#" -gt 0 ] || { echo "usage: scripts/ci-apt-install.sh PACKAGE..." >&2; exit 2; }

options='-o Acquire::Retries=3 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30'
attempt=1
while :; do
    # Downloads already finished stay in apt's cache for the next attempt.
    # shellcheck disable=SC2086
    if timeout 4m sudo apt-get $options update &&
        timeout 6m sudo apt-get $options install -y "$@"; then
        exit 0
    fi
    [ "$attempt" -lt 3 ] || { echo "apt-get failed $attempt times" >&2; exit 1; }
    attempt=$((attempt + 1))
    # An install stopped part way leaves packages to finish setting up.
    sudo dpkg --configure -a || true
    # The runner's own mirror is the usual culprit; the next attempts use
    # Ubuntu's main archive.
    echo "apt-get failed or stalled; trying again (attempt $attempt) from archive.ubuntu.com" >&2
    sudo find /etc/apt/sources.list /etc/apt/sources.list.d -type f \
        \( -name '*.list' -o -name '*.sources' \) \
        -exec sed -i 's|http://azure\.archive\.ubuntu\.com/|http://archive.ubuntu.com/|g' {} +
done
