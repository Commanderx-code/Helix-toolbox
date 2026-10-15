#!/usr/bin/env bash
# Every check the GitHub workflow runs.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p helixforge
target/release/helixforge docgen docs/walkthrough.md
git diff --exit-code -- docs/walkthrough.md
echo "HelixForge checks passed."
