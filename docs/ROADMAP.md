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

## Milestone 3: installer compatibility

- Bring the Commander installers across with their shared helpers and tests
- Home Manager ownership protection as an engine check
- Port the dotfiles and Myfish source synchronisation
- Release workflow: static Linux x86_64 build, checksum, launcher

## Milestone 4: feature parity

- Bring the general catalog across: package entries as data, scripts otherwise
- Distribution checks in disposable VMs; the migration matrix all "done"
- Retire the Commander Toolbox as the stable version

## Milestone 5: graphical interface

- Tauri 2 frontend on the same crates, with an embedded terminal panel for
  interactive entries and an event stream from the engine
