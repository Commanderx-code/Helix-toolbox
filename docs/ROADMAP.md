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

- **Scheduled source sync never observed.** The Sync sources workflow has run
  only on pushes; no scheduled (`17,47 * * * *`) run has appeared yet. Watch
  that one does; until then sources are adopted on pushes to `main`.
- **A run is lost when HelixForge itself dies mid-run** (terminal closed,
  killed): history is written when the run ends. Writing a "started" record
  first and marking unmatched ones "did not finish" changes the history
  format, so it waits for a decision.
- **No real-machine or virtual-machine runs.** Boot, login-screen and desktop
  entries, Flatpak installs, Nix entries and `sudo` prompts have no automated
  test; containers cannot cover them.
- **The command line confirms through standard input,** so
  `curl … | sh -s -- run <entry>` cannot be confirmed; the menu is
  unaffected.

## Milestone 5: graphical interface

- Tauri 2 frontend on the same crates, with an embedded terminal panel for
  interactive entries and an event stream from the engine
