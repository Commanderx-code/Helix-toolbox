#!/bin/sh -e

# Render the icon sizes the packages ship, from the two sources in assets/:
# icon-small.svg for the menu sizes (16 to 48 pixels) and icon.png for the
# rest. Writes assets/icons/ for packaging/nfpm.yaml and the window's
# src-tauri/icons/ for its tauri.conf.json. Needs rsvg-convert (librsvg)
# and magick (ImageMagick 7). Run from the repository root after changing
# either source, and commit what it writes.

cd "$(dirname "$0")/.."

small=assets/icon-small.svg
large=assets/icon.png
out=assets/icons
window=apps/helix-toolbox-gui/src-tauri/icons

mkdir -p "$out"
for size in 16 22 24 32 48; do
    rsvg-convert --width "$size" --height "$size" "$small" --output "$out/${size}x${size}.png"
done
for size in 64 128 256; do
    magick "$large" -filter Lanczos -resize "${size}x${size}" "$out/${size}x${size}.png"
done

# The window bundles its own copies; Tauri reads each file's size from its
# name, and 128x128@2x is the 256-pixel one.
for size in 16 24 32 48 64 128; do
    cp "$out/${size}x${size}.png" "$window/${size}x${size}.png"
done
cp "$out/256x256.png" "$window/128x128@2x.png"
cp "$large" "$window/icon.png"

ls -l "$out" "$window"
