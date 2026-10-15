# Working on HelixForge

Guidance for anyone, person or coding agent, changing this repository.

## What this is

A Rust workspace: `apps/helixforge` (command line and terminal interface),
`crates/helixforge-core` (data model), `crates/helixforge-catalog` (reading
and validating `catalog/` and `scripts/`), `crates/helixforge-platform`
(distribution detection), `crates/helixforge-engine` (plans, runners, the
pseudo-terminal, history). `docs/ARCHITECTURE.md` explains how they fit;
`docs/MIGRATION.md` tracks what still has to come across from the previous
toolbox at `../commander-toolbox`, which stays the stable version and the
reference for behaviour.

## Rules

- Interfaces never change the system. They ask the engine for a plan and run
  it through a runner. New operations go in the engine, not in the interface.
- The shell program never contains free text: only quoted literals from the
  validated catalog. Keep `helixforge_core::valid_package_name` and
  `valid_id` in front of anything that reaches a command line.
- Tested installers from the previous toolbox are brought across, not
  rewritten, unless there is a reason that is written down.
- Tests must not change the machine they run on. Fixtures live in
  `tests/fixtures/`; real runs in tests write only to temporary directories.
- Keep the Commander Toolbox repository untouched.

## Checks

`scripts/verify.sh` runs everything the workflow runs: `cargo fmt`, `cargo
clippy -D warnings`, `cargo test --workspace`, a release build, and the
walkthrough regeneration (`helixforge docgen docs/walkthrough.md`, which
must leave no diff). Run it before committing.

## Conventions

- Commits are plain messages in the repository owner's name, with no
  attribution lines for tools or assistants.
- Catalog entries are one TOML file per entry; see `README.md` for the
  format. Scripts referenced by id live in `scripts/installers/`; the
  installers from the previous toolbox are under `scripts/installers/commander/`
  with their tests in `tests/installers/`, and `scripts/verify.sh` runs both.
- Documentation does not say that something was written by an AI tool.
