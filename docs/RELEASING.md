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
   release program with the version stamped in.
4. `scripts/package.sh` makes the Arch, Debian and RPM packages with nfpm;
   `SHA256SUMS` covers them and the program, and each gets a build
   provenance attestation.
5. The GitHub release `vX.Y.Z` is created as a draft, the files are
   uploaded, and only then is it published.
6. With a crates.io token set up (below), `scripts/stamp-version.py` writes
   the version into the checkout's `Cargo.toml` and the five crates are
   published to crates.io in dependency order: `helixforge-core`,
   `helixforge-platform`, `helixforge-catalog`, `helixforge-engine`, then
   `helixforge`. Nothing of this is committed.

## crates.io

The library crates exist only so `helixforge` can be published; their
descriptions say they are not general-purpose libraries. The catalog crate
reaches the repository's `catalog/` and `scripts/` through the links
`crates/helixforge-catalog/catalog` and `scripts`, which `cargo package`
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
only yanked (`cargo yank --version X.Y.Z helixforge`), which stops new
installs from choosing it.
