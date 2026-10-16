# Changelog

Each release is tagged `vMAJOR.MINOR.PATCH` on GitHub, with the program, its
packages and their checksums. A release whose workflow only adopted updated
dotfiles or Myfish sources raises the patch number and is not listed here.

## v1.2.0

- **Reliability**: a new screen and `helixforge reliability` show how far
  each entry has been tested: Container Tested (installed and verified in the
  Distributions workflow's containers), Stand-in Tested (its installer run
  against stand-in commands), VM Tested or Hardware Tested (from reports), or
  Not Yet Validated, with the systems, the date it was last verified and its
  last run on this machine. Boot, login-screen, driver and whole-system
  entries come first, and running one that has no more than stand-in tests
  says so in its confirmation.
- **Trust inspector**: `v` on any entry, or `helixforge trust <id>`, shows
  the installer's origin, file, checksum and licence, the software's
  licence, and keeps release provenance, package integrity checks (pacman's
  SigLevel, apt, dnf's gpgcheck, Flathub, the unreviewed AUR) and the
  installer's own downloads (pinned, checked, or unverified) apart, with the
  privileges it uses.
- Releases wait for the Distributions workflow on their commit and are not
  published when it fails; each carries the record of what the containers
  verified, also published as `validation.json`. The containers now also
  install Neovim and btop.

## v1.1.0

- **HELIX Drift**: `helixforge drift save <name>` (or `s` on the new Drift
  screen) records a baseline of the machine: what is installed and from
  where, the configuration files entries write (by checksum), the login
  shell, display manager, repositories and Flatpak remotes, and what
  HelixForge ran. Drift then lists what is missing, changed, replaced or new
  compared with it, or with a profile, and queues the entries that put the
  missing parts back for review. Baselines export and import, so a fresh
  installation can be checked against the old machine.
- **Command palette**: `Ctrl-K` finds any screen, action (check updates
  online, DNS diagnostics, save a baseline, change the theme) or entry by
  name, from anywhere in the menu.
- The sidebar drops its spacing, then scrolls, when the terminal is too
  short for every screen, and the hint line leaves out what does not fit.

## v1.0.4 to v1.0.6

- The Favourites screen is now spelled **Favorites**; pins stay in
  `favourites.toml`, so nothing needs moving.
- The Updates screen has a **HelixForge** row that updates HelixForge
  itself, the way it was installed: a package is downloaded, checked against
  the release's SHA256SUMS and installed with the package manager; a Cargo
  install runs `cargo install` again; a checkout is fast-forwarded and
  rebuilt. The launcher and `start.sh` already run the latest release.
- A new README with screenshots, an install table and a documentation index;
  the detail it held moved to docs/REFERENCE.md, docs/CATALOG.md and
  CONTRIBUTING.md. A code of conduct, a pull request template and a security
  policy for private vulnerability reports.

## v1.0.1 to v1.0.3

- v1.0.3: HelixForge is on [crates.io](https://crates.io/crates/helixforge):
  `cargo install --locked helixforge` builds it on any machine with Rust,
  and every release publishes the crates at its own version
  ([RELEASING.md](docs/RELEASING.md)).
- v1.0.2: `cargo install --git` works ([INSTALL.md](docs/INSTALL.md)), and
  the crates are ready for crates.io.
- v1.0.1: Helix Doctor treats a failed service whose unit file was removed
  (its package uninstalled) as a notice, "removed, but its last failure is
  still remembered", with `systemctl reset-failed` as the repair instead of a
  restart that cannot work.

## v1.0.0

The first stable release. What stays the same across 1.x is written down in
[STABILITY.md](docs/STABILITY.md).

- Tested on openSUSE Tumbleweed and Ubuntu 22.04 as well as Arch, Debian,
  Ubuntu 24.04 and Fedora, on every change; HelixForge's own packages are
  installed, used and removed on each.
- 37 more entries know their openSUSE package (first in v0.7.1), each checked
  against the openSUSE repositories.
- A **monochrome** theme, with no colour, used for the run when `NO_COLOR`
  is set.
- Settings are read one by one: a setting or value from a later version is
  ignored instead of resetting the others.
- Damaged settings, favourites, profiles and history are skipped or replaced
  by defaults, and never stop the menu or the commands.
- The release workflows pin every action to a commit.
- docs/BETA.md became docs/REPORTING.md.

## v0.7.0

Beta release: packaging, documentation and testing.

- Packages for Arch, Debian/Ubuntu and Fedora/openSUSE with every release,
  carrying shell completions, manual pages, a menu entry and icon, each with
  a SHA-256 sum and a signed build provenance attestation
  ([INSTALL.md](docs/INSTALL.md)). First published with v0.6.1.
- `helixforge completions bash|zsh|fish`.
- `helixforge report` prints what a bug report needs, for pasting into an
  issue, with `--json`.
- A user guide ([GUIDE.md](docs/GUIDE.md)), a beta testing guide
  ([REPORTING.md](docs/REPORTING.md)), issue forms and this changelog.
- Fixed: unpacking the built-in catalog could fail on some file systems,
  depending on the order the build machine listed the scripts folder.

## v0.6.0

- `--json` on the commands that read and on every `--dry-run` plan, in
  versioned shapes ([JSON.md](docs/JSON.md)); refused for anything that
  changes the machine.
- Command-line output piped into `head` ends quietly.
- v0.5.1: the **Updates** screen and `helixforge updates`: what pacman, apt,
  dnf or zypper, the AUR helper, Flatpak, Home Manager, NixOS and a Nix
  profile have waiting, checked offline or online; updates delegated to
  those tools after confirmation.

## v0.5.0

- Ranked search with loose spellings, and `helixforge search`.
- `i` filters a list to installed entries or ones with an update.
- `*` pins favourites; the **Favourites** screen lists them and recently run
  entries.
- Clearing a search returns to the row you were on.
- v0.4.1: your own profiles: `s` saves the Install Plan as a named profile;
  `helixforge profile list|export|import|delete`.

## v0.4.0

- **Helix Doctor 2.0**: checks for the network, DNS, failed services,
  graphics drivers, the package database, home folder permissions, boot
  images, GRUB, the EFI partition and disk space, with evidence and repairs
  that run after their own confirmation; `helixforge doctor [--repair]`.
- v0.3.1: the **Backups** screen and `helixforge backups|restore`; a Snapper
  or Timeshift snapshot before system changes where the machine has one.

## v0.3.0

- The Install Plan as a queue: steps ordered by what they need, an option
  to keep going after a failure, each step's outcome shown, and unfinished
  entries kept to run again.
- v0.2.2: checks before a run: missing tools, repositories, graphics,
  free space, conflicts.

## v0.2.0

- Script entries say what they install, so their status shows too; commands
  installed some other way are traced to their package.
- v0.2.1: Pacman Config changes only what is off and says what it found.

## v0.1.x

- v0.1.3: `u` and `U` update entries; `helixforge update`.
- v0.1.2: status for every package entry: installed, update available,
  managed by Nix, or not installed, with source and version.
- v0.1.1: installed state from each entry's packages and the packages it
  conflicts with (WINE and WINE Staging).
- v0.1.0: numbered releases. Before them: the rebuild of Commander Toolbox
  in Rust with every entry carried across; tests in Arch, Debian, Ubuntu and
  Fedora containers; verified installs with fallback to the next source; the
  dashboard design, logo and banner; gaming entries (WINE and Proton,
  CachyOS's gaming sets, Itch, Minigalaxy, Rare, Faugus, Prism and the
  RuneScape launchers).
