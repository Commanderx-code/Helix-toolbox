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
| `Package` | its sources in order: `sudo <manager> install <package>`, the AUR helper on Arch, Flathub; or a note when it is already installed |
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
                                                          verify() on success ──▶ Finished { outcome, checks }
                                                                 │
                                                          history::record
```

Steps run in order and stop at the first failure. An exit status of 128 or
more, or a kill from the interface, is recorded as interrupted, never as
success.

Inside a package step the program tries each `Source` until one exits 0 and
leaves the package installed, as the source's `Probe` says (`pacman -Q`,
`dpkg-query`, `rpm -q`, `apk info -e`, `xbps-query`, or the Flatpak's
directory); only a source without a probe falls back to the entry's command
on the PATH. Before planning, `package_state()` asks the same probes about
the entry's own packages, then about the packages it `conflicts` with, and
only then whether its optional `binary` is on the PATH: `Installed`,
`OnPath` (an install HelixForge did not make, such as Nix's) or `Absent`
with the installed packages it would replace.

`inventory::Inventory` is the same knowledge for showing it. `gather()` runs
a few read-only queries once (`pacman -Q` and `-Qu`, `dpkg-query -W` and
`apt list --upgradable`, `rpm -qa` and `dnf -C list --upgrades`, `flatpak
list` and `flatpak remote-ls --updates --cached`), none of which needs root
or the network, and answers probes from that snapshot. `status()` turns an
entry into a `Status`: `Installed` and `Update` (source, version, newer
version, and a Nix copy of the command found first on the PATH), `Managed`
(the command resolves into the Nix store: Home Manager when the current
generation's `home-path` has the same file, NixOS for
`/run/current-system/sw`, else Nix, with the version from the store path),
`Elsewhere` and `Absent`. The interface gathers it on a background thread
when it opens and after each run; `helixforge status` prints it.

`preflight::check()` looks at a plan before it runs and returns `Finding`s,
each a blocker or a warning: requirements no source can do without
(`missing_requirements()`, which `check_requirements()` also uses), entries in
the plan whose `conflicts` name each other's packages, entries that replace an
installed package, `needs.repositories` missing from `/etc/pacman.conf` on
Arch, `needs.gpu` vendors absent from the PCI display controllers in
`/sys/bus/pci/devices`, and free space on `/` against the packages' installed
sizes and `needs.space_mb`. `Facts` carries those questions so tests answer
them. The command line prints the findings with the plan and refuses a
blocked run; the interface shows them in the Install Plan's summary and the
confirmation, and a blocked plan opens "Cannot run yet" instead.

After planning, `arrange()` orders the steps: each comes after a step whose
entry installs a command it needs (`identity().binary`) or that its entry
names in `needs.after`, the queue's order deciding the rest (Kahn's algorithm
taking the earliest ready step; a cycle keeps the queue's order). `Plan.depends`
keeps those links and `Plan.provided` the commands the plan installs, which
`missing_requirements()` then excuses. `shell_program_with()` writes each
step's outcome to a status file (`index state exit`); with `Plan.keep_going`
a failed step no longer ends the program, steps that depend on it are skipped,
and an exit status of 128 or more still stops everything. `Finished::from_run`
reads the file into `StepResult`s (done, failed, interrupted, skipped, not
run), verifies the package steps that finished, and `describe()` adds "1 of 3
steps done" to the history. `history::active()` finds other HelixForge runs
still going from their markers, a preflight blocker.

`backups` finds configuration backups by name (`parse_name`: the original
and the time stamp) under `Roots`: the user's configuration and data
folders to any depth (skipping caches, repositories and links) and the home
folder, and `/etc`, `/boot` and two SDDM folders a few levels deep, which need
root to restore. `plan_restore()` builds a `Step::Shell` (a program HelixForge
composes from quoted paths only) that moves the current file aside as
`.helixforge-backup-<time>` and copies the backup back. `Snapshots` detects
Snapper with a root configuration or Timeshift; `worth_a_snapshot()` says
whether a plan changes the system (risk high, or PFM, K, D, SI flags) and
`with_snapshot()` puts a snapshot step first.

`Entry::identity()` gives what an entry installs, from its package action or,
for script and command entries, its `detect` table, so detection, status and
updates treat both alike; scripts still run when chosen. A command on the
PATH that none of the entry's packages explains is traced to its owning
package (`pacman -Qoq`, `dpkg -S`, `rpm -qf`), which then answers status and
updates.

`Inventory::plan_update()` turns entries with an update into `Step::Update`s:
one per entry through its own source (`flatpak update`, for the system or
the user installation that has it; `apt-get install --only-upgrade`, `dnf
upgrade`, `zypper update`), and on Arch one `sudo pacman -Syu` for all of them
with a note that pacman does not support updating one package alone (risk
high). Each step carries `UpdateTarget`s: the entry, the probe that finds its
package and its version before. The shell program fails if a target is gone
afterwards; `verify()` gathers a fresh inventory and passes a target only when
its version changed. Entries that are up to date, managed by Nix, installed
elsewhere, absent or scripts become notes. Removal and verification use
the same order. `Machine` carries the two questions so tests can answer
them. A source whose tools are missing is skipped; an
exit status of 128 or more ends the run. When the program succeeds,
`verify()` asks the same questions from Rust, and the `Finished` result,
outcome plus checks, is what the history records. A package step whose first
source cannot run does not stop the run from starting when another source
can. Native steps re-enter HelixForge through its hidden `native`
subcommand, so even Rust-implemented operations go through the same program
and the same terminal, and will through the graphical interface's terminal
panel too.

## Interface

A header with the name, tagline and a status badge (the banner picture
instead of the name and tagline where the terminal shows pictures:
`tui/banner.rs` asks it once at start and encodes the built-in copy for
Kitty's graphics or Sixel); a sidebar menu of
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
