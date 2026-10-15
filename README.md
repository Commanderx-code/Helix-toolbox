# ⬡ HELIXFORGE

**BUILD • CONFIGURE • OPTIMIZE**

HelixForge is a Linux setup menu for the terminal: a catalog of installers,
configurations and maintenance entries organised in tabs and groups, each run
inside the application after a confirmation, with a record of every run. It is
written in Rust and built so a graphical interface can share the same engine
later.

> Early development. The engine, catalog and interface are in place; the catalog
> itself is still small while the entries of the previous toolbox are brought
> across.

## Run

With a Rust toolchain (1.88 or newer):

```sh
cargo run -p helixforge              # the terminal interface
cargo run -p helixforge -- --help    # the command line
```

In the interface: `Tab` moves between tabs, `j`/`k` or the arrows move in the
list, `Enter` opens a group or runs an entry, `d` shows the description, `p`
the script or command, `/` searches every tab, `v` queues several entries,
`g` explains the flags column, `t` switches the theme and `q` quits. Each run
asks `[Y]` first and then runs in a terminal inside the window; `Ctrl-C` stops
it, `l` saves its output.

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

`catalog/tabs.toml` names the tabs. Each tab is a directory with a `tab.toml`;
inside it, entries are `<id>.toml` files and groups are directories with a
`group.toml`. An entry is one of three kinds:

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
[script]                         # a script beside the TOML file, run from its directory
path = "system-report.sh"
```

```toml
[command]                        # an inline shell command
line = "echo hello"
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
The catalog is embedded into the binary at build time and unpacked to a
temporary directory when it starts, so scripts run as real files.

## Layout

```text
apps/helixforge/            the command line and the terminal interface
crates/helixforge-core/     the data model: tabs, groups, entries, actions
crates/helixforge-catalog/  loading and validating the catalog tree
crates/helixforge-platform/ distribution and package manager detection
crates/helixforge-engine/   plans, the shell program, the pseudo-terminal, history
catalog/                    the entries, their scripts and the shared helpers
docs/                       architecture, roadmap, security notes, walkthrough
```

History lives in `~/.local/state/helixforge/history.jsonl` with saved output
under `logs/` beside it; `XDG_STATE_HOME` is honoured.

## Checks

```sh
scripts/verify.sh
```

runs the formatter, clippy, the tests, a release build and regenerates the
walkthrough. The GitHub workflow runs the same.

## License

MIT, see [LICENSE](LICENSE).
