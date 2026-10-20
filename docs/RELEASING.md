# Releasing

Releases are made by the **Sync sources and release** workflow
(`.github/workflows/sync-sources.yml`) on every push to `main`, and when the
dotfiles or Myfish repositories report new sources. Nobody tags by hand.

1. `scripts/release-version.py` picks the version: the next patch of the
   `[workspace.package] version` series in `Cargo.toml`, or that version
   itself for the first release of a new series. Raising it in `Cargo.toml`
   (as the last pull request of a milestone does) starts a new series.
2. It waits for the Distributions workflow on the same commit and stops if
   that failed; its record of what each container verified
   (`scripts/validation-record.py`) is built into the program for the
   Reliability screen and published as `validation.json`. A source update,
   which has no Distributions run of its own, carries its ancestor's.
3. The workflow runs every check `scripts/verify.sh` runs and builds the
   release program with the version stamped in. A second job, on GitHub's
   ARM64 runner, checks out the same commit with the same record, runs the
   tests and builds the aarch64 program.
4. `scripts/package.sh` makes the Arch, Debian and RPM packages with nfpm,
   for the architecture it runs on. `SHA256SUMS` covers both architectures'
   files, and each gets a build provenance attestation. A third job builds
   the window's Debian package, RPM and AppImage on each architecture with
   `tauri build`, after `scripts/stamp-version.py` on its own `Cargo.toml`.
5. The GitHub release `vX.Y.Z` is created as a draft, the files are
   uploaded, and only then is it published.
6. With a crates.io token set up (below), `scripts/stamp-version.py` writes
   the version into the checkout's `Cargo.toml` and the five crates are
   published to crates.io in dependency order: `helix-toolbox-core`,
   `helix-toolbox-platform`, `helix-toolbox-catalog`, `helix-toolbox-engine`, then
   `helix-toolbox`. Nothing of this is committed.

## The Arch repository

The `repository` job of the release workflow, after the release is
published, builds a pacman repository from the release's Arch packages
(`repo-add`, in an Arch container) and deploys it to GitHub Pages at
`https://commanderx-code.github.io/Helix-toolbox/{x86_64,aarch64}`, with an
index page that shows the `pacman.conf` lines. Each release replaces the
whole site, so the repository has only the latest version. It is not
signed: users list it with `SigLevel = Optional TrustAll`, and the packages
are the release's own files, covered by its checksums and attestations.

Pages was turned on once with the build type "GitHub Actions"
(`gh api -X POST repos/OWNER/REPO/pages -f build_type=workflow`); the job
needs nothing else, no secret and no branch.

## crates.io

The library crates exist only so `helix-toolbox` can be published; their
descriptions say they are not general-purpose libraries. The catalog crate
reaches the repository's `catalog/` and `scripts/` through the links
`crates/helix-toolbox-catalog/catalog` and `scripts`, which `cargo package`
follows. The Checks workflow runs `cargo package --workspace` on every
change, which builds each crate from its packaged form.

To turn publishing on, once:

1. Sign in to [crates.io](https://crates.io) with the GitHub account and
   verify the email address in the account settings.
2. Create an API token with the **publish-new** and **publish-update**
   scopes (Account Settings → API Tokens).
3. Add it to the repository as the Actions secret `CARGO_REGISTRY_TOKEN`
   (`gh secret set CARGO_REGISTRY_TOKEN`).

The next release publishes all five crates. Without the secret the step is
skipped and nothing else changes. A version on crates.io cannot be deleted,
only yanked (`cargo yank --version X.Y.Z helix-toolbox`), which stops new
installs from choosing it.
