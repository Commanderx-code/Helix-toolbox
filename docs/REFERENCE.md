# How HelixForge works

What each part of HelixForge does, in detail. [GUIDE.md](GUIDE.md) is the
shorter walk-through for getting started; [CATALOG.md](CATALOG.md) covers the
entries themselves and how a run is planned.

## The menu

In the interface the sidebar is a menu of screens. `Tab` or `j`/`k` in the
sidebar move between Dashboard, Applications, Favorites, Install Plan, and the other
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

`Ctrl-K` opens the command palette from any screen: type part of what you
want and `Enter` does it. It finds every screen (`open updates`), actions
(`check updates online`, `run DNS diagnostics`, which opens Helix Doctor
at that check, `save this machine as a baseline`, `clear the install plan`,
`change the theme`) and every entry (`install ghostty`), loose spellings
included. An entry chosen there is planned and confirmed like any other.

Search ranks what it finds: names that match exactly or start with the
text first, then names containing it, ids, loose spellings (`rpgrp` finds
Ripgrep) and descriptions. `Escape` clears it and goes back to the row you
were on. `i` cycles the list between all entries, installed ones (including
those managed by Nix) and ones with an update, flattening the groups of the
area. `*` pins the selected entry; **Favorites** lists pinned entries, then
the ones run recently, from History, and is also a quick action on the
Dashboard. Pins are kept in `~/.config/helixforge/favourites.toml`. Each area
remembers its group and row when you move to another.

The menu follows the HelixForge dashboard design: cards for the main screens
and quick actions, with Nerd Font icons. `t` or Settings switches between
four themes: **solid**, the default, paints the design's dark panels;
**transparent** draws over your terminal's own background, so a wallpaper or
transparency shows through (it was called **default** before, and that name
still works); **compatible** keeps to sixteen colors; **monochrome** uses no
color at all, only bold and reverse video. With `NO_COLOR` set, HelixForge
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

The **HelixForge** row is HelixForge itself, updated the way it was
installed, which it tells from where it runs. `r` asks GitHub for the latest
release. A package in `/usr/bin` is updated by downloading the new release's
package for this distribution into `~/.cache/helixforge/release`, checking it
against the release's `SHA256SUMS` (the hidden `helixforge fetch-release`
command, which takes only a release version and one of the release's own file
names), then installing it with `pacman -U`, `apt-get install`, `dnf install`
or `zypper install`. A copy from `cargo install` runs `cargo install --locked
helixforge` again. A build in a checkout fast-forwards it from GitHub and
builds it, and leaves a checkout with local changes alone. The launcher and
`start.sh` fetch the latest release every time they start, so there is
nothing to run for them.

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

## Drift

**Drift** shows how this machine differs from a saved **baseline** or from
a profile, without changing anything. A baseline records what is installed
(each package entry's source and version), the configuration files and
folders catalog entries write (by SHA-256 checksum, never their contents),
the login shell, display manager, Arch repositories, Flatpak remotes and
desktop, and the entries without a package HelixForge has run successfully.
`s` on the Drift screen, or `helixforge drift save <name>`, saves one in
`~/.config/helixforge/baselines/`; `--path` adds files of your own.

Comparing lists, most important first: **Missing** applications and
configuration files, entries that ran there and are **Not run** here,
**Replaced** paths (a file that became a Home Manager link), **Changed**
files and settings, then what was **Added** since and other **Versions**.
`b` compares with another baseline or with a profile instead, which checks
each of its entries: installed, its files present, or run here. An entry
with no recorded run here still counts as set up when what it sets is in
place: the files it writes are all there, or its installer's own renderer
(`configure.py`, which only prints) finds the boot or login-screen settings
it would write already in the file, as after a run from before HelixForge
1.1 recorded runs, or a change made by hand. `Enter`
explains a difference; `Space` adds the entry that puts it back to the
Install Plan and `a` adds all of them, to be reviewed and confirmed there.

To set up a new machine like an old one: `helixforge drift save "Old
laptop"` and `helixforge drift export "Old laptop"` there, then `helixforge
drift check old-laptop.toml` on the new one; it ends with the `helixforge
run` that would restore what is missing (planned and confirmed first), and
names what this distribution cannot install. `drift list`, `import` and
`delete` manage the saved ones; `--exit-code` makes `check` exit with 1
when anything differs.

## The setup wizard

The first time HelixForge starts, it offers the setup wizard (Settings,
**Setup wizard**, and `Ctrl-K` "set up this machine" open it again). It shows
what it found: the distribution and package manager, the other package
tools (paru, yay, Flatpak, Nix, Home Manager, Snap), the desktop and session,
the graphics and whether this is a laptop. Then it offers workstations from
`catalog/setup.toml`: **Developer**, **Gaming** (with the NVIDIA driver entry
for the distribution where there is an NVIDIA card), **Privacy** (a firewall
baseline and AppArmor, or SELinux on Fedora), **Minimal** and **Custom**
(every suggestion, none ticked). Its entries are listed with how they stand,
ticked where they can run here and are not installed; `Enter` puts the
ticked ones in the Install Plan, where they are planned, previewed and
confirmed like anything else. The wizard runs nothing itself.
`helixforge setup` lists the workstations; `helixforge setup gaming` prints
what it would add and its plan, and the `helixforge run` that would run it.

## The desktop

**Desktop** shows the desktop environment and its version (KDE Plasma,
GNOME, Xfce, Cinnamon, MATE, Hyprland, Sway), the session, the login screen
with its configuration files and SDDM theme, and the appearance settings
read with the desktop's own tools (`kreadconfig6`, `gsettings`,
`xfconf-query`): color scheme, style, icons, cursor, global theme, font.
It warns when something else sets them: Home Manager's Plasma or dconf
steps, or Stylix, which may set them again at the next switch, and refuses
to change them where Home Manager links the settings file itself. `s`
saves the look in `~/.config/helixforge/looks/`; `Enter` on a saved look
puts back what differs with the desktop's own commands
(`plasma-apply-colorscheme`, `gsettings set`, …), after confirmation. The
entries for this desktop (its login screen theme, terminal configuration,
boot splash) are listed below, to run or queue. `helixforge desktop`,
`desktop save <name>`, `desktop apply <name>`, `list` and `delete`.

## Offline bundles

For a machine without reliable internet, `helixforge bundle create <folder>
<entries…>` downloads, on a machine of the same distribution and
architecture, the packages those entries need into a new folder, with a copy
of HelixForge (the catalog and installers are inside it), a manifest
(`bundle.toml`) and `SHA256SUMS`. By default it takes the dependencies this
machine lacks, which suits a machine set up like it; `--complete` takes every
dependency. pacman's packages come with their signatures, which `pacman -U`
checks against its keyring when it installs them; apt's are checked against
the checksums in the repository's signed index as they are downloaded; dnf's
are signed, and `rpm -K` checks them before they install; so are zypper's,
which zypper downloads as you, without root, into a scratch root that
shares this machine's repositories (an empty package database for
`--complete`, this machine's own otherwise), checking each against the
repository's signed metadata. Entries that need the network (AUR builds,
scripts, Nix, Flatpaks not installed here) are listed as such and left
out.

An entry with no package for this distribution whose Flatpak is installed
here goes in as that Flatpak: `flatpak create-usb` copies it, with the
runtimes and extensions it needs, from this machine's installation into the
bundle's `flatpak` folder, and the bundle keeps its remote's signing key.
`create-usb` needs the remote to have a collection ID; Flathub, added from
its `.flatpakrepo` file, has none until it is given its own:
`sudo flatpak remote-modify --collection-id=org.flathub.Stable flathub`.
`bundle create` says so rather than changing your remotes.

On the other machine, `./helixforge bundle verify .` checks that the bundle
was made for this distribution and architecture and that every file matches
its checksum. `./helixforge bundle install .`, after confirmation, copies the
packages into a new private folder, checks the copies against the checksums
it verified (written into the plan, not read from the bundle again), checks
each signature (`pacman-key --verify`, `rpm -K`), and installs them from the
files (dnf and zypper without their repositories; zypper asks its own
question only in a terminal, as it reads answers from nowhere else). Debian packages have no
signature of their own: each must be in this machine's signed repository
index with the same checksum, or the install is refused unless
`--trust-bundle` takes the bundle's word, for a bundle you made yourself.
A bundle installs as a whole, since its entries share their dependencies.
pacman and apt fetch only a dependency the bundle lacks, and only when the
network is there; a `--complete` bundle lacks none.

Flatpaks install from the bundle's copy (`flatpak install --sideload-repo`),
and flatpak checks each commit's signature against the remote's key on this
machine, as it would from the remote itself. `verify` checks that the copy
holds each application at the commit the manifest records. A remote without
a collection ID is given the bundle's when it is the one published for the
remote's address (Flathub's); any other, and a remote this machine lacks,
which is added with the bundle's copy of its key, only with
`--trust-bundle`, since nothing here vouches for them.

## What a change will do

Before a plan runs, `e` (on the Install Plan, in the confirmation, or on a
backup in Backups) and `--effects` on `plan` and the `--dry-run` commands show
what it is expected to change on this machine, step by step, each change
marked by how it is known:

- **✓ known**: worked out here. A package step asks the package manager
  without root (`pacman -Sp`, `apt-get -s`) which other packages come with it
  or go; a configuration file is rendered by the installer's own code
  (`configure.py`, which only prints) from the file as it is now and shown
  side by side with it; a restore compares the file with the backup.
- **~ expected**: what the entry declares in its `[effects]` table (files,
  packages, services, when it takes effect) or its script plainly runs.
- **? not known in advance**: what a script decides as it runs, the answers
  it asks you for, a boot menu regenerated from the installed kernels, or the
  dependencies dnf and zypper list only with root.

`Enter` on a file shows it now and afterwards, side by side; on any other
change, the whole line. The boot loader, boot splash, login screen and
AppArmor entries declare their effects; other scripts are previewed by the
paths they write and the services their script names, and say plainly that
the rest cannot be known before they run.

The dotfiles entries (Neovim, Fastfetch, Starship, Konsole and Ghostty) are
compared file by file once their pinned source, the dotfiles commit the
installers read, is on this machine: each file is new, changed (side by
side), the same, or, in a folder the installer replaces whole, left only in
the backup it keeps. The installer's own `configure.py` lists them without
writing anything, and refuses what another tool manages as it would when it
runs. Until then the preview names the commit; `d` on the changes, or
`helixforge --effects plan --fetch-sources ID`, downloads it into
`~/.local/share/helixforge/sources`, where the installers look first, and
changes nothing else.

## Hardware

**Hardware** reads the machine without root: fwupd's devices and the
firmware updates in the metadata it already has; each disk's kind, size,
model, firmware and temperature; every temperature sensor, with those near
their limit picked out; each battery's charge, health (what it holds against
its design capacity) and cycle count; PCI devices with their drivers, and USB
devices. A firmware update (`f`) asks you to type FIRMWARE, then shows its
plan with its own confirmation, because an interrupted firmware update can
leave a device unusable; a SMART health check (`s`) reads the disk's health
record and needs your password. `helixforge hardware`, `--smart <disk>` and
`--firmware <device>` do the same from the command line.

## Reliability and trust

**Reliability** lists every entry with how far it has been tested, the
critical ones first (marked `!`: boot loaders, the boot splash, the login
screen, kernel security modules, graphics drivers and whole-system or disk
changes):

| Level | Means |
|---|---|
| Hardware Tested | a tester ran it on a real machine and reported it |
| VM Tested | the Distributions workflow ran it for real in a virtual machine (Arch, Debian or Fedora, on BIOS or UEFI firmware, or the Arch live ISO), rebooted into the result and checked it |
| Container Tested | the Distributions workflow ran it in distribution containers and HelixForge verified the result there |
| Stand-in Tested | installer tests run its script against stand-in commands and files, on every change and in each container |
| Not Yet Validated | nothing tests it yet beyond the checks every entry gets |

Beside each are the systems it was tested on, the date it was last verified
and when it last ran successfully on this machine. Release builds carry the
record of the Distributions run they were built after; other builds show the
stand-in tests and reports only. `i` shows only critical or only untested
entries, `Enter` the evidence. Before a critical entry with no more than
stand-in tests runs, its confirmation says so. `helixforge reliability
[--critical]` prints the same.

`v` on any entry (or `helixforge trust <id>`) opens the **trust inspector**:
the installer's origin (HelixForge's own installers, Linutil's, or commands
built from package names), its file and SHA-256 checksum, its license and
the software's (as the package manager reports it), and three kinds of
assurance kept apart:

- **Release provenance**: whether this HelixForge, which ships the catalog
  and every installer, is a release build with a signed build provenance
  attestation, and whether its download was checked against `SHA256SUMS`.
- **Package integrity**: what each package source checks: pacman's
  signatures and the `SigLevel` in effect, apt's signed repositories, dnf's
  `gpgcheck`, zypper's and Flatpak's signatures; the AUR's user-submitted
  build scripts are marked unverified.
- **Downloads the installer makes itself**: sources pinned to a commit,
  downloads it checks with a checksum or signature, and ones nothing checks,
  such as a script piped into a shell or a plain HTTP address.

Then the privileges it uses (a password, system files, services, the
commands it runs as root) and its validation level.

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
helixforge drift save "Old laptop"    # record this machine as a baseline
helixforge drift check                # how it differs now; --profile <name> for a profile
helixforge setup gaming               # what the Gaming workstation would add here, as a plan
helixforge bundle create ~/b git htop # packages and HelixForge, for a machine offline
helixforge desktop                    # the desktop, its look and who else sets it
helixforge plan sddm --effects        # what it would change, files side by side
helixforge --effects plan --fetch-sources dotfiles-nvim  # with the dotfiles source to compare
helixforge hardware                   # firmware, disks, temperatures, batteries, devices
helixforge reliability --critical     # how far the boot, login and driver entries are tested
helixforge trust sddm                 # where an entry comes from and what checks it
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
