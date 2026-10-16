# How HelixForge works

What each part of HelixForge does, in detail. [GUIDE.md](GUIDE.md) is the
shorter walk-through for getting started; [CATALOG.md](CATALOG.md) covers the
entries themselves and how a run is planned.

## The menu

In the interface the sidebar is a menu of screens. `Tab` or `j`/`k` in the
sidebar move between Dashboard, Applications, Favourites, Install Plan, and the other
areas, History and Settings; `Enter` or `l` opens the selected screen and
`h` returns to the sidebar. In an area, `j`/`k` or the arrows move in the
list, `Enter` opens a group or runs an entry, `Space` adds it to the install
plan, `d` shows the description, `p` the script or command, `g` explains the
flags column and `/` searches every area. The Install Plan shows what is
queued with its risk and requirements; `Enter` runs it. Each run asks `[Y]`
first and then runs in a terminal inside the window; `Ctrl-C` stops it and
everything it started (if something ignores that, the window says so and a
second `Ctrl-C` kills it), `l` saves its output, and History lists every run.
`?` shows all keys and `q` quits. The menu needs a terminal; the commands
below also work without one.

Search ranks what it finds: names that match exactly or start with the
text first, then names containing it, ids, loose spellings (`rpgrp` finds
Ripgrep) and descriptions. `Escape` clears it and goes back to the row you
were on. `i` cycles the list between all entries, installed ones (including
those managed by Nix) and ones with an update, flattening the groups of the
area. `*` pins the selected entry; **Favourites** lists pinned entries, then
the ones run recently, from History, and is also a quick action on the
Dashboard. Pins are kept in `~/.config/helixforge/favourites.toml`. Each area
remembers its group and row when you move to another.

The menu follows the HelixForge dashboard design: cards for the main screens
and quick actions, with Nerd Font icons. `t` or Settings switches between
four themes: **solid**, the default, paints the design's dark panels;
**transparent** draws over your terminal's own background, so a wallpaper or
transparency shows through (it was called **default** before, and that name
still works); **compatible** keeps to sixteen colours; **monochrome** uses no
colour at all, only bold and reverse video. With `NO_COLOR` set, HelixForge
starts in monochrome without changing the theme it remembers. Settings also turns the icons
off for terminals without a Nerd Font. Both choices are remembered in
`~/.config/helixforge/settings.toml`; `--theme` and `--plain` override them
for one run.

## Status and updates

Package entries show how they stand on this machine, beside their flags:
**Installed** with where from and the version (`pacman 15.2.0-1`, `apt`,
`dnf`, `AUR`, `Flatpak 7.5`), **Update** when the repository metadata already
on the machine has a newer version (`pacman 15.2.0-1 → 15.3.0-1`; nothing is
downloaded to find out), **Managed** when the command comes from Nix (`Home
Manager 1.4.7`, `Nix`, `NixOS`), or **Not installed**. When a package is
installed but a Nix copy of its command comes first on the PATH, the status
says so (`; Home Manager's 0.12.5 runs first`). The check runs in the
background when the menu opens and after every run; `d` repeats it in the
description.

`u` updates the selected entry and `U` every entry with an update, after
the same confirmation as a run: `flatpak update` for its installation, `apt-get
install --only-upgrade`, `dnf upgrade` or `zypper update` for that package,
and on Arch `pacman -Syu`, the whole system, since updating one package alone
is a partial upgrade, which Arch does not support. Afterwards each package's
version is compared with its version before; one that did not change is not
verified. Entries managed by Nix say to update them there.

**Updates** shows the whole machine, not only catalog entries: for each tool
that keeps it (pacman or apt, dnf, zypper; the AUR helper; Flatpak's system
and user installations; Home Manager with a flake, NixOS, a Nix profile) what
is waiting, from what that tool already knows. `r` checks online through
read-only commands (`checkupdates`, which leaves pacman's database alone,
`paru -Qua` or `yay -Qua`, `flatpak remote-ls`); nothing else asks the
network. Nix channels show how old their locked nixpkgs is, since Nix counts
nothing before it updates. `Enter` lists the packages, `u` updates the
selected channel and `U` every channel with updates waiting, one after
another and the AUR after the repositories, through the tools themselves
(`sudo pacman -Syu`, `paru -Sua`, `flatpak update`, `nix flake update` then
`home-manager switch`), after the usual confirmation. The Home Manager update
changes the flake's `flake.lock`, in your dotfiles checkout when the
configuration is a link to one.

Script entries such as Steam, VS Code or Firefox show the same status when
they say what they install (most application scripts do), and `u` updates
that package; a script entry without it shows only its flags and is updated
by running it again. When a command is on the PATH but in none of an entry's
packages, the package that owns the file is looked up (`pacman -Qo`, `dpkg
-S`, `rpm -qf`), so `/usr/bin/github-desktop` shows as `pacman: github-desktop
3.4.14-2`.

## The header banner

In terminals that show pictures through Kitty's graphics (Ghostty, Kitty) or
Sixel (foot, WezTerm, xterm with Sixel on), the header is the HelixForge
banner. Elsewhere, including Konsole, tmux and the console, with icons off or
in the compatible theme, it is the text header. HelixForge asks the terminal
once at start and waits at most 0.4 seconds for the answer.

## Profiles

**Profiles** of your own: queue what a workstation needs (applications,
the Myfish shell setup, dotfiles configurations, system tweaks; all are
entries) and press `s` on the Install Plan to save it under a name. It is a
TOML file in `~/.config/helixforge/profiles/` and appears under Profiles,
**Your profiles**, where `Enter` runs it and `Space` queues it, like the
built-in ones; its entries run in order, after what they depend on.
`helixforge profile list`, `export <name> [file]`, `import <file>` (which
refuses a profile naming entries this catalog does not have) and `delete
<name>`; `helixforge run profile-<name>` runs one.

```toml
name = "Dev Box"
description = "Terminal tools and editor configuration"
entries = ["git", "ripgrep", "myfish-fish", "dotfiles-nvim", "ghostty"]
```

## Helix Doctor, backups and snapshots

**Helix Doctor** checks the machine, read-only, and shows what it found:
network and DNS (a name lookup of github.com), failed system and user
services with their last journal lines, whether each graphics card has a
driver in use (or nouveau where NVIDIA's is wanted), a stale pacman lock or
unfinished dpkg configuration, files in your home that belong to root,
kernels without an initramfs, a GRUB configuration without its menu, the EFI
partition's room and free disk space. `Enter` shows a check's evidence and
what to do; where a safe repair exists (restart the resolver or a service,
remove a stale lock, `dpkg --configure -a`, give files back to you, rebuild
the initramfs or GRUB's menu, clear package caches) `f` runs it after its own
confirmation, or runs the entry that addresses it (the graphics drivers).
The Helix Doctor reports are listed below the checks. `helixforge doctor`
prints the checks and `helixforge doctor --repair <id>` runs one repair.

The **Backups** screen (and `helixforge backups`) lists the configuration
backups HelixForge's entries leave beside a file before changing it
(`<file>.commander-backup-<time>`, `<file>.helixforge-backup-<time>`), newest
first: your own in `~/.config`, `~/.local/share` and `~`, and system ones in
`/etc`, `/boot` and the SDDM theme and font folders. `Enter` (or `helixforge
restore <backup>`) puts one back after a confirmation: the file now in its
place is moved aside as its own backup first, so a restore can be undone the
same way; system files ask for your password. Only backups HelixForge finds
can be restored.

On Btrfs with Snapper (a `root` configuration) or with Timeshift, a snapshot
is taken first before runs that change system files, the kernel or disks
(flags PFM, K, D, SI, or high risk): **Snapshot before system changes** in
Settings, `--snapshot` on the command line. HelixForge does not roll
snapshots or packages back itself; the Backups screen says where to (Btrfs
Assistant, `snapper rollback`, Timeshift).

## The command line


```sh
helixforge system                     # the distribution and package manager it sees
helixforge list                       # the catalog as tabs, groups and entries
helixforge search rip grep            # entries by name, id or description, closest first
helixforge info git                   # one entry
helixforge status                     # every package entry: installed, update, managed or not
helixforge update --dry-run           # what updating every entry with an update would run
helixforge updates --online           # what each package tool has waiting
helixforge updates --apply --dry-run  # what updating those channels would run
helixforge plan git system-report     # what running entries would do
helixforge run git --dry-run          # the plan, nothing changed
helixforge run git                    # runs after you type RUN, asked on the terminal even when piped
helixforge remove ardour              # removes a package entry that offers it
helixforge profile list               # your profiles; also export, import, delete
helixforge doctor                     # health checks, read-only
helixforge doctor --repair permissions  # one repair, after confirmation
helixforge backups                    # configuration backups, newest first
helixforge restore <backup>           # put one back, keeping the current file
helixforge history                    # recent runs
helixforge docgen docs/walkthrough.md # the catalog as Markdown
helixforge status --json              # any reading command, or a --dry-run plan, as JSON
helixforge report                     # what a bug report needs, to paste into an issue
```

`--json` prints one JSON document for other programs instead of text; it
works with the commands that read and with `--dry-run` plans, and refuses
anything that would change the machine. The shapes are in
[docs/JSON.md](docs/JSON.md).
