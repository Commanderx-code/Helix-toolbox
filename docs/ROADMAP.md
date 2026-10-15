# Roadmap

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

## Smart status (done)

- [x] Package entries show Installed (source and version), Update (from the
      repository metadata already on the machine), Managed (Nix, Home
      Manager, NixOS, with the store path's version) or Not installed, and a
      Nix copy of a packaged command that comes first on the PATH; the menu
      checks in the background on start and after every run, and
      `helixforge status` prints the same
- [ ] Next: an update action per entry (Flatpak, apt and dnf can update one
      package; on Arch a single package means a partial upgrade, so the
      system update entries are the way there), and status for script
      entries, which would need to say what they install

## Logo and banner (done)

- `assets/`: the banner (README and the menu's header), the transparent logo
  and the app icon
- The menu's header shows the banner in terminals with Kitty's graphics or
  Sixel, found by one query at start with a 0.4 second limit; Konsole, tmux,
  screen, the console, plain icons and the compatible theme keep the text
  header

## Milestone 5: graphical interface

- Tauri 2 frontend on the same crates, with an embedded terminal panel for
  interactive entries and an event stream from the engine
- Window and launcher icons made from `assets/icon.png`
