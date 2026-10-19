# Architecture

## Stack

| Component | Choice |
| :--- | :--- |
| Language | Rust (edition 2024) |
| Terminal interface | Ratatui and Crossterm |
| Window | Tauri 2, Svelte 5 and xterm.js ([GUI.md](GUI.md)) |
| Graphical interface, later | Tauri 2 on the same crates |
| Command line | Clap |
| Interactive execution | portable-pty, rendered with tui-term and vt100 |
| Configuration and catalog | Serde and TOML |
| Errors | thiserror in the crates, anyhow in the application |
| Logging | tracing, to `~/.local/state/helix-toolbox/logs/helix-toolbox.log` |
| Tests | Cargo tests with fixtures; Linux VM runs for real installers, later |
| Releases | GitHub Actions, later |

There is no async runtime (Tokio). A run in a pseudo-terminal reports
itself as typed events on a bounded standard channel
(`helix_toolbox_engine::execution`, [EVENTS.md](EVENTS.md)), fed by threads of
its own; the terminal interface takes them on each pass of its event loop,
and a graphical interface will take the same events.

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
helix-toolbox-core      Tab, Group, Entry, Action, Precondition, Risk; name validation
helix-toolbox-catalog   reads catalog/ and scripts/ (embedded or from a directory), applies preconditions
helix-toolbox-platform  /etc/os-release, package manager family, PATH lookups
helix-toolbox-engine    plan() -> Plan of Steps; shell_program(); execution (pty runs as events); Runner (inherited, mock); history
helix-toolbox           clap command line, ratatui interface, tracing setup
helix-toolbox-gui       the window: Tauri 2 on the same crates, a workspace of its own under apps/helix-toolbox-gui
```

`--json` output is built in `apps/helix-toolbox/src/json.rs`, field by field
from the engine's types rather than derived from them, so the engine can
change without changing what other programs read; the shapes and their
`schema` version are in [JSON.md](JSON.md).

## The operation model

An entry's `Action` is one of:

| Action | Becomes |
| :--- | :--- |
| `Package` | its sources in order: `sudo <manager> install <package>`, the AUR helper on Arch, Flathub; or a note when it is already installed |
| `Script` | `cd <its directory> && <interpreter> <script>`; interactive by default |
| `Command` | `sh -e -c '<line>'` |
| `Nix` | `nix profile install <installable>` or `home-manager switch --flake <flake>` |
| `Native` | `helix-toolbox native <action>`: Helix Toolbox re-invoked, the operation implemented in Rust |
| `Profile` | the actions of the entries it names, in order |

`plan()` turns entries into a `Plan`: ordered `Step`s, notes for entries that
need nothing done, the executables the steps require, whether any step is
privileged, and the highest risk among them (from the flags: `D`, `K` and
`SI` are high; installs, services and anything privileged are medium).

## A run, end to end

```text
entries ──plan──▶ Plan ──shell_program──▶ sh program ──▶ execution::Execution (interface window)
                                                         │    pty::Session, ExecutionEvents on a channel
                                                         └─ Runner
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
`OnPath` (an install Helix Toolbox did not make, such as Nix's) or `Absent`
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
when it opens and after each run; `helix-toolbox status` prints it.

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

`confirmation::prepare()` is the step between a plan and its run for every
interface: it adds the snapshot where the run is worth one, runs the checks,
and returns `Prepared::Nothing` (the notes saying why), `Blocked` (the
blockers) or `Ready`, the plan with the choice after a failure applied and
the lines to confirm (risk and password, warnings, how entries were tested,
the steps). `overview::Overview` is the dashboard's facts about the machine,
and `settings` reads the run choices from `settings.toml` and keeps the
favorites in `favorites.toml`, so the interfaces share them.

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
steps done" to the history. `history::active()` finds other Helix Toolbox runs
still going from their markers, a preflight blocker.

`profiles` keeps the user's workstation profiles as TOML (`Profile`: name,
description, entry ids) in `$XDG_CONFIG_HOME/helix-toolbox/profiles/<slug>.toml`.
`check()` refuses an empty profile or one naming entries the catalog lacks;
`add_to()` puts every valid one into the Profiles tab as a "Your profiles"
group of `Action::Profile` entries with ids `profile-<slug>`, so planning,
queueing and running treat them like the catalog's own. `from_queue()` turns
the Install Plan into a profile, expanding profiles so none nests.

`doctor::examine()` runs the health checks through a `System` (reading
files, listing folders, links, a few read-only commands, a name lookup with a
time limit, free space, owners, running processes); `Here` is this machine
and `Fake` a made-up one for tests. Each check returns a `Diagnosis`: an id,
`Health` (good, notice, problem), a summary, the evidence, advice, and either
a `Repair` (commands of quoted words, run as a one-step plan with its own
confirmation) or a catalog entry that addresses it. The interface runs the
checks on a background thread when Helix Doctor opens and with `r`.

`backups` finds configuration backups by name (`parse_name`: the original
and the time stamp) under `Roots`: the user's configuration and data
folders to any depth (skipping caches, repositories and links) and the home
folder, and `/etc`, `/boot` and two SDDM folders a few levels deep, which need
root to restore. `plan_restore()` builds a `Step::Shell` (a program Helix Toolbox
composes from quoted paths only) that moves the current file aside as
`.helix-toolbox-backup-<time>` and copies the backup back. `Snapshots` detects
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
afterward; `verify()` gathers a fresh inventory and passes a target only when
its version changed. Entries that are up to date, managed by Nix, installed
elsewhere, absent or scripts become notes.

`updates::overview()` looks past the catalog, at every tool that keeps the
machine: a `Channel` each for the distribution's package manager, the AUR
helper, Flatpak's system and user installations, Home Manager with a flake,
NixOS and a Nix profile, with the `Pending` packages its tool reports (None
where it cannot say before updating, as with Nix, whose note gives the
locked nixpkgs date instead), how it was asked, the commands that update it
and a caveat. It asks through `doctor::System`, so tests use `doctor::Fake`;
offline it reads cached metadata only, and `online` adds read-only network
queries (`checkupdates`, `paru -Qua`, `flatpak remote-ls` without
`--cached`). `updates::plan()` makes one `Step::Update` per chosen channel,
with no targets to verify, the AUR depending on the repositories. Removal and verification use
the same order. `Machine` carries the two questions so tests can answer
them. A source whose tools are missing is skipped; an
exit status of 128 or more ends the run. When the program succeeds,
`verify()` asks the same questions from Rust, and the `Finished` result,
outcome plus checks, is what the history records. A package step whose first
source cannot run does not stop the run from starting when another source
can. Native steps re-enter Helix Toolbox through its hidden `native`
subcommand, so even Rust-implemented operations go through the same program
and the same terminal, and will through the graphical interface's terminal
panel too.

## Interface

The window (`apps/helix-toolbox-gui`, [GUI.md](GUI.md)) is a second
interface on the same engine: its Rust side answers the frontend's calls
with shapes built field by field in `views.rs`, plans and checks through
`confirmation::prepare()`, and forwards a run's execution events on a
channel to an xterm.js terminal. Its tool screens call the same engine
functions the terminal interface does (`doctor::examine`,
`updates::overview`, `drift::compare`, `hardware::gather`,
`desktop::detect` and the rest). It holds no catalog logic of its own.

A header with the name, tagline and a status badge (with the emblem picture
before a spaced-out name where the terminal shows pictures:
`tui/banner.rs` asks it once at start and encodes the built-in emblem for
Kitty's graphics or Sixel); a sidebar menu of
screens in two sections; the current screen; a hint line. The screens are
the Dashboard (system overview and quick actions), one screen per catalog
area (search bar and entry list with the flags column, groups opened in
place, an installed or updates filter), Favorites (pinned entries from
`favorites.toml` beside the settings, then recent ones from the history),
the Install Plan (the queue, planned live), Updates (the channels, gathered
on a background thread), History (the records,
with saved logs) and Settings. Popups over the current screen show text, the
confirmation with the plan's risk line, and the running terminal. All of it
is one `App` in `apps/helix-toolbox/src/tui/app.rs` driven by key events; screen
tests render it into a test backend. Search ranking is
`helix_toolbox_core::search::score`, shared with `helix-toolbox search`.

## Graphical interface, later

A Tauri crate uses the same engine: plans for its forms, an `Execution` and
its events for an embedded terminal panel where entries are interactive
([EVENTS.md](EVENTS.md)), and the history for its log view. Nothing in the
engine assumes a terminal.
