# ⬡ HELIXFORGE

**BUILD • CONFIGURE • OPTIMIZE**

HelixForge is a Linux setup menu for the terminal: a catalog of installers,
configurations and maintenance entries organised in tabs and groups, each run
inside the application after a confirmation, with a record of every run. It is
written in Rust and built so a graphical interface can share the same engine
later.

> Milestone 1 is complete: the engine, catalog, interface and command line
> are in place and tested. The catalog is still small while the entries of
> the previous toolbox are brought across; see `docs/ROADMAP.md`.

## Run

With a Rust toolchain (1.88 or newer):

```sh
cargo run -p helixforge              # the terminal interface
cargo run -p helixforge -- --help    # the command line
```

In the interface the sidebar is a menu of screens. `Tab` or `j`/`k` in the
sidebar move between Dashboard, Applications, Install Plan, and the other
areas, History and Settings; `Enter` or `l` opens the selected screen and
`h` returns to the sidebar. In an area, `j`/`k` or the arrows move in the
list, `Enter` opens a group or runs an entry, `Space` adds it to the install
plan, `d` shows the description, `p` the script or command, `g` explains the
flags column and `/` searches every area. The Install Plan shows what is
queued with its risk and requirements; `Enter` runs it. Each run asks `[Y]`
first and then runs in a terminal inside the window; `Ctrl-C` stops it, `l`
saves its output, and History lists every run. `?` shows all keys and `q`
quits.

From the command line:

```sh
helixforge system                     # the distribution and package manager it sees
helixforge list                       # the catalog as tabs, groups and entries
helixforge info git                   # one entry
helixforge plan git system-report     # what running entries would do
helixforge run git --dry-run          # the plan, nothing changed
helixforge run git                    # runs after you type RUN
helixforge history                    # recent runs
helixforge docgen docs/walkthrough.md # the catalog as Markdown
```

## The catalog

`catalog/tabs.toml` names the areas: Applications, Configurations, Helix
Doctor, Maintenance, Security and Profiles. Each is a directory with a
`tab.toml`; inside it, entries are `<id>.toml` files and groups are
directories with a `group.toml`. An entry is one of six kinds:

```toml
name = "Git"
description = "Distributed version control."
flags = "I"                      # what it does; g in the interface explains the codes

[package]                        # one native package per family
binary = "git"                   # installed when this is on the PATH
arch = "git"
debian = "git"
fedora = "git"
```

```toml
[script]                         # scripts/installers/system-report.sh, run from its directory
id = "system-report"
interactive = true               # the default; false for scripts that never prompt
```

```toml
[script]                         # or a script beside the entry's TOML file
path = "setup.sh"
```

```toml
[command]                        # an inline shell command
line = "echo hello"
```

```toml
[nix]                            # one of the two
profile_install = "nixpkgs#ripgrep"
home_manager_switch = "~/dotfiles/home-manager#commander"
```

```toml
[native]                         # implemented in Rust by the engine
action = "system-report"
```

```toml
[profile]                        # other entries, as one reviewable plan
entries = ["git", "ripgrep", "fzf", "neovim"]
```

Entries and groups can carry `order` to sort before their name, `multi_select
= false` to keep them out of the queue, and `preconditions` that hide them on
machines they do not apply to:

```toml
[[preconditions]]
matches = true
check = "command_exists"         # or "file_exists", { containing_file = "..." }, { environment = "..." }
values = ["sddm"]
```

Scripts source the shared helpers with `. "$HELIXFORGE_LIB/common-script.sh"`.
The catalog and scripts are embedded into the binary at build time and
unpacked to a temporary directory when it starts, so scripts run as real
files.

## Plans

Running entries first makes a plan: the steps in order, what they need,
whether any asks for your password, and the risk from the flags. The
interface shows it in the confirmation; `helixforge plan` and `helixforge run
--dry-run` print it. Every run is recorded with its outcome, including runs
that failed or were interrupted.

## Layout

```text
apps/helixforge/            the command line and the terminal interface
crates/helixforge-core/     the data model: tabs, groups, entries, actions, risk
crates/helixforge-catalog/  loading and validating the catalog and scripts
crates/helixforge-platform/ distribution and package manager detection
crates/helixforge-engine/   plans, the shell program, runners, the pseudo-terminal, history
catalog/                    the entries, one TOML file each, in six areas
scripts/shared/             the shell helpers scripts source; scripts/installers/ the scripts by id
tests/fixtures/             a small catalog for the end-to-end tests
docs/                       architecture, roadmap, security, migration matrix, walkthrough
```

History lives in `~/.local/state/helixforge/history.jsonl` with saved output
and the tracing log under `logs/` beside it; `XDG_STATE_HOME` is honoured and
`HELIXFORGE_LOG` sets the log level.

The previous toolbox stays the stable way to run the entries that have not
come across yet; [docs/MIGRATION.md](docs/MIGRATION.md) tracks each one.

## Checks

```sh
scripts/verify.sh
```

runs the formatter, clippy, the tests, a release build and regenerates the
walkthrough. The GitHub workflow runs the same.

## License

MIT, see [LICENSE](LICENSE).
