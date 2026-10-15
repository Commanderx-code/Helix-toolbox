# Architecture

## Stack

| Component | Choice |
| :--- | :--- |
| Language | Rust (edition 2024) |
| Terminal interface | Ratatui and Crossterm |
| Graphical interface, later | Tauri 2 on the same crates |
| Command line | Clap |
| Interactive execution | portable-pty, rendered with tui-term and vt100 |
| Configuration and catalog | Serde and TOML |
| Errors | thiserror in the crates, anyhow in the application |
| Logging | tracing, to `~/.local/state/helixforge/logs/helixforge.log` |
| Tests | Cargo tests with fixtures; Linux VM runs for real installers, later |
| Releases | GitHub Actions, later |

An async runtime (Tokio) is not in the workspace yet. The engine reports a
running command's output through a shared buffer and a flag, which the
terminal interface polls. The graphical interface will want an event stream;
that is the point to add Tokio, with the engine exposing a channel instead of
a buffer.

## Principles

1. Every interface calls the same engine: plan, review, run, record. The
   interface, terminal now and graphical later, never changes the system.
2. Entries come from the catalog embedded in the binary. There is no remote
   catalog and no downloaded script.
3. A run is one POSIX shell program built from quoted literals out of the
   validated catalog. Nothing typed by the user reaches it.
4. Every run is shown as a plan, with its risk and whether it asks for a
   password, and confirmed before it starts. Then it is recorded, including
   runs that failed or were interrupted.
5. Privileges come from `sudo` inside the run, where the prompt is visible;
   there is no daemon and nothing stays elevated.

## Crates

```text
helixforge-core      Tab, Group, Entry, Action, Precondition, Risk; name validation
helixforge-catalog   reads catalog/ and scripts/ (embedded or from a directory), applies preconditions
helixforge-platform  /etc/os-release, package manager family, PATH lookups
helixforge-engine    plan() -> Plan of Steps; shell_program(); Runner (inherited, pty, mock); history
helixforge           clap command line, ratatui interface, tracing setup
```

## The operation model

An entry's `Action` is one of:

| Action | Becomes |
| :--- | :--- |
| `Package` | `sudo <manager> install <package>`, or a note when its binary is already on the PATH |
| `Script` | `cd <its directory> && <interpreter> <script>`; interactive by default |
| `Command` | `sh -e -c '<line>'` |
| `Nix` | `nix profile install <installable>` or `home-manager switch --flake <flake>` |
| `Native` | `helixforge native <action>`: HelixForge re-invoked, the operation implemented in Rust |
| `Profile` | the actions of the entries it names, in order |

`plan()` turns entries into a `Plan`: ordered `Step`s, notes for entries that
need nothing done, the executables the steps require, whether any step is
privileged, and the highest risk among them (from the flags: `D`, `K` and
`SI` are high; installs, services and anything privileged are medium).

## A run, end to end

```text
entries ──plan──▶ Plan ──shell_program──▶ sh program ──▶ Runner
                                                         ├─ pty::Session     (interface window)
                                                         ├─ InheritedRunner  (command line)
                                                         └─ MockRunner       (tests)
                                                                 │
                                                          Outcome: Success | Failed{code} | Interrupted | CouldNotStart
                                                                 │
                                                          history::record
```

Steps run in order and stop at the first failure. An exit status of 128 or
more, or a kill from the interface, is recorded as interrupted, never as
success. Native steps re-enter HelixForge through its hidden `native`
subcommand, so even Rust-implemented operations go through the same program
and the same terminal, and will through the graphical interface's terminal
panel too.

## Interface

A header with the name, tagline and a status badge; a sidebar menu of
screens in two sections; the current screen; a hint line. The screens are
the Dashboard (system overview and quick actions), one screen per catalog
area (search bar and entry list with the flags column, groups opened in
place), the Install Plan (the queue, planned live), History (the records,
with saved logs) and Settings. Popups over the current screen show text, the
confirmation with the plan's risk line, and the running terminal. All of it
is one `App` in `apps/helixforge/src/tui/app.rs` driven by key events; screen
tests render it into a test backend.

## Graphical interface, later

A Tauri crate uses the same engine: plans for its forms, the pseudo-terminal
session for an embedded terminal panel where entries are interactive, and
the history for its log view. Nothing in the engine assumes a terminal.
