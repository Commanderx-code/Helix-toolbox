# Using Helix Toolbox

Helix Toolbox sets up and looks after a Linux machine from one menu in the
terminal. Everything it changes is planned first and shown to you, runs only
after you confirm it, and is recorded in History. This guide walks through
the menu and the common jobs; [INSTALL.md](INSTALL.md) covers installing it,
[REFERENCE.md](REFERENCE.md) every screen and command in full, and
[CATALOG.md](CATALOG.md) the catalog format and how runs are planned.

## Starting

Run `helix-toolbox` as your normal user, not with `sudo`: entries that change
the system ask for your password inside the run, when they need it. The
terminal should be at least 100 columns by 25 rows; a Nerd Font shows the
icons (Settings turns them off otherwise). In Ghostty, Kitty, WezTerm, foot
and other terminals that show pictures, the header shows the Helix Toolbox
emblem beside the name.
`t` cycles the themes: solid, transparent, compatible (sixteen colors) and
monochrome (no color, the theme `NO_COLOR` selects for a run). The same
catalog and screens open in a window too, `helix-toolbox-gui`
([GUI.md](GUI.md)).

## The first start

The first time it starts, Helix Toolbox offers to set the machine up: it shows
what it found (distribution, desktop, graphics, package tools), asks which
kind of workstation this is (Developer, Gaming, Privacy, Minimal or Custom)
and lists what that would add, ticked where it is not installed yet. `Enter`
puts the ticked entries in the Install Plan to review; nothing runs until you
confirm it there. `Esc` skips it; Settings, **Setup wizard**, opens it later.

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
| Desktop | The desktop, its login screen and look, saved looks |
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
8. Afterward Helix Toolbox checks each package with the package manager,
   and for a script what it installs and what it was to set up (the theme
   a setting selects is installed, a service is running, a word is on the
   kernel command line), and says what it verified. What only shows after a
   reboot is checked after it, in Helix Doctor. Anything that did not
   finish stays in the Install Plan to run again.

`i` in a list cycles between all entries, installed ones and ones with an
update. `*` pins an entry to **Favorites**. `r` removes an installed package
entry that offers removal; `u` updates the selected entry and `U` every entry
with an update. `c` checks an entry now, without changing anything: whether
it is still as it left the machine (**Check it now** in the window,
`helix-toolbox verify <entry>` on the command line).

## Profiles

A profile is a set of entries that run as one plan, in order, with what they
depend on first. Profiles lists the built-in ones. To make your own, queue
what a machine needs (applications, the Myfish shell, dotfiles
configurations, tweaks) and press `s` on the Install Plan to name it. It is
saved in `~/.config/helix-toolbox/profiles/` and appears under Profiles, **Your
profiles**. There `x` exports it to a file to take to another machine, where
`o` in Profiles imports it, and `Delete` deletes it. `helix-toolbox profile
export` and `import` do the same on the command line.

## How far to trust an entry

**Reliability** shows how far each entry has been tested: **Container
Tested** (installed and verified in distribution containers by the
Distributions workflow), **Stand-in Tested** (its installer run against
stand-in commands), **VM Tested** or **Hardware Tested** (reported from a
virtual or real machine), or **Not Yet Validated**. Entries that could stop a
machine booting, logging in or showing a display come first, marked `!`;
those are the ones to have a snapshot or a live USB ready for. If you run
one successfully, reporting it helps everyone: `w` on the entry, or on its
run in History, opens the report, and `o` there opens the issue in your
browser, filled in ([REPORTING.md](REPORTING.md)).

`v` on an entry opens the **trust inspector**: where its installer comes
from, its checksum and license, what checks the packages it installs (pacman
or apt signatures, Flathub), what it downloads itself and whether anything
checks that, and what it does as root. Signed release provenance, package
integrity checks and unverified third-party downloads are listed apart.

## Moving to a new machine

**Drift** compares a machine with a **baseline**, a record of how a machine
was set up. On the machine you have set up, open Drift and press `s` to save
one (or run `helix-toolbox drift save "My laptop"`). Later, Drift lists what
has moved: applications that are missing, configuration files that are gone
or changed, a different login shell or display manager, entries Helix Toolbox
ran there and not since.

To bring a fresh installation up to the old one, export the baseline there
(`helix-toolbox drift export "My laptop"`), copy the file over, and on the new
machine run `helix-toolbox drift check my-laptop.toml`, or import it and open
Drift. `a` puts everything that restores the missing parts in the Install
Plan; nothing runs until you review and confirm it there. `b` compares with
a profile instead.

## Keeping the machine up to date

**Updates** shows each tool that keeps the machine: pacman or apt, dnf or
zypper; the AUR helper; Flatpak; Home Manager, NixOS or a Nix profile. It
first shows what those tools already know, without going online; `r` checks
online. `Enter` lists the packages, `u` updates the selected tool and `U`
every tool with updates waiting. Helix Toolbox runs each tool's own update
command; on Arch that is the whole system, as Arch requires. Nix cannot count
its updates beforehand, so its row shows how old its locked packages are, and
updates with `u`.

The **Helix Toolbox** row updates Helix Toolbox itself, the way it was installed:
the new package for your distribution (downloaded and checked first, or on
Arch with the repository, with the system), `cargo install` again, a fresh
build of your checkout, or, for a copy of the program you put somewhere
yourself, the new program in its place. The window's package is updated
with it, and the window's AppImage replaced where it is. Copies run through
the launcher or `start.sh` are always the latest release already.

To be told when updates are waiting, turn on **Tell me when updates are
waiting** in Settings, in the window or the menu, or run `helix-toolbox
notifications on`. A timer of your own (systemd's, in
`~/.config/systemd/user`) then checks soon after you log in and every 6
hours, online the way `r` does, and shows a desktop notification when
something new is waiting; it tells of the same updates once. Its **Show
updates** button opens the window on Updates. It never updates anything
itself. `helix-toolbox notifications off`, or the same setting, removes the
timer.

The same check can also tell you when Helix Doctor finds a new problem
(**Tell me of new health problems**, `helix-toolbox notifications health
on`) or when the machine has changed since a saved Drift baseline (**Tell me
when it drifts from a baseline**, `helix-toolbox notifications drift
<baseline>`). Each problem or change is told once, and nothing is repaired
or put back by itself.

## The desktop

**Desktop** shows the desktop and its login screen, the theme, icons,
cursor and font in use, and the entries made for this desktop. `s` saves
the look under a name, and `Enter` on it later puts it back after a
confirmation. If Home Manager or Stylix sets the desktop's look, the screen
says so: their next switch may undo a change made here.

## The hardware

**Hardware** lists firmware updates fwupd knows of, the disks with their
temperatures, the sensors, battery health and the PCI and USB devices, all
read without changing anything. `f` updates a device's firmware after you
type FIRMWARE and confirm its plan; keep a laptop on mains power while it
runs. `s` checks a disk's SMART health, which asks for your password.

## Services and space

**Services** lists what systemd starts on the machine and in your session,
and what your desktop starts at login; `i` switches between them and `f`
filters by name. Select one to see what needs it; `s` starts or stops it
and `e` enables or disables it at boot, each after its plan. For what the
machine depends on (the login screen, the network, the firewall) the plan
says first what stops working. An application at login is turned off for you
only, and can be turned on again the same way.

**Storage** shows how full the disk is and what can be cleaned, with what
each would free: the package cache, packages nothing needs any more,
unused Flatpak runtimes, old logs, the trash and picture previews. `Enter`
on one shows its plan. Snapshots, bootable kernels and configuration
backups are never offered.

## A slow boot, and a bad day

**Performance** shows how long the machine took to start and where the
time went, from the firmware to the login screen. Every boot is recorded,
so after a change you can see this boot beside the one before (`b` picks an
earlier one): what got slower, whether the kernel changed, and what Helix
Toolbox ran in between. When the boot waited for something it rarely needs,
such as the network, a hint says so and `Enter` plans turning it off.

**Recovery** asks the questions to answer before something goes wrong: is
there a snapshot to go back to, can the boot menu start one, is there a
second kernel, are your own files backed up somewhere else, and is there a
Drift baseline to rebuild a new machine from. `c` looks inside the newest
snapshot as root, reading only; `f` runs an entry that sets the missing
part up.

## A machine without internet

On a machine of the same distribution that is online, `helix-toolbox bundle
create ~/bundle git htop neovim` puts those entries' packages and Helix Toolbox
itself in a folder. Copy it to the other machine, and there, inside it, run
`./helix-toolbox bundle install .`: it checks every file, then installs from
them; with everything in the bundle, nothing is downloaded. A Flatpak
installed on the first machine goes in too, with its runtimes, once its
remote has a collection ID (`bundle create` says how to give Flathub its
own). Entries that need the network are named so you can run them later.
In the menu and the window, **Offline bundles** does the same: it bundles the
Install Plan's entries, and checks and installs a bundle from its folder.

## When something is wrong

**Helix Doctor** checks the network and name resolution, failed services,
the graphics drivers, the package database, home folder permissions, boot
images, GRUB, the EFI partition and disk space. It only reads. `Enter` shows
the evidence; where Helix Toolbox can repair a problem, `f` shows the repair,
which runs after its own confirmation.

**Backups** lists the configuration backups entries left, newest first.
`Enter` restores one; the current file is kept as a backup first, so a
restore can be undone the same way. With Snapper or Timeshift set up,
Helix Toolbox can take a snapshot before a run that changes system files, the
kernel or disks (Settings, or `--snapshot` on the command line). It never
promises to roll back packages itself.

**History** lists every run with its result; `Enter` opens its saved log.

To report a problem, run `helix-toolbox report` and paste its output into an
issue; [REPORTING.md](REPORTING.md) says what helps, and
[STABILITY.md](STABILITY.md) what stays the same across 1.x releases.

## The command line

Everything the menu does is also a command, for scripts and for machines
without a full terminal: `helix-toolbox --help` lists them, and each has its own
`--help` and manual page (`man helix-toolbox`). The commands that read take
`--json` for other programs ([JSON.md](JSON.md)). A few to start with:

```sh
helix-toolbox search editor          # find entries
helix-toolbox status                 # what is installed, updated or managed
helix-toolbox run git ripgrep --dry-run
helix-toolbox updates --online       # what every package tool has waiting
helix-toolbox notifications on       # a desktop notification when updates wait
helix-toolbox doctor                 # the health checks
helix-toolbox report                 # for a bug report
```

## Your files

| Path | What |
|---|---|
| `~/.config/helix-toolbox/settings.toml` | Theme, icons and the other settings |
| `~/.config/helix-toolbox/favorites.toml` | Pinned entries |
| `~/.config/helix-toolbox/profiles/` | Your profiles |
| `~/.config/helix-toolbox/baselines/` | Your baselines, for Drift |
| `~/.config/helix-toolbox/looks/` | Your saved desktop looks |
| `~/.local/state/helix-toolbox/` | History, saved logs, and the updates last told of |
| `~/.config/systemd/user/helix-toolbox-updates.*` | The update notifications' timer, when they are on |

Removing Helix Toolbox leaves them; delete them to start afresh.
