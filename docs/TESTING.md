# Testing

Tests never change the machine they run on. Fixtures live in
`tests/fixtures/`; anything a test runs writes only to temporary
directories, with `HOME`, `XDG_STATE_HOME` and `TMPDIR` pointed there. Real
package installs happen only in the Distributions workflow's containers.
`scripts/verify.sh` runs every local layer below; check its exit status.

## Layers

| Layer | Where | What it covers |
| :--- | :--- | :--- |
| Unit tests | each crate's `src/` | catalog validation, plans, the shell program, history, the pseudo-terminal session, the unpacked-copy lock and sweep |
| Screen tests | `apps/helixforge/src/tui/` | the interface drawn into a test backend: keys, screens, popups, the small-terminal warning |
| Engine end to end | `apps/helixforge/tests/end_to_end.rs` | fixture plans run through the mock runner and a real shell |
| Program in a terminal | `apps/helixforge/tests/terminal.rs` | the built `helixforge` driven through a pseudo-terminal, below |
| Installer tests | `tests/installers/` | the Commander installers against stand-in commands and temporary homes |
| Shellcheck | `scripts/verify.sh` | every shared helper and installer, with errors gated for the general catalog |
| Workflow lint | `scripts/verify.sh` | actionlint over every workflow, including the shell in each step |
| Source render | `scripts/verify.sh` | rendering the dotfiles and Myfish exports again changes no catalog file |
| Distributions | `.github/workflows/distributions.yml` | Arch, Debian, Ubuntu 22.04 and 24.04, Fedora and openSUSE Tumbleweed containers with real package installs and removals, each verified, and the real package manager failing over to a stand-in Flathub; HelixForge's own package for each installed, used and removed |

## The program in a terminal

`terminal.rs` starts the real binary in a pseudo-terminal with the hidden
`--catalog-dir tests/fixtures` option, reads the screen with a terminal
emulator and types keys. It checks:

- sidebar navigation, the help popup, search and clearing it
- pinning an entry, `favourites.toml`, and the entry under Favourites after a
  restart
- the Updates screen between the catalog areas and History
- completions for bash, zsh and fish, and the manual pages, naming the
  commands and leaving out hidden ones
- unreadable settings, favourites, profiles and history lines: skipped or
  replaced by defaults, with the menu and commands still working; settings
  from a later version (unknown names and values) keep the ones understood
- the monochrome theme drawing no colour anywhere, the selection in reverse
  video, and `NO_COLOR` not replacing the remembered theme
- `--json` from list, search, info, plan, a dry run, history, profile list
  and system parsing as JSON with schema 1, and a real run or `docgen`
  refused with it
- adding to the install plan, declining the confirmation (nothing runs) and
  accepting it
- typing an answer into an entry that asks a question
- `Ctrl-C` stopping an entry and its child process, recorded as interrupted
- a child that ignores the hangup: the window counts it, and a second
  `Ctrl-C` kills it
- a failing entry's exit status, the History screen and `history.jsonl`
- resizing while an entry runs, the too-small warning and recovery, and a
  key pressed together with a resize being read at once
- `Ctrl-C` in the confirmation leaving without running
- a killed HelixForge's unpacked copy being removed by the next start, and a
  clean exit removing its own
- refusing to open the menu without a terminal, and opening it when standard
  input is a pipe, as with `curl … | sh`
- the banner drawn when the terminal answers the graphics query as Kitty's
  graphics do, the text header when it does not, and the first key read in
  both cases (`tui/banner.rs` has unit tests for the answers of Kitty, Sixel,
  Konsole, WezTerm and plain terminals)
- the command line's `list`, `run --dry-run`, refusal to confirm without a
  terminal, `run --yes`, failure status and `history`
- a run whose history cannot be written: the command line and the menu both
  say so
- the menu or the command line killed during a run: the next history lists
  it once as not finished; killed after a run ended: its real result
- the command line's confirmation asked on the terminal when standard input
  is a pipe, and a wrong word cancelling
- `helixforge status` after real installs of htop and ripgrep: Installed,
  from the family's package manager, with a version, and `update htop
  --dry-run` saying it is up to date (Distributions)
- profiles: file names from names, saving, replacing, refusing unknown
  entries, empty profiles and unusable names, listing with a broken file,
  the "Your profiles" group planning like a built-in profile, and a queue
  flattened into one; the Install Plan's `s` saving one that then appears
  under Profiles; `helixforge profile import`, `list`, a dry run of
  `profile-<name>`, `export` and `delete` in a scratch configuration
- the health checks against made-up machines: healthy, offline, DNS failing
  (restarting NetworkManager), failed system and user services, graphics
  with no driver or nouveau, a stale or busy pacman lock, unfinished dpkg,
  root-owned settings (paths quoted), missing initramfs and GRUB menu, a full
  EFI partition and disk; the Helix Doctor screen's evidence, repair
  confirmation and reports; `helixforge doctor` on each distribution
  (Distributions)
- backups in a scratch home: `helixforge backups` lists one, `restore` puts
  it back and keeps the replaced file as a backup, and a path that is not a
  backup is refused; in the menu, the Backups screen lists, describes and
  confirms a restore, and a GRUB theme run gets a Snapper snapshot first
  unless Settings turns it off
- a failing fixture and a marker run together: stopped, the marker is not
  run and the retry command names both; with `--keep-going` the marker
  runs, and the history says "1 of 2 steps done"; in the menu, both stay in
  the Install Plan after "Not everything finished"
- two fixture entries that replace each other, queued together: the
  command line prints the blocker, exits without starting a step or
  recording a run, and the menu shows "Cannot run yet" instead of asking
- an update through a stand-in Flatpak: status shows it, `update` with no
  names finds it, runs `flatpak update --user` and verifies 1.0 → 1.1, and
  an update that installs nothing newer is not verified
- package entries from `tests/fixtures/packages`, against stand-in `sudo`,
  package managers and `flatpak` on the PATH: the distribution's package
  failing over to Flathub, `Verified:` lines and `exit 0, verified` in the
  history, removal verified, a source that installs nothing not being
  trusted, and the running window's `FINISHED · VERIFIED`

The fixture catalog's entries:

| Entry | Does |
| :--- | :--- |
| marker | writes a marker file where `HELIXFORGE_TEST_OUT` points |
| failing | exits with status 9 |
| report | the native system report |
| bundle | a profile of marker and report |
| greeter | asks for a name and greets it |
| sleeper | records a child's process id and sleeps |
| stubborn | the same, ignoring the hangup |

`tests/fixtures/packages` holds two package entries whose distribution
package does not exist: **fallback-sample**, which the stand-in `flatpak`
installs, and **missing-sample**, which it only pretends to install. The
engine's unit tests also run generated package programs against harmless
commands: falling back, skipping a source whose tool is missing, stopping on
an interrupted source, and removals that leave the package behind.

## Release checks

Every push to `main` runs the Checks, Distributions and Sync sources
workflows; the last publishes the next release, tagged `vMAJOR.MINOR.PATCH`,
with `helixforge-linux-x86_64` and `SHA256SUMS`, and checks that
`helixforge --version` reports that version. `scripts/release-version.py`
picks it: the commit's own version when it already has one (a rebuild), else
one patch above the newest tag, or the workspace version in `Cargo.toml` when
that is higher. Releases before v0.1.0 were named `helixforge-<commit>`. To
check a published build by hand, without running any entry:

```sh
tag=$(gh release view --repo Commanderx-code/Helix-Forge --json tagName --jq .tagName)
dir=$(mktemp -d) && cd "$dir"
gh release download "$tag" --repo Commanderx-code/Helix-Forge
sha256sum -c SHA256SUMS
chmod +x helixforge-linux-x86_64
./helixforge-linux-x86_64 --version
./helixforge-linux-x86_64 list | head
./helixforge-linux-x86_64 run system-report --dry-run
```

## Not covered

- Boot, login-screen and desktop entries (GRUB, Limine, Plymouth, SDDM)
  need a real machine or a virtual machine; containers cannot boot them.
- Real Flatpak installs, Nix entries and `sudo` password prompts are not
  run by any automated test; Flathub fallback runs against a stand-in
  `flatpak`.
- The release is built for Linux x86_64 with glibc 2.34 or later only.
- The banner picture is checked against a test terminal that answers like
  Ghostty and one that answers like a Sixel terminal, with the image the
  program sends painted into the screenshot; how real terminals draw it is
  checked by eye.
