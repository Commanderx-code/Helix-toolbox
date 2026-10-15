# Changelog

Each release is tagged `vMAJOR.MINOR.PATCH` on GitHub, with the program, its
packages and their checksums. A release whose workflow only adopted updated
dotfiles or Myfish sources raises the patch number and is not listed here.

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
  ([BETA.md](docs/BETA.md)), issue forms and this changelog.
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
