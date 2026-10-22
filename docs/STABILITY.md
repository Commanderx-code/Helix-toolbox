# What stays the same in 1.x

Helix Toolbox follows [semantic versioning](https://semver.org) from v1.0.0.
Within 1.x, what is listed here as stable keeps working the way it does in
1.0; anything that would break it waits for 2.0. This page is that promise.

## Version numbers

- **Patch** (1.0.1): fixes, catalog updates and the sources adopted from the
  dotfiles and Myfish repositories.
- **Minor** (1.1.0): new commands, options, entries, screens, JSON fields and
  settings.
- **Major** (2.0.0): anything below that changes or goes away.

## Stable

**Commands and options.** Every command and option `helix-toolbox --help` and
its manual pages list keeps its name and meaning. The exit status is 0 when
the command did what it was asked and 1 when it did not; a run that was
interrupted or failed is 1. `--dry-run` never changes the machine, and
`--json` never runs anything.

**The name.** HelixForge became Helix Toolbox in 1.7, with the command
`helix-toolbox` (and `htb`). The packages keep `helixforge` as a link to it
for the rest of 1.x, the folders moved with a link at each old name, and
what other programs read keeps its old name; [MIGRATION.md](MIGRATION.md)
lists each.

**JSON output.** The shapes in [JSON.md](JSON.md), at `"schema": 1`, only
gain fields: none is removed, renamed or given another meaning. A change that
would break them raises `schema` and comes with a major version.

**Your files.** In `~/.config/helix-toolbox` and
`~/.local/state/helix-toolbox` (`helixforge` before 1.7): `settings.toml`,
`favorites.toml` (`favourites.toml` before 1.5.9), profiles in `profiles/`, baselines in `baselines/` (from 1.1), looks in `looks/` (from
1.4) and `history.jsonl`
written by any 1.x release are read by every later 1.x release. Settings a release does not know are ignored rather than
discarding the rest, so going back to an earlier 1.x keeps what it
understands. A profile or baseline exported from one machine imports on another
running the same or a later 1.x.

`apps/helix-toolbox/tests/contracts.rs` checks the two promises above on
every change: each `--json` command's shape against the one saved when it
was added, and files written by 1.0, 1.1 and 1.4.

**Entry ids.** An entry's id (`ripgrep`, `pacman-config`,
`profile-dev-box`) does not change. An entry can be removed only when what it
installs no longer exists or can no longer be installed safely; the changelog
says so, and a profile naming it says which entry is gone instead of failing
silently.

**What a run asks first.** Nothing changes the machine without showing the
plan and asking, unless you skip the question yourself (`--yes`, or the
confirmation setting). Package-manager steps run one at a time.

## Not part of the promise

- **Text written for people**: the menu's layout, wording, colors and
  keys may improve, and so may the command line's text output and
  `helix-toolbox report`. Scripts should use `--json`.
- **The catalog's arrangement**: groups, order, descriptions and which
  entries show on which distribution.
- **Hidden commands and options** (`native`, `manpages`, `--catalog-dir`),
  which exist for Helix Toolbox itself and its tests.
- **What the entries install**: they follow the software they install,
  whose own versions and options change.
- **The Rust crates** inside the repository, which are not published as
  libraries.

## Supported systems

Linux on x86_64 or aarch64 (ARM64) with glibc 2.35 or newer. Every release
is tested in containers of Arch, Debian stable, Ubuntu 22.04 and 24.04, Fedora
and openSUSE Tumbleweed, and on ARM64 machines in the same containers, with
Arch Linux ARM for Arch; derivatives of those (Garuda, CachyOS, EndeavourOS, Manjaro,
Mint, Pop!_OS and others) use the same package managers and are expected to
work. Other systems may run it with fewer entries shown.

Fixes, security fixes included, go into the latest release only: update to
get them.
