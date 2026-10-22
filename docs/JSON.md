# JSON output

`--json` makes a command print one JSON document on standard output instead
of text, for scripts, other tools and the graphical interface to come. It
works with the commands that read (`system`, `list`, `search`, `info`,
`status`, `plan`, `history`, `updates`, `doctor`, `backups`, `drift check`,
`drift list`, `reliability`, `trust`, `hardware`, `setup`, `desktop`,
`bundle verify`, `bundle create --dry-run`, `profile list`)
and with `--dry-run` plans (`run`, `remove`, `update`, `updates --apply`,
`restore`, `doctor --repair`). Anything that would change the machine is
refused with `--json` unless it is a dry run: a script cannot start a run by
asking for JSON. The menu, `docgen`, and the profile commands that write
files refuse it too.

Errors go to standard error as text, with exit status 1 and nothing on
standard output.

```sh
helix-toolbox status --json | jq -r '.data.entries[] | select(.status.state == "update") | .id'
helix-toolbox updates --online --json | jq '.data.channels[] | {id, count}'
helix-toolbox run git ripgrep --dry-run --json | jq '.data.runnable'
```

## The envelope

```json
{
  "schema": 1,
  "helixforge": "1.7.0",
  "helix_toolbox": "1.7.0",
  "command": "status",
  "data": { }
}
```

`helix_toolbox` is the program's version, from 1.7. `helixforge` is the
same version under the program's name before 1.7, kept so schema 1 readers
still find it. The Updates channel for the program itself keeps the id
`helixforge` for the same reason.

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

With `--effects`, a plan also has `effects`: for each step `{id, name,
changes}`, each change `{certainty, area, text, diff}`. `certainty` is
`known`, `expected` or `unknown`; `area` is `file`, `package`, `service`,
`boot` or `other`; `diff` is `null` or `{path, changed, rows}`, each row
`{before, after, tag}` with `tag` being `same`, `changed`, `removed` or
`added`.

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
commands}` or `null`, run with `helix-toolbox doctor --repair <id>`.

## backups

`{snapshots, backups}`: `snapshots` is `{tool, restore}` when Snapper or
Timeshift is set up, else `null`; each backup is `{original, backup, taken,
system, directory}`.

## drift

`helix-toolbox drift check --json` prints `{against, baseline, profile,
matched, differences, unknown, restore}`. `against` is `baseline` or
`profile`; `baseline` is `{name, taken, system}` (`taken` in Unix seconds)
or `null`, and `profile` the profile's id or `null`. Each difference is
`{kind, label, subject, was, now, entry, note}`:

| `kind` | |
|---|---|
| `missing` | an application the baseline had, or the profile names, is not installed |
| `file-missing` | a configuration file or folder is gone |
| `not-applied` | an entry ran on the baseline machine and has no successful run here |
| `file-replaced` | a path is another kind of thing now, such as a file that became a link |
| `file-changed` | a file or folder has other contents |
| `setting` | the login shell, display manager, repositories, Flatpak remotes or desktop differ |
| `added` | installed here and not in the baseline |
| `version` | installed in both, at another version or from another source |

`entry` is the entry that installs or writes it, or `null`. `matched` counts
what is the same, `unknown` lists ids this catalog does not have, and
`restore` the entries that would put back what is missing and can run on this
machine, in order, for `helix-toolbox run`.

## baselines

`helix-toolbox drift list --json`: `{baselines: [{id, name, taken, system,
entries, files, settings, applied}]}`, newest first; the last four are
counts, `id` the name `drift check` takes.

## reliability

`{record, entries: [{id, name, assessment}]}`. `record` is `{run, url,
commit, finished, distributions}` for the Distributions run a release build
carries, or `null`. An assessment is `{level, label, systems, verified,
evidence, ran_here, critical}`: `level` is `hardware-tested`, `vm-tested`,
`container-tested`, `stand-in-tested` or `not-yet-validated`; `systems` are
the containers or reported systems; `verified` the last date (`YYYY-MM-DD`)
or `null`; `ran_here` the last successful run on this machine in Unix
seconds, or `null`; `critical` whether failing could stop the machine
booting, logging in or showing a display, or it changes disks or the whole
system.

## trust

`{id, name, kind, origin, file, sha256, installer_license,
package_license, provenance, integrity, downloads, privileges, validation}`.
`file` is the installer script, relative to the scripts folder, and
`sha256` and `installer_license` are its checksum and license;
`package_license` is the license of the software it installs, as the package
manager reports it, or `null`. Each of
`provenance`, `integrity` and `downloads` is a list of `{standing, label,
subject, text}`, `standing` being `provenance` (signed build provenance),
`checked` (a checksum or signature a tool checks), `pinned` (fixed to a
revision by its hash), `unverified` or `unknown`. `validation` is an
assessment as above.

## hardware

`{firmware, disks, sensors, batteries, pci, usb}`. `firmware` is
`{available, devices, updates, note}`: devices `{id, name, vendor, version,
updatable}`, updates `{device_id, device, from, to, summary, urgency}`.
A disk is `{name, model, bytes, kind, firmware, celsius}` (`kind` `NVMe`,
`SSD`, `HDD` or `USB`); a sensor `{chip, label, celsius, high, critical}`; a
battery `{name, status, capacity, health, cycles, model, technology}`
(`capacity` and `health` in percent); a device `{address, class, name,
driver}`. Unknown values are `null`.

## setup

`{machine, workstations, chosen, suggestions}`. `machine` is
`{distribution, family, package_manager, tools, desktop, desktop_name, gpus,
laptop}`; each workstation `{id, name, description, entries, when}`;
`chosen` the workstation named, or `null`; each suggestion `{id, name,
available, note}`.

## desktop

`{desktop, entries}`. `desktop` is `{kind, name, session, display_manager,
appearance, owners, blocked}`: `kind` is `kde`, `gnome`, `xfce`, `cinnamon`,
`mate`, `hyprland`, `sway`, `other` or `null`; `display_manager` is `{name,
files, theme}` or `null`; each appearance value `{key, label, value}`;
`owners` says what else sets the look; `blocked` why Helix Toolbox would not
change it, or `null`. `entries` are the suggestions for this desktop.

## bundle

`bundle create --dry-run --json`: `{entries, downloads, dnf, flatpaks,
complete}`, each entry `{id, name, offline, packages, network, flatpak}`
(`flatpak` and `network` only when they apply), each download `{url, file,
checksum}`, each Flatpak `{id, reference, commit, remote, url, collection,
user}`. `bundle verify --json`: the manifest, `{format, created, helix-toolbox,
release, distribution, family, architecture, complete, entries, files,
flatpak}` (`flatpak`, the Flatpaks in the same form, only in format 2), each
file `{path, sha256, bytes, source, assurance}`.

## verify

`{passed, entries: [{id, name, checks}]}`: `passed` is false when a check
failed (and the command exits 1). A check is `{name, label, passed, unsure,
later, detail}`, `label` being `Verified`, `Not verified`, `Not checked`
(`unsure`: it could not be checked here) or `After the reboot` (`later`: it
holds only once the machine has started since the entry's last run).

## services

`services list`: `{units: [{name, description, enablement, active, sub,
user}], session}`. `enablement` is the unit file's state (`enabled`,
`disabled`, `static`, `masked`, `indirect`…) or `null` for a unit without a
file of its own, `active` and `sub` how it runs or `null` when it is not
loaded, `user` whether it is the session's; `session` whether the user's
own systemd answered. `services show <unit>` is `service`: `{unit,
needed_by, consequence}`, `consequence` what stopping or disabling it costs
or `null`.

## startup

`{applications: [{file, name, comment, exec, enabled, here, system, own}]}`:
`here` whether it applies to this desktop (`OnlyShowIn`, `NotShowIn`),
`system` whether it comes from the system's folder, `own` the user's file
when there is one.

## storage

`{filesystems: [{path, free, total}], items: [{id, title, detail, size,
frees, packages, not_offered}], kept: [{title, detail, size}]}`, sizes in
bytes, `null` when unknown; `frees` is `null` when only the cleanup can
tell, `not_offered` why an item cannot be cleaned now, `packages` what
unused packages would remove.

## hardware_report

`report --worked <entry>`: `{entries: [{id, name}], machine, system,
version, date, records, toml, issue}`. `machine` is `{distribution,
desktop, model, graphics, kernel, architecture, virtualized}` (`graphics` a
list of cards with their drivers; unknown values `null`), and `system` the
line made from it. `records` are the `[[hardware]]` records it asks for,
`{entry, system, version, date, note}` (`note` left out when empty), and
`toml` the same as TOML; `issue` is the link to the new issue, filled in.

## profile list

`{profiles: [{id, name, description, entries}]}`, `id` being what `run`
takes (`profile-dev-box`).
