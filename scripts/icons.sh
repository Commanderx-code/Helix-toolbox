#!/bin/sh -e

# Render the logo, assets/icon.svg, into every picture made from it:
# assets/icon.png and assets/logo.png at 512 pixels, the sizes in
# assets/icons/ for packaging/nfpm.yaml, the window's src-tauri/icons/ for
# its tauri.conf.json, and the terminal interface's header emblem. Needs
# rsvg-convert (librsvg) and magick (ImageMagick 7). Run from the
# repository root after changing the logo, and commit what it writes.

cd "$(dirname "$0")/.."

logo=assets/icon.svg
out=assets/icons
window=apps/helix-toolbox-gui/src-tauri/icons

render() {
    rsvg-convert --width "$1" --height "$1" "$logo" | magick - -strip "PNG32:$2"
}

render 512 assets/icon.png
render 512 assets/logo.png
mkdir -p "$out"
for size in 16 22 24 32 48 64 128 256; do
    render "$size" "$out/${size}x${size}.png"
done

# The window bundles its own copies. Tauri installs each in the folder of
# its size, and a name ending in @2x in a high-density one (256x256@2)
# that desktops do not look in, so the 256-pixel one is named for its size.
for size in 16 24 32 48 64 128 256; do
    cp "$out/${size}x${size}.png" "$window/${size}x${size}.png"
done
cp assets/icon.png "$window/icon.png"

# Four rows high in the terminal's header; 160 pixels keeps it sharp there.
render 160 apps/helix-toolbox/src/tui/emblem.png

ls -l assets/icon.png assets/logo.png "$out" "$window"
