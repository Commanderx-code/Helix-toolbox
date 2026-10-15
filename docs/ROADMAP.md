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

## Milestone 5: graphical interface

- Tauri 2 frontend on the same crates, with an embedded terminal panel for
  interactive entries and an event stream from the engine
