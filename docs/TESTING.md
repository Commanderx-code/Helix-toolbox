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
| Crates | Checks workflow | `cargo package --workspace`: every crate packaged and built from its package, as crates.io and `cargo install` would |
| Virtual machines | `scripts/vm-tests.py`, in the Distributions workflow | the boot, login-screen, disk and kernel entries run for real under QEMU, in Arch, Debian and Fedora cloud images on BIOS and UEFI firmware, the Arch live ISO and an Arch machine that says it is a Steam Deck: each is run through `helixforge run`, rebooted into and checked, with the boot screens kept |
| Distributions | `.github/workflows/distributions.yml` | Arch, Debian, Ubuntu 22.04 and 24.04, Fedora and openSUSE Tumbleweed containers with real package installs and removals, each verified, and the real package manager failing over to a stand-in Flathub; HelixForge's own package for each installed, used and removed. The ARM64 build's tests, and the same containers on ARM64 runners, Arch being Arch Linux ARM's own root file system checked against its build key (`scripts/arch-arm-tests.sh`), with the x86_64-only entries left out |

## The program in a terminal

`terminal.rs` starts the real binary in a pseudo-terminal with the hidden
`--catalog-dir tests/fixtures` option, reads the screen with a terminal
emulator and types keys. It checks:

- sidebar navigation, the help popup, search and clearing it
- pinning an entry, `favorites.toml`, and the entry under Favorites after a
  restart
- the Updates screen between the catalog areas and History; HelixForge's
  own row for each way of installing it, and `fetch-release` keeping a file
  that matches the release's SHA256SUMS and deleting one that does not
  (from a release folder served over `file://`)
- completions for bash, zsh and fish, and the manual pages, naming the
  commands and leaving out hidden ones
- unreadable settings, favorites, profiles and history lines: skipped or
  replaced by defaults, with the menu and commands still working; settings
  from a later version (unknown names and values) keep the ones understood
- the monochrome theme drawing no color anywhere, the selection in reverse
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
  is a pipe, and a wrong word canceling
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

## Virtual machines

`scripts/vm-tests.py --distro MACHINE` boots one of these under QEMU with
KVM, each image checked against its published checksum:

| Machine | What it is |
|---|---|
| `arch`, `debian`, `fedora` | the distribution's cloud image on BIOS firmware, with a USB stick plugged into the Arch one |
| `opensuse` | the openSUSE Tumbleweed cloud image on BIOS firmware, for offline bundles |
| `arch-themes` | the Arch cloud image on BIOS firmware, for the GRUB themes, with Garuda's repository added for its theme |
| `arch-uefi` | the Arch cloud image on UEFI firmware (OVMF) |
| `arch-iso` | the Arch live ISO on UEFI firmware, as root, with an empty disk |
| `steamdeck` | the Arch cloud image with firmware that names Valve as the board's maker, as a Steam Deck's does |

The cloud images have a user in the administrators' group with
passwordless `sudo`, and are first made to boot like a desktop installation:
GRUB and the kernel on the screen rather than the serial line, and the boot
menu shown for five seconds. Their disks take TRIM, as an SSD does. Then
each case runs its entry through `helixforge run` in a terminal, answering
the installer's questions as they come (`APPLY`, the package manager's
`[Y/n]`, yes to `[y/N]`, and the entry's own), reboots, captures the screen
every half second while the machine boots, and checks the result:

| Entry | Machines | Checked |
|---|---|---|
| Dracula GRUB Theme | Arch, Debian, Fedora, Arch UEFI | `GRUB_THEME` names the installed theme, graphical output is selected, and a graphical boot menu was on the screen |
| The other GRUB themes (CachyOS, Catppuccin, Tela, Vimix, the six CTT themes, and Garuda's from its repository) | Arch themes | the same as Dracula's |
| Restore Default GRUB Appearance | Arch, Debian, Fedora, Arch UEFI | no theme is selected, and the machine boots |
| Enable Boot Splash | Arch, Debian, Fedora | `splash` is on the kernel command line, Plymouth is in the boot image and started |
| Boot Splash Theme | Arch | another installed theme chosen by its number is selected, in the boot image, and Plymouth started at the next boot |
| Dotfiles Boot Splash | Arch | without the KDE Store theme dotfiles names, it says where to get it and changes nothing; with a theme of that name installed, as the store's Get New would, it is selected, in the boot image, and Plymouth started at the next boot |
| Disable Boot Splash | Arch, Debian, Fedora | `splash` is gone from the kernel command line |
| AppArmor Setup and Status | Arch, Debian | AppArmor is enabled in the kernel (on Arch after the first phase's reboot), and its service is enabled and running |
| Fedora SELinux Tools and Status | Fedora | the tools are installed, `semanage` works, and SELinux still enforces after the reboot |
| SDDM Theme | Arch, Debian, Fedora | its preview ran (on a virtual display, closed after 20 seconds), the silent theme is installed and selected, and SDDM shows a graphical login screen after the reboot |
| Enable SSD TRIM | Arch, Debian, Fedora | the first trim trimmed the root file system, and `fstrim.timer` is enabled, running and scheduled after the reboot |
| Bootable USB Creator | Arch | the stick holds the image byte for byte (a small GRUB image made in the machine), and boots on BIOS and UEFI firmware |
| CachyOS and Garuda Limine Boot Menus | Arch UEFI | after the machine is made to boot through Limine, as CachyOS installs it: the look is merged with the boot entry unchanged and nothing left of the previous look, the wallpaper is beside `limine.conf`, the machine boots through Limine and the wallpaper was on the screen |
| Arch Server Setup | Arch live ISO | it installs on the empty disk (btrfs, the first keyboard layout and locale); the installed system boots on its own and reports on the serial line that it started cleanly, with the user in `wheel`, NetworkManager running, root on the `@` subvolume, the swap file in use, the host name and the LTS kernel |
| Linux Neptune for SteamDeck | Steam Deck stand-in | linux-neptune, its speaker firmware and a GRUB entry are installed, and the machine boots it |

The Arch machine also makes an offline bundle of a Flatpak (Flycast) it
installed from Flathub: `bundle create` first names the collection ID
Flathub needs, then copies the application with its runtimes. With
everything Flatpak uninstalled, Flathub added again without a collection ID
and the machine restarted with no way out to the network, `bundle install`
gives the remote its collection ID and installs Flycast from the bundle,
which then runs. With the remote removed too, the install is refused, and
with `--trust-bundle` the remote is added with the bundle's copy of its key.
The openSUSE machine has zypper download a bundle of htop as the user,
without root, one with the dependencies the machine lacks and one with all
of them; restarted with no network, it installs the complete one.

The captured screens (the boot menus, the splash, the login screen, the
preview) are kept as the job's artifact, for a person to look at. Entries
verified here are **VM Tested** on the Reliability screen. Real graphics
hardware and firmware other than QEMU's are not covered. It runs on a
machine with KVM and OVMF too, in a folder of its own:

```sh
python3 scripts/vm-tests.py --distro arch --binary target/release/helixforge \
  --work /tmp/vm --out /tmp/vm/arch.json --shots /tmp/vm/screens
```

## The reliability record

Each Distributions container ends by turning its own HelixForge history into
a record of the entries it ran successfully, and which of those HelixForge
verified (`scripts/validation-record.py record`); a last job merges them into
one `validation.json`. The release waits for Distributions on its commit,
publishes nothing if that run failed, and builds `validation.json` into the
program as `catalog/validation-results.json` (a source update, which has no
run of its own, takes its ancestor's). The Reliability screen and
`helixforge reliability` read it beside `catalog/validation.toml`, which
lists the installer tests that cover each entry with stand-ins, the
critical entries, and runs reported from virtual and real machines.
`tests/installers/test_validation.py` checks that every entry and test that
file names exists.

## Release checks

Every push to `main` runs the Checks, Distributions and Sync sources
workflows; the last waits for Distributions on the same commit, then
publishes the next release, tagged `vMAJOR.MINOR.PATCH`,
with `helixforge-linux-x86_64`, `helixforge-linux-aarch64`, the packages and
`SHA256SUMS`, and checks that
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

- Real graphics drivers (the NVIDIA entries), real firmware and a real
  Steam Deck: the virtual machines have QEMU's graphics and firmware. The
  Reliability screen shows the boot and driver entries as critical, and a
  run of one that is not tested in a virtual machine says so before it
  starts.
- Real Flatpak installs, Nix entries and `sudo` password prompts are not
  run by any automated test; Flathub fallback runs against a stand-in
  `flatpak`.
- The releases are built for Linux x86_64 and aarch64 with glibc 2.35 or
  later. The ARM64 build runs the tests and the containers on GitHub's ARM64
  runners, but not the virtual machines.
- The banner picture is checked against a test terminal that answers like
  Ghostty and one that answers like a Sixel terminal, with the image the
  program sends painted into the screenshot; how real terminals draw it is
  checked by eye.
