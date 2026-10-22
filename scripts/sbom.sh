#!/bin/sh -e

# Software bills of materials, CycloneDX JSON, for what a release ships:
# the command line (every crate in it), the window's Rust side and the
# window's frontend, into DIRECTORY. Needs cargo-cyclonedx and npm; reads the
# lock files, so it describes exactly what was built from them.

out=${1:?usage: scripts/sbom.sh DIRECTORY}
cd "$(dirname "$0")/.."
mkdir -p "$out"
out=$(cd "$out" && pwd)

cyclonedx() {
    cargo cyclonedx --manifest-path "$1" --format json --spec-version 1.5 --no-build-deps \
        --override-filename helix-toolbox-sbom
}

cyclonedx apps/helix-toolbox/Cargo.toml
mv apps/helix-toolbox/helix-toolbox-sbom.json "$out/helix-toolbox-sbom.cdx.json"
# It writes one beside each crate of the workspace; the program's holds them all.
find crates -name helix-toolbox-sbom.json -delete

cyclonedx apps/helix-toolbox-gui/src-tauri/Cargo.toml
mv apps/helix-toolbox-gui/src-tauri/helix-toolbox-sbom.json "$out/helix-toolbox-gui-sbom.cdx.json"

(cd apps/helix-toolbox-gui && npm sbom --package-lock-only --omit dev --sbom-format cyclonedx) \
    > "$out/helix-toolbox-gui-frontend-sbom.cdx.json"

for file in "$out"/*-sbom.cdx.json; do
    python3 -c 'import json, sys
bom = json.load(open(sys.argv[1]))
assert bom["bomFormat"] == "CycloneDX", sys.argv[1]
print(sys.argv[1] + ": CycloneDX " + bom["specVersion"] + ", " + str(len(bom.get("components", []))) + " components")' "$file"
done
