# Architecture

## Principles

1. The interface, terminal now and graphical later, never changes the system
   itself. It asks the engine to plan and run entries.
2. Entries come from the catalog embedded in the binary. There is no remote
   catalog and no downloaded script.
3. A run is one POSIX shell program built from quoted literals out of the
   validated catalog. Package names, script paths and command lines never meet
   user input.
4. Every run is shown before it starts and confirmed, then recorded.
5. Privileges come from `sudo` inside the run, visible in the terminal window,
   never from a daemon.

## Crates

```text
helixforge-core      Tab, Group, Entry, Action, Precondition; validation of names and ids
helixforge-catalog   reads catalog/ (embedded or from a directory), applies preconditions
helixforge-platform  /etc/os-release, package manager family, PATH lookups
helixforge-engine    plan(entry) -> Step, shell_program(steps), pty::Session, history
helixforge           clap command line, ratatui interface
```

## A run, end to end

```text
entry ──plan──▶ Step ──shell_program──▶ sh program ──▶ pty::Session (interface)
                                                   └─▶ run_inherited  (command line)
                                                           │
                                                   history::record
```

- A package entry becomes `sudo <manager> install <package>`; when the entry's
  binary is already on the PATH it is reported instead of run.
- A script entry runs `cd <its directory> && <interpreter> <script>`; the
  interpreter comes from the shebang, `/bin/sh -e` without one, and an entry
  whose interpreter is missing is hidden like one whose preconditions fail.
- A command entry runs `sh -e -c '<line>'`.
- Steps run in order and stop at the first failure.

## Interface

Tabs on the left with a system panel, the search bar and entry list on the
right, key hints along the bottom. Popups over the list show text, the
confirmation and the running terminal, which is a pseudo-terminal rendered
with tui-term so prompts, colours and full-screen programs work.

## Graphical interface, later

A Tauri crate would use the same `helixforge-engine`: plans for its forms, the
pseudo-terminal session for an embedded terminal panel where entries are
interactive, and the history for its log view. Nothing in the engine assumes a
terminal.
