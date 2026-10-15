#!/bin/sh -e
# Sourced by resolve.sh after environment setup and installation confirmation.
resolve_apt_nvidia() {
    if [ ! -e /etc/apt/sources.list.d/nvidia-container-toolkit.list ] &&
        [ ! -e /etc/apt/sources.list.d/commander-nvidia-container-toolkit.list ]; then
        confirm 'Add the official NVIDIA Container Toolkit APT repository with a dedicated signing key?'
        install_packages ca-certificates curl gnupg
        repo_work=$(mktemp -d)
        trap 'rm -rf -- "$repo_work"' 0
        curl -fLsS https://nvidia.github.io/libnvidia-container/gpgkey -o "$repo_work/key.asc"
        # Pin NVIDIA's published signing key instead of trusting whatever the URL serves.
        mkdir -m 700 "$repo_work/gnupg"
        key_fprs=$(GNUPGHOME="$repo_work/gnupg" gpg --batch --quiet --show-keys --with-colons "$repo_work/key.asc" |
            awk -F: '$1 == "pub" { want = 1; next } want && $1 == "fpr" { print $10; want = 0 }')
        if [ "$key_fprs" != C95B321B61E88C1809C4F759DDCAE044F796ECB0 ]; then
            printf '%s\n' 'NVIDIA signing key fingerprint mismatch; the repository was not added.' >&2
            exit 1
        fi
        GNUPGHOME="$repo_work/gnupg" gpg --batch --quiet --dearmor --output "$repo_work/key.gpg" "$repo_work/key.asc"
        # Write the source line locally so downloaded content cannot add other repositories or options.
        # shellcheck disable=SC2016 # $(ARCH) is expanded by apt, not the shell.
        printf '%s\n' 'deb [signed-by=/usr/share/keyrings/commander-nvidia-container-toolkit.gpg] https://nvidia.github.io/libnvidia-container/stable/deb/$(ARCH) /' \
            > "$repo_work/signed.list"
        backup_system /usr/share/keyrings/commander-nvidia-container-toolkit.gpg
        "$ESCALATION_TOOL" install -m 644 "$repo_work/key.gpg" /usr/share/keyrings/commander-nvidia-container-toolkit.gpg
        "$ESCALATION_TOOL" install -m 644 "$repo_work/signed.list" /etc/apt/sources.list.d/commander-nvidia-container-toolkit.list
        rm -rf -- "$repo_work"
        trap - 0
    fi
    "$ESCALATION_TOOL" apt-get update
}
