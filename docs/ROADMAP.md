# Roadmap

## The 1.x release plan

| Version | Main objective | Items |
| :--- | :--- | :--- |
| v1.1.0 | Configuration drift and the command palette | 1 (HELIX Drift), 6 (command palette) (done) |
| v1.2.0 | How far each installer can be trusted | 4 (reliability dashboard), 7 (trust inspector) |
| v1.3.0 | Seeing a change before it happens, and the hardware | 2 (system change preview), 5 (hardware and firmware center) |
| v1.4.0 | First-run setup and the desktop | 3 (setup wizard), 9 (desktop environment manager) |
| v1.5.0 | Machines without reliable internet | 8 (offline installation bundles) |

The items:

1. HELIX Drift: compare a machine with a saved baseline or a workstation
   profile: missing applications, changed settings, replaced configuration
   files and other differences, such as what a fresh installation still
   lacks from the original setup.
2. System change preview: before a change runs, the files it replaces
   side by side, the services it restarts and the packages it may replace;
   effects that cannot be known in advance (inside complex scripts) are
   marked as such rather than claimed.
3. Setup wizard: a first-run guide that detects the distribution, desktop,
   hardware and package managers, offers Developer, Gaming, Privacy,
   Minimal and Custom workstations, and makes a normal Install Plan to
   review; nothing is applied by itself.
4. Installer reliability: which installers passed automated tests, on
   which distributions and when, labelled Container Tested, VM Tested,
   Hardware Tested or Not Yet Validated; above all for bootloader and
   graphics-driver entries.
5. Hardware and firmware center: read-only fwupd, SMART and NVMe health,
   temperatures, battery health and PCI and USB devices; firmware updates
   listed apart, each with its own explicit confirmation.
6. Command palette: `Ctrl-K` searches every screen, action and entry.
7. Trust inspector: an installer's source repository, pinned revision,
   checksums and signatures, licence, last validation and privileges,
   telling verified release provenance from package integrity checks and
   from unverified third-party scripts.
8. Offline bundles: HelixForge, its catalog and predownloaded packages for
   one distribution and architecture, with packages and signatures still
   verifiable, and the actions that need the network named.
9. Desktop environment manager: detect KDE Plasma, GNOME, XFCE and others,
   and offer the settings, themes, display-manager configuration and
   customisation profiles that fit, warning where Home Manager or a
   desktop's own tools own a setting.

## v1.1.0: configuration drift and the command palette (done)

- [x] HELIX Drift (item 1): baselines of what is installed, the
      configuration files entries write (by checksum), machine settings and
      the entries HelixForge ran; saved, listed, exported and imported as
      TOML; compared with this machine or a profile on the Drift screen and
      with `helixforge drift check` (`--json`, `--exit-code`), with the
      entries that restore what is missing queued for review
- [x] Command palette (item 6): `Ctrl-K` over any screen, with every
      screen, the common actions and every entry, ranked like search

## Release plan to 1.0

| Version | Main objective | Roadmap items |
| :--- | :--- | :--- |
| v0.2.0 | Smart application detection, ownership and installed-state accuracy | status, updates, package identity (done) |
| v0.3.0 | Dependency and conflict handling, more robust installation planning | 2 (dependencies and conflicts), 8 (installation queue) (done) |
| v0.4.0 | Unified backup and recovery, enhanced HELIX Doctor | 3 (backup and recovery center), 5 (HELIX Doctor 2.0) (done) |
| v0.5.0 | Workstation profiles, favourites and search | 4 (profiles 2.0), 7 (search and favourites) (done) |
| v0.6.0 | Unified update overview and JSON command line | 6 (update manager), 9 (JSON output) (done) |
| v0.7.0 | Packaging, usability, documentation and beta testing | 10 (native packaging) (done) |
| v1.0.0 | Stable, validated public TUI release | (done) |

The items:

2. Dependency and conflict detection: before installing, check
   prerequisites, package conflicts, alternatives, free space and existing
   installations (WINE and WINE Staging, incompatible graphics drivers).
3. Backup and recovery center: existing configuration backups in one place,
   with the files they cover and how to restore them; optional Btrfs/Snapper
   snapshots before high-risk changes where the system supports them. No
   package rollback is promised unless the system provides it.
4. Workstation profiles 2.0: presets of applications, shell settings,
   dotfiles and optional tweaks, exported and imported as TOML.
5. HELIX Doctor 2.0: guided troubleshooting (DNS, drivers, failed services,
   package databases, permissions, boot configuration) that shows evidence
   and offers a separately confirmed repair where one is supported.
6. Unified update manager: updates across the native package manager,
   Flatpak, AUR helpers and supported Nix workflows, delegated to those
   tools.
7. Search and favourites: fuzzy search, an installed-only filter, recently
   used entries, pinned favourites, quick actions, and selections kept when
   moving between categories.
8. Installation queue: dependency ordering, skip or continue policies,
   per-operation status and recovery instructions after a partial failure,
   with package-manager operations kept serial.
9. JSON output: stable JSON for system, list, plan, history and operation
   status, for automation, other tools and the GUI.
10. Native packaging: an Arch package, Debian and RPM packages, perhaps ARM64
    builds, with signed provenance and verification instructions.

## Milestone 1: core foundation (done)

- Cargo workspace: core, catalog, platform, engine, application
- Catalog tree of tabs, groups and entries from TOML, embedded at build time,
  with stable ids, preconditions, flags, ordering and multi-select
- Operation model: package, script, command, Nix, native, profile
- Explicit plans with requirements, privilege and risk; mock, inherited and
  pseudo-terminal runners through one trait; outcomes that tell failure
  from interruption; JSONL history; tracing logs
- Terminal interface with the previous toolbox's layout and keys
- Command line: system, list, info, plan, run, history, docgen
- Tests in every crate, screen tests of the interface, and fixture-based
  end-to-end tests that run a safe script through a real shell

## Milestone 2: functional TUI (done)

- Sidebar as a menu of screens after the dashboard design: Dashboard,
  Applications and Install Plan under Main menu; Configurations, Helix
  Doctor, Maintenance, Security, Profiles, History and Settings under More
  tools; a header with the status badge and a hint line at the bottom
- Dashboard with the system overview and quick actions that open screens
- Install Plan: the queue built with Space in any area, planned live with
  step count, risk, requirements and notes; run, remove or clear from there
- History screen over the JSONL records, opening saved logs
- Settings: theme, confirmation, showing hidden entries (reloads the catalog)
- Help popup with every key

## Milestone 3: installer compatibility (done)

- [x] Bring the Commander installers across with their shared helpers and
      tests: 44 entries in Applications, Configurations, Helix Doctor,
      Maintenance and Security, run through `HELIXFORGE_LIB`
- [x] Home Manager ownership protection as an engine check: entries declare
      what they write, and a Home Manager symlink there blocks the entry in
      the plan (the installers keep their own refusal as a second line)
- [x] Port the dotfiles and Myfish source synchronisation to catalog files
- [x] Release workflow (Linux x86_64 build with a checksum) and `start.sh`
- [x] First release published from the workflow

## Milestone 4: feature parity (done)

- [x] Bring the general catalog across: 177 entries, scripts unchanged
- [x] Every carried-over script parses, every entry names an existing script,
      and the catalog gates shellcheck errors
- [x] The migration matrix is all "done" (two entries dropped on purpose)
- [x] Distribution checks: the Distributions workflow runs HelixForge in Arch,
      Debian, Ubuntu and Fedora containers on every push and weekly, with real
      package installs and removals (containers stand in for virtual machines;
      boot, login-screen and desktop entries still need a real machine)
- [x] Turn plain package installs into data-only package entries: 38 of them,
      with AUR and Flatpak routes and removal
- [x] Retire Commander Toolbox: the dotfiles `toolbox` command opens
      HelixForge, and the old repository is archived

## Phase 1: release stability and safe end-to-end testing (done)

Reviewed on 2026-10-15 against release `helixforge-0eac13844ec2`, without
running any entry on a real machine. `docs/TESTING.md` describes the layers.

- [x] Local verification suite passes; it now also runs the installer tests,
      shellcheck and a fresh render of the dotfiles and Myfish exports
- [x] Checks, Distributions and Sync sources pass for the release commit; the
      release is not a draft, targets that commit and carries the binary and
      `SHA256SUMS`
- [x] Release integrity: the checksum matches, the binary needs glibc 2.34,
      `--version` and `--help` work
- [x] The built program is driven in a pseudo-terminal against a fixture
      catalog: navigation, search, install plan, confirmation, typing into a
      run, `Ctrl-C`, history, resizing, and the command line
- [x] The Checks workflow on pull requests runs the installer tests,
      shellcheck and the source render, which only the release workflow ran
      before (a shellcheck failure once reached `main` that way)

Defects found and fixed:

- Opening the menu without a terminal panicked (exit 101); it now says the
  menu needs a terminal. It checks for a controlling terminal rather than
  standard input, so `curl … | sh` keeps working.
- A key pressed together with a terminal resize stayed unread until the next
  key, so a `Ctrl-C` right after a resize did nothing. crossterm's default
  event source is edge-triggered; HelixForge now uses its `poll(2)` source.
- `Ctrl-C` signalled only the run's shell: a child ignoring the hangup kept
  running while the window said INTERRUPTED. The whole process group is
  signalled now, survivors are shown and recorded, and a second `Ctrl-C`
  kills them.
- A killed HelixForge left its unpacked catalog in `/tmp` for good. Copies
  hold a lock now, and the next start removes copies nobody holds.
- A run whose history could not be written was silently unrecorded; the
  command line and the menu now say so.

Remaining before the next phase:

- [x] **Scheduled source sync never observed.** GitHub fired the half-hourly
  schedules on these repositories only every two to six hours. dotfiles and
  Myfish now start the sync when their checks pass on `main`, once they hold
  the `HELIXFORGE_DISPATCH_TOKEN` secret (see `docs/SOURCE-SYNC.md`).
- [x] **A run was lost when HelixForge itself died mid-run.** Fixed: a run
  keeps a marker beside the history while it is in progress, and the next
  read records an orphaned one as not finished, or with its real result if
  it had ended. `history.jsonl` keeps its format.
- **No real-machine or virtual-machine runs.** Boot, login-screen and desktop
  entries, Flatpak installs, Nix entries and `sudo` prompts have no automated
  test; containers cannot cover them.
- [x] **The command line confirmed through standard input,** so
  `curl … | sh -s -- run <entry>` could not be confirmed. It asks on the
  terminal now.

## Install verification and fallback sources (done)

- [x] After a run, every package install and removal is checked: the program
      on the PATH, the package manager's list (`pacman -Q`, `dpkg-query`,
      `rpm -q`, `apk info -e`, `xbps-query`) or the Flatpak's directory. The
      running window, the command line and the history say `verified`; a run
      that succeeded without its package is recorded as failed
- [x] Package entries fall back to the next source, the AUR on Arch and then
      Flathub, when one fails or leaves nothing installed; a source whose tool
      is missing is skipped, and an interrupted command stops the run
- [x] Tests: generated programs against harmless commands, the menu and the
      command line against stand-in package managers, and the Distributions
      workflow with each real package manager failing over to a stand-in
      Flathub

Found while doing it: seven package entries (Meld, Slack, Zoom, ZapZap, WPS
Office, Waterfox, Zen Browser) gave their Flatpak ID as `binary`; fixed in
"Installed state from package identity" below.

## Installed state from package identity (done)

- [x] Whether a package entry is installed comes from its own packages (the
      family's, the AUR's on Arch, its Flatpak), asked of the package manager
      and Flatpak, not from a command on the PATH; plans, removals and
      verification all ask in that order
- [x] `conflicts` names the packages an entry replaces: WINE Staging is no
      longer taken as installed because WINE's `wine` command is on the
      PATH, and the plan says WINE will be replaced
- [x] `binary` is optional and only recognises installs made another way
      (Nix, Home Manager, a download); the seven Flatpak IDs became the real
      commands, and the compatibility tools and WINE Meta, which add none,
      have none
- [x] The Install Plan screen keeps its plan for a few seconds and drops it
      after a run, since planning now asks the package manager

## After 1.0

- ARM64 builds and packages, once there is an ARM64 machine to test them on
- The graphical interface (Milestone 5 below), on the same engine
- What testers report (docs/REPORTING.md)

## v1.0.0: stable, validated public release (done)

- [x] Validated on more systems: openSUSE Tumbleweed and Ubuntu 22.04 (the
      oldest glibc the packages support) join the Distributions containers;
      37 more entries name their openSUSE package, each checked against the
      repositories there, which every openSUSE name in the catalog now is
- [x] Damaged settings, favourites, profiles and history never stop the
      menu or the commands
- [x] A written compatibility promise for 1.x (docs/STABILITY.md):
      commands, exit status, JSON schema 1, your files, entry ids; settings
      read one by one, so a later version's settings cost nothing
- [x] A monochrome theme, chosen for the run when `NO_COLOR` is set
- [x] Security review: every action in the workflows pinned to a commit
- [x] Documentation for a stable release

## v0.7.0: packaging, usability, documentation and beta testing (done)

- [x] Native packaging (item 10): an Arch package, a Debian package and an
      RPM from each tested release build, made with nfpm by
      `scripts/package.sh`, with bash, zsh and fish completions, manual
      pages, a menu entry and icon; installed, used and removed in the
      Arch, Debian, Ubuntu and Fedora containers; SHA-256 sums and signed
      build provenance attestations for every published file, with
      verification instructions in docs/INSTALL.md
- [x] `helixforge report`: version, how it was installed, the system,
      tools, terminal, settings, recent runs and health checks, for an
      issue, with the home folder shown as `~`; `--json` too
- [x] Issue forms for problems and ideas, asking for the report
- [x] A user guide (docs/GUIDE.md), a beta testing guide (docs/REPORTING.md) and
      a changelog
- [ ] ARM64 builds: moved to after v1.0, when there is a tester with an
      ARM64 machine

## v0.6.0: unified update overview and JSON command line (done)

- [x] Unified update overview (item 6): an Updates screen and `helixforge
      updates` with what pacman, apt, dnf or zypper, the AUR helper,
      Flatpak's installations, Home Manager, NixOS and a Nix profile have
      waiting, offline from what they know or online through read-only
      commands; updates delegated to those tools, one after another, after
      confirmation
- [x] JSON output (item 9): `--json` on system, list, search, info,
      status, plan, history, updates, doctor, backups and profile list, and
      on every `--dry-run` plan, in versioned shapes documented in
      docs/JSON.md; refused for anything that changes the machine
- [x] Command-line output piped into `head` ends quietly instead of with a
      broken pipe error

## v0.5.0: workstation profiles, favourites and search (done)

- [x] Workstation profiles 2.0 (item 4): save the Install Plan as a named
      profile of applications, shell settings, dotfiles and tweaks; profiles
      as TOML in the user's configuration, shown under Profiles beside the
      built-in ones; export, import (checked against the catalog), list and
      delete from the command line
- [x] Search and favourites (item 7): ranked search with loose spellings
      (`helixforge search` too), an installed and an updates filter, pinned
      favourites and recently run entries on a Favourites screen and
      Dashboard quick action, and the row kept when a search is cleared

## v0.4.0: backup and recovery, HELIX Doctor 2.0 (done)

- [x] Backup and recovery center (item 3): one Backups screen and
      `helixforge backups` for user and system configuration backups, with
      when each was taken and what it covers; restores that keep the
      replaced file as a backup; a Snapper or Timeshift snapshot first before
      system changes where the machine supports it, without promising
      package rollback
- [x] HELIX Doctor 2.0 (item 5): read-only checks of network, DNS,
      services, graphics drivers, the package database, home permissions,
      boot images, the GRUB menu, the EFI partition and disk space, each
      with its evidence, advice and a separately confirmed repair or entry
      where one is supported; Helix Doctor opens them, its reports below

## v0.3.0: dependencies, conflicts and the installation queue (done)

- [x] Checks before a run (item 2): missing tools, entries that replace
      each other, packages that would be replaced, Arch repositories,
      graphics hardware and free space, as blockers and warnings in the
      plan, the confirmation and the command line; `[needs]` in the catalog
      (Nvidia drivers, Steam and the CachyOS sets, DaVinci Resolve)
- [x] The installation queue (item 8): steps ordered after the steps that
      install what they need or that they name in `needs.after`; stop or
      continue after a failure (Settings, `--keep-going`), skipping only what
      depends on the failed step; each step's result in the window, the
      command line and the history; unfinished entries stay in the Install
      Plan and the command line prints the retry; one HelixForge run at a
      time

## Smart status (done)

- [x] Package entries show Installed (source and version), Update (from the
      repository metadata already on the machine), Managed (Nix, Home
      Manager, NixOS, with the store path's version) or Not installed, and a
      Nix copy of a packaged command that comes first on the PATH; the menu
      checks in the background on start and after every run, and
      `helixforge status` prints the same
- [x] Update action: `u` and `U` in the menu, `helixforge update`; one
      package through Flatpak, apt, dnf or zypper, the whole system with
      `pacman -Syu` on Arch; verified by the version changing
- [x] Script entries say what they install in `[detect]` (52 application
      scripts: Steam, Lutris, Heroic, VS Code, the browsers and more), so
      they show status and update through that package; a command found in
      none of an entry's packages is traced to the package that owns it

## Logo and banner (done)

- `assets/`: the banner (README and the menu's header), the transparent logo,
  the app icon and the README's screenshots
- The menu's header shows the banner in terminals with Kitty's graphics or
  Sixel, found by one query at start with a 0.4 second limit; Konsole, tmux,
  screen, the console, plain icons and the compatible theme keep the text
  header

## Milestone 5: graphical interface

- Tauri 2 frontend on the same crates, with an embedded terminal panel for
  interactive entries and an event stream from the engine
- Window and launcher icons made from `assets/icon.png`
