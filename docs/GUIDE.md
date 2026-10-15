# Using HelixForge

HelixForge sets up and looks after a Linux machine from one menu in the
terminal. Everything it changes is planned first and shown to you, runs only
after you confirm it, and is recorded in History. This guide walks through
the menu and the common jobs; [INSTALL.md](INSTALL.md) covers installing it
and the [README](../README.md) the catalog format and the command line in
full.

## Starting

Run `helixforge` as your normal user, not with `sudo`: entries that change
the system ask for your password inside the run, when they need it. The
terminal should be at least 100 columns by 25 rows; a Nerd Font shows the
icons (Settings turns them off otherwise). In Ghostty, Kitty, WezTerm, foot
and other terminals that show pictures, the header is the HelixForge banner.

## Finding your way

The sidebar on the left lists the screens. `Tab` and `Shift-Tab` move
between them from anywhere; in the sidebar, `j`/`k` or the arrows move and
`Enter` opens the screen. `h` or `←` comes back to the sidebar. `?` shows
every key, `/` searches every area, `t` changes the theme and `q` quits.

| Screen | What it is for |
|---|---|
| Dashboard | The machine at a glance, and quick actions |
| Applications | The application catalog, in groups |
| Favourites | Entries you pinned with `*`, then the ones you ran recently |
| Install Plan | What you queued, checked and ready to run together |
| Configurations | Shell, dotfiles and desktop configuration |
| Helix Doctor | Health checks that only read the system, with repairs |
| Maintenance, Security | Clean-ups, system tweaks and hardening |
| Profiles | Ready-made sets of entries, and your own |
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
4. `Space` queues it in the **Install Plan**; queue as many as you like.
   `Enter` runs one entry straight away instead.
5. On the Install Plan, check the plan: the steps in order, the risk, whether
   it asks for your password, and anything that blocks it (a missing tool,
   too little space, a conflicting package). `Enter` runs it.
6. The run asks `[Y]` first, then shows its output in a window inside the
   menu. Type into it when an installer asks a question. `Ctrl-C` stops it;
   `l` saves the log.
7. Afterwards HelixForge checks each package with the package manager and
   says what it verified. Anything that did not finish stays in the Install
   Plan to run again.

`i` in a list cycles between all entries, installed ones and ones with an
update. `*` pins an entry to **Favourites**. `r` removes an installed package
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

## Keeping the machine up to date

**Updates** shows each tool that keeps the machine: pacman or apt, dnf or
zypper; the AUR helper; Flatpak; Home Manager, NixOS or a Nix profile. It
first shows what those tools already know, without going online; `r` checks
online. `Enter` lists the packages, `u` updates the selected tool and `U`
every tool with updates waiting. HelixForge runs each tool's own update
command; on Arch that is the whole system, as Arch requires. Nix cannot count
its updates beforehand, so its row shows how old its locked packages are, and
updates with `u`.

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
issue; [BETA.md](BETA.md) says what helps.

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
| `~/.local/state/helixforge/` | History and saved logs |

Removing HelixForge leaves them; delete them to start afresh.
