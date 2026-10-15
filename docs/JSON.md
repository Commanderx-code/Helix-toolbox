# JSON output

`--json` makes a command print one JSON document on standard output instead
of text, for scripts, other tools and the graphical interface to come. It
works with the commands that read (`system`, `list`, `search`, `info`,
`status`, `plan`, `history`, `updates`, `doctor`, `backups`, `profile list`)
and with `--dry-run` plans (`run`, `remove`, `update`, `updates --apply`,
`restore`, `doctor --repair`). Anything that would change the machine is
refused with `--json` unless it is a dry run: a script cannot start a run by
asking for JSON. The menu, `docgen`, and the profile commands that write
files refuse it too.

Errors go to standard error as text, with exit status 1 and nothing on
standard output.

```sh
helixforge status --json | jq -r '.data.entries[] | select(.status.state == "update") | .id'
helixforge updates --online --json | jq '.data.channels[] | {id, count}'
helixforge run git ripgrep --dry-run --json | jq '.data.runnable'
```

## The envelope

```json
{
  "schema": 1,
  "helixforge": "0.6.0",
  "command": "status",
  "data": { }
}
```

`schema` is the version of these shapes. Within one schema version fields are
only added, never removed or changed in meaning; a field you do not know can
be ignored. `command` names the shape of `data`: `run --dry-run` and the
other dry runs print a `plan`. Text is passed on as the tools reported it;
JSON escapes control characters, so nothing in a document reaches a terminal
as a control sequence. Optional values are `null` rather than missing.

## system

| Field | |
|---|---|
| `distribution` | `{id, name, id_like}` from `/etc/os-release` |
| `package_manager` | `pacman`, `apt`, `dnf`, `zypper`, … or `null` |
| `kernel`, `architecture` | |
| `tools` | `{sudo, nix, home-manager, flatpak}`: whether each is on the PATH |

## list, search, info

An entry is `{id, name, description, flags, kind}`, `kind` being `package`,
`script`, `command`, `nix`, `native` or `profile`.

- `list`: `{tabs: [{id, name, items}]}`; an item is an entry with
  `"type": "entry"`, or `{"type": "group", name, items}`.
- `search`: `{query, results}`, entries best match first, each with its
  `score`. No match is an empty list, not an error.
- `info`: the entry with `action` (for a package: `binary`, `packages` by
  family, `aur`, `flatpak`, `conflicts`, `removable`; for a script: `file`,
  `interactive`; for a command: `line`; for Nix: `nix` and its
  `installable` or `flake`; for native: `native`; for a profile: `entries`)
  and `status` as below, `null` for entries that are not packages.

## status

`{entries: [{id, name, status}]}`. `status` is `null` for an entry that is
not a package, else `state` and the fields for it, with the `label` and
`detail` the menu shows:

| `state` | Fields |
|---|---|
| `installed` | `source`, `version`, `shadowed_by` (a Nix copy found first on the PATH) |
| `update` | the same and `latest` |
| `managed` | `by` (`Home Manager`, `NixOS`, `Nix`), `version` |
| `elsewhere` | `path` |
| `absent` | `replaces`: installed packages it conflicts with |

## plan

| Field | |
|---|---|
| `steps` | `[{index, id, name, command, fallbacks, depends_on}]`, in run order; `depends_on` lists earlier step indexes |
| `notes` | what needs nothing done, and why |
| `blocked` | entries left out |
| `requirements` | commands the steps need |
| `privileged` | whether it asks for a password |
| `risk` | `low`, `medium` or `high` |
| `keep_going` | whether a failure skips only what depends on it |
| `findings` | `[{severity, message}]` from the checks before a run, `severity` `blocker` or `warning` |
| `runnable` | there are steps and no blockers |

## history

`{runs: [{unix_timestamp, entries, names, success, result, log}]}`, newest
first; `result` is the exit status, `interrupted` or why the run could not
start, and `log` a saved log's path or `null`.

## updates

`{online, channels: [{id, name, count, pending, note, commands, caveat}]}`.
`count` and `pending` (`[{name, from, to}]`) are `null` where the tool
cannot say before it updates (Nix) or only says online (the AUR); `0` and `[]`
mean nothing waits. `commands` are what updating runs, as argument lists.

## doctor

`{checks: [{id, title, health, summary, evidence, advice, repair, entry}]}`,
`health` being `good`, `notice` or `problem`; `repair` is `{label,
commands}` or `null`, run with `helixforge doctor --repair <id>`.

## backups

`{snapshots, backups}`: `snapshots` is `{tool, restore}` when Snapper or
Timeshift is set up, else `null`; each backup is `{original, backup, taken,
system, directory}`.

## profile list

`{profiles: [{id, name, description, entries}]}`, `id` being what `run`
takes (`profile-dev-box`).
