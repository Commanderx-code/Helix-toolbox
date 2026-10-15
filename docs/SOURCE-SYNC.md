# Automatic source updates

HelixForge watches **Commanderx-code/dotfiles** and **Commanderx-code/Myfish**.
The [Sync sources and release workflow](https://github.com/Commanderx-code/Helix-Forge/actions/workflows/sync-sources.yml)
runs when dotfiles or Myfish pass their checks on `main`, on pushes to
HelixForge's `main`, through **Run workflow**, and on a schedule at minutes 17
and 47 of each hour. The schedule is only a backstop: GitHub fires scheduled
workflows on these repositories every few hours at best, so the sources
notify HelixForge themselves.

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

## Notifications from the sources

Each source repository has a **Notify HelixForge** workflow. It runs when that
repository's `check.yml` finishes successfully for a push to `main` and starts
this workflow with the reason `source-updated`. Such a run behaves like a
scheduled one: it adopts the new revision, and builds and publishes only when
something changed. A notification carries no data that HelixForge trusts; the
sync still checks the source's latest `main` commit and its check run itself.

Starting a workflow in another repository needs a token. Create it once:

1. On GitHub, open **Settings > Developer settings > Fine-grained personal
   access tokens > Generate new token**.
2. Name it `HelixForge sync`, choose an expiry, set **Repository access** to
   **Only select repositories > Commanderx-code/Helix-Forge**, and under
   **Repository permissions** set **Actions** to **Read and write**. Nothing
   else; the token can start and view workflows but cannot change code.
3. Store it in both sources:

   ```sh
   gh secret set HELIXFORGE_DISPATCH_TOKEN --repo Commanderx-code/dotfiles
   gh secret set HELIXFORGE_DISPATCH_TOKEN --repo Commanderx-code/Myfish
   ```

Without the secret, the notifier succeeds with a notice and HelixForge picks
the source up on its next push or scheduled run. When the token expires, the
notifier fails visibly in the source repository; create a new one and set it
again.

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
