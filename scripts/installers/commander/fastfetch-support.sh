#!/bin/sh -e

# Source after common.sh has initialized the package manager.
install_fastfetch_support() {
    case "$PACKAGER" in
        pacman | apt-get | nala) set -- imagemagick ;;
        dnf) set -- ImageMagick ImageMagick-libs ;;
        *) printf 'Unsupported image-support package manager: %s\n' "$PACKAGER" >&2; return 1 ;;
    esac
    # Resolve past any previously installed Commander launcher.
    if ! python3 ./fastfetch-distro.py --version >/dev/null 2>&1; then
        set -- fastfetch "$@"
    fi
    install_packages "$@"

    features=$(python3 ./fastfetch-distro.py --list-features) || return 1
    if printf '%s\n' "$features" | grep -Eq '^imagemagick[67]$'; then
        printf '%s\n' 'ImageMagick installed; Fastfetch includes the Kitty/Sixel image backend.'
    else
        printf '%s\n' 'ImageMagick installed, but this Fastfetch build lacks its image backend.' 'Direct PNG protocols (Kitty-direct and iTerm) remain available. For Sixel or encoded Kitty, use a Fastfetch build with ImageMagick support.'
    fi
    printf '%s\n' 'Your terminal must support images: for example Konsole, Kitty, Ghostty, WezTerm or Foot.'
}
