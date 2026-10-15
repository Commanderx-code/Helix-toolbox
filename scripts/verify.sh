#!/usr/bin/env bash
# Every check the GitHub workflows run before a build is published.
set -euo pipefail
cd "$(dirname "$0")/.."
if command -v actionlint >/dev/null; then
    actionlint .github/workflows/*.yml
elif command -v nix >/dev/null; then
    nix run nixpkgs#actionlint -- .github/workflows/*.yml
else
    echo "actionlint is not installed; the Checks workflow lints the workflows." >&2
fi
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python3 -B -m unittest discover -s tests/installers
shellcheck -x -P . scripts/shared/*.sh scripts/installers/*.sh scripts/installers/commander/*.sh \
    tests/fixtures/scripts/shared/*.sh tests/fixtures/scripts/installers/*.sh start.sh
find scripts/installers -name '*.sh' -not -path '*/commander/*' -print0 | xargs -0 shellcheck -x -P . -S error
# Rendering the dotfiles and Myfish exports again must change nothing.
generated() {
    find catalog scripts/installers/commander/catalogs scripts/installers/commander/common.sh \
        -type f -print0 | sort -z | xargs -0 sha256sum
}
before=$(generated)
python3 -B scripts/sync-sources.py --render
if [ "$(generated)" != "$before" ]; then
    echo "The catalog differs from a fresh render of the dotfiles and Myfish exports." >&2
    git status --short -- catalog scripts/installers/commander >&2
    exit 1
fi
cargo build --release -p helixforge
target/release/helixforge --version
target/release/helixforge docgen docs/walkthrough.md
git diff --exit-code -- docs/walkthrough.md
echo "HelixForge checks passed."
