# Using HelixForge

HelixForge sets up and looks after a Linux machine from one menu in the
terminal. Everything it changes is planned first and shown to you, runs only
after you confirm it, and is recorded in History. This guide walks through
the menu and the common jobs; [INSTALL.md](INSTALL.md) covers installing it,
[REFERENCE.md](REFERENCE.md) every screen and command in full, and
[CATALOG.md](CATALOG.md) the catalog format and how runs are planned.

## Starting

Run `helixforge` as your normal user, not with `sudo`: entries that change
the system ask for your password inside the run, when they need it. The
terminal should be at least 100 columns by 25 rows; a Nerd Font shows the
icons (Settings turns them off otherwise). In Ghostty, Kitty, WezTerm, foot
and other terminals that show pictures, the header is the HelixForge banner.
`t` cycles the themes: solid, transparent, compatible (sixteen colours) and
monochrome (no colour, the theme `NO_COLOR` selects for a run).

## Finding your way

The sidebar on the left lists the screens. `Tab` and `Shift-Tab` move
between them from anywhere; in the sidebar, `j`/`k` or the arrows move and
`Enter` opens the screen. `h` or `←` comes back to the sidebar. `?` shows
every key, `/` searches every area, `t` changes the theme and `q` quits.
`Ctrl-K` opens the command palette: type what you want, a screen (`updates`),
an action (`dns`, `check updates online`) or an entry (`install ghostty`),
and press `Enter`.

| Screen | What it is for |
|---|---|
| Dashboard | The machine at a glance, and quick actions |
| Applications | The application catalog, in groups |
| Favorites | Entries you pinned with `*`, then the ones you ran recently |
| Install Plan | What you queued, checked and ready to run together |
| Configurations | Shell, dotfiles and desktop configuration |
| Helix Doctor | Health checks that only read the system, with repairs |
| Hardware | Firmware updates, disks, temperatures, batteries, devices |
| Maintenance, Security | Clean-ups, system tweaks and hardening |
| Profiles | Ready-made sets of entries, and your own |
| Drift | How the machine differs from a saved baseline or a profile |
| Reliability | How far each entry has been tested, critical ones first |
| Updates | What every package tool has waiting |
| History | Every run, its result and its saved log |
| Backups | Configuration backups entries left, to restore |
| Settings | Theme, icons, confirmation, hidden entries, keep going, snapshots |

## Installing applications

1. Open **Applications** and a group, or press `/` and type part of a name
   (loose spellings work: `rpgrp` finds Ripgrep). `Escape` clears the search
   and goes back to where you were.
2. The right of each row says how the entry stands: **Installed** with where
   from and the version, **Update** when a newer version is waiting,
   **Managed** when Nix or Home Manager provides it, or **Not installed**.
   The letters after it are its flags; `g` explains them.
3. `d` describes the entry, `p` shows exactly what it will run.
4. `v` shows where it comes from and what checks it: the trust inspector
   (below).
5. `Space` queues it in the **Install Plan**; queue as many as you like.
   `Enter` runs one entry straight away instead.
6. On the Install Plan, check the plan: the steps in order, the risk, whether
   it asks for your password, and anything that blocks it (a missing tool,
   too little space, a conflicting package). `e` shows what it is expected
   to change: configuration files side by side where they can be worked
   out, the packages that come with it, services, and what cannot be known
   before it runs. `Enter` runs it.
7. The run asks `[Y]` first, then shows its output in a window inside the
   menu. Type into it when an installer asks a question. `Ctrl-C` stops it;
   `l` saves the log.
8. Afterwards HelixForge checks each package with the package manager and
   says what it verified. Anything that did not finish stays in the Install
   Plan to run again.

`i` in a list cycles between all entries, installed ones and ones with an
update. `*` pins an entry to **Favorites**. `r` removes an installed package
entry that offers removal; `u` updates the selected entry and `U` every entry
with an update.

## Profiles

A profile is a set of entries that run as one plan, in order, with what they
depend on first. Profiles lists the built-in ones. To make your own, queue
what a machine needs (applications, the Myfish shell, dotfiles
configurations, tweaks) and press `s` on the Install Plan to name it. It is
saved in `~/.config/helixforge/profiles/` and appears under Profiles, **Your
profiles**. `helixforge profile export` and `import` move it to another
machine.

## How far to trust an entry

**Reliability** shows how far each entry has been tested: **Container
Tested** (installed and verified in distribution containers by the
Distributions workflow), **Stand-in Tested** (its installer run against
stand-in commands), **VM Tested** or **Hardware Tested** (reported from a
virtual or real machine), or **Not Yet Validated**. Entries that could stop a
machine booting, logging in or showing a display come first, marked `!`;
those are the ones to have a snapshot or a live USB ready for. If you run
one successfully, reporting it helps everyone ([REPORTING.md](REPORTING.md)).

`v` on an entry opens the **trust inspector**: where its installer comes
from, its checksum and licence, what checks the packages it installs (pacman
or apt signatures, Flathub), what it downloads itself and whether anything
checks that, and what it does as root. Signed release provenance, package
integrity checks and unverified third-party downloads are listed apart.

## Moving to a new machine

**Drift** compares a machine with a **baseline**, a record of how a machine
was set up. On the machine you have set up, open Drift and press `s` to save
one (or run `helixforge drift save "My laptop"`). Later, Drift lists what
has moved: applications that are missing, configuration files that are gone
or changed, a different login shell or display manager, entries HelixForge
ran there and not since.

To bring a fresh installation up to the old one, export the baseline there
(`helixforge drift export "My laptop"`), copy the file over, and on the new
machine run `helixforge drift check my-laptop.toml`, or import it and open
Drift. `a` puts everything that restores the missing parts in the Install
Plan; nothing runs until you review and confirm it there. `b` compares with
a profile instead.

## Keeping the machine up to date

**Updates** shows each tool that keeps the machine: pacman or apt, dnf or
zypper; the AUR helper; Flatpak; Home Manager, NixOS or a Nix profile. It
first shows what those tools already know, without going online; `r` checks
online. `Enter` lists the packages, `u` updates the selected tool and `U`
every tool with updates waiting. HelixForge runs each tool's own update
command; on Arch that is the whole system, as Arch requires. Nix cannot count
its updates beforehand, so its row shows how old its locked packages are, and
updates with `u`.

The **HelixForge** row updates HelixForge itself, the way it was installed:
the new package for your distribution (downloaded and checked first), `cargo
install` again, or a fresh build of your checkout. Copies run through the
launcher or `start.sh` are always the latest release already.

## The hardware

**Hardware** lists firmware updates fwupd knows of, the disks with their
temperatures, the sensors, battery health and the PCI and USB devices, all
read without changing anything. `f` updates a device's firmware after you
type FIRMWARE and confirm its plan; keep a laptop on mains power while it
runs. `s` checks a disk's SMART health, which asks for your password.

## When something is wrong

**Helix Doctor** checks the network and name resolution, failed services,
the graphics drivers, the package database, home folder permissions, boot
images, GRUB, the EFI partition and disk space. It only reads. `Enter` shows
the evidence; where HelixForge can repair a problem, `f` shows the repair,
which runs after its own confirmation.

**Backups** lists the configuration backups entries left, newest first.
`Enter` restores one; the current file is kept as a backup first, so a
restore can be undone the same way. With Snapper or Timeshift set up,
HelixForge can take a snapshot before a run that changes system files, the
kernel or disks (Settings, or `--snapshot` on the command line). It never
promises to roll back packages itself.

**History** lists every run with its result; `Enter` opens its saved log.

To report a problem, run `helixforge report` and paste its output into an
issue; [REPORTING.md](REPORTING.md) says what helps, and
[STABILITY.md](STABILITY.md) what stays the same across 1.x releases.

## The command line

Everything the menu does is also a command, for scripts and for machines
without a full terminal: `helixforge --help` lists them, and each has its own
`--help` and manual page (`man helixforge`). The commands that read take
`--json` for other programs ([JSON.md](JSON.md)). A few to start with:

```sh
helixforge search editor          # find entries
helixforge status                 # what is installed, updated or managed
helixforge run git ripgrep --dry-run
helixforge updates --online       # what every package tool has waiting
helixforge doctor                 # the health checks
helixforge report                 # for a bug report
```

## Your files

| Path | What |
|---|---|
| `~/.config/helixforge/settings.toml` | Theme, icons and the other settings |
| `~/.config/helixforge/favourites.toml` | Pinned entries |
| `~/.config/helixforge/profiles/` | Your profiles |
| `~/.config/helixforge/baselines/` | Your baselines, for Drift |
| `~/.local/state/helixforge/` | History and saved logs |

Removing HelixForge leaves them; delete them to start afresh.
