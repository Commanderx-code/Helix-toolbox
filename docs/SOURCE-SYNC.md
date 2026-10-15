# Automatic source updates

HelixForge watches **Commanderx-code/dotfiles** and **Commanderx-code/Myfish**.
The [Sync sources and release workflow](https://github.com/Commanderx-code/Helix-Forge/actions/workflows/sync-sources.yml)
runs at minutes 17 and 47 of each hour, on pushes to `main`, and through
**Run workflow**.

For each source it reads the current `main` commit, then checks that commit's
`check.yml` push or dispatched workflow. Only a completed successful run is
accepted. A failed, cancelled, missing or unfinished run leaves that source's
previous pin in place; an API error fails the sync instead of selecting an
unchecked version.

The workflow imports each accepted `toolbox.json`, renders its entries as
catalog files (`catalog/configurations/dotfiles/`, `catalog/configurations/myfish/`
and the terminal tools under `catalog/applications/terminal-tools/`), runs
every check, builds HelixForge and regenerates the walkthrough. Only after
those steps succeed does it commit and push the new pins and catalog. It then
publishes a release named `helixforge-<commit>` with the Linux x86_64 build
and its SHA-256 sum; `start.sh` and the dotfiles launcher fetch the latest.

## What updates automatically

- Changes to supported dotfiles configuration and Myfish's installer are
  included when their tested commit becomes the new source pin, recorded in
  `scripts/installers/commander/common.sh`.
- New `config` entries in a source's `toolbox.json` become catalog entries
  with generated wrappers around `catalog-install.sh`. Removing one removes
  it from future builds without touching an existing user's files.
- Entries rendered by the sync carry a first-line note saying so; edit the
  source repository's `toolbox.json`, not the rendered file.

Each rendered entry declares what it writes under the home directory. The
engine refuses to run an entry whose target is a Home Manager symlink (a link
into the Nix store) and says so in the plan, so a workstation that Home
Manager manages keeps its ownership.

## Local checks

```sh
python3 -B scripts/sync-sources.py --render   # offline, from the embedded catalogs
python3 -B -m unittest discover -s tests/installers
```

Running `python3 -B scripts/sync-sources.py` without `--render` queries
GitHub and updates the pins and catalog in your checkout. It never commits,
pushes, publishes or installs anything by itself.
