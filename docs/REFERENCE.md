# How Helix Toolbox works

What each part of Helix Toolbox does, in detail. [GUIDE.md](GUIDE.md) is the
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
Dashboard. Pins are kept in `~/.config/helix-toolbox/favorites.toml`. Each area
remembers its group and row when you move to another.

The menu follows the Helix Toolbox dashboard design: cards for the main screens
and quick actions, with Nerd Font icons. `t` or Settings switches between
four themes: **solid**, the default, paints the design's dark panels;
**transparent** draws over your terminal's own background, so a wallpaper or
transparency shows through (it was called **default** before, and that name
still works); **compatible** keeps to sixteen colors; **monochrome** uses no
color at all, only bold and reverse video. With `NO_COLOR` set, Helix Toolbox
starts in monochrome without changing the theme it remembers. Settings also turns the icons
off for terminals without a Nerd Font. Both choices are remembered in
`~/.config/helix-toolbox/settings.toml`; `--theme` and `--plain` override them
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
is a partial upgrade, which Arch does not support. Afterward each package's
version is compared with its version before; one that did not change is not
verified. Entries managed by Nix say to update them there.

After every run, each package step is checked with the package manager, and
each script entry for what its `[detect]` table says it installs and for its
`[[verify]]` checks (docs/CATALOG.md): the files and folders it leaves,
words on the kernel command line, the value of a setting and whether it
names something installed, and systemd units enabled or running. They are
read as you and change nothing. The result says `verified`, or `not
verified:` with what is wrong; what cannot be read here (a file only root
can read, a container without systemd) is reported as not checked, never as
passed. Checks that hold only after a reboot are kept beside the history and
made once the machine has started again: Helix Doctor's **Checks after the
reboot** shows them waiting, then what they found. `c` on an entry, **Check
it now** in the window and `helix-toolbox verify <entry>…` (exit status 1
when one fails, `--json` too) make an entry's checks at any time.

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

The **Helix Toolbox** row is Helix Toolbox itself, updated the way it was
installed, which it tells from where it runs. `r` asks GitHub for the latest
release. A package in `/usr/bin` is updated by downloading the new release's
package for this distribution into `~/.cache/helix-toolbox/release`, checking it
against the release's `SHA256SUMS` (the hidden `helix-toolbox fetch-release`
command, which takes only a release version and one of the release's own file
names), then installing it with `pacman -U`, `apt-get install`, `dnf install`
or `zypper install`; the window's package (`helix-toolbox-gui`) is fetched and
installed with it when the package tool says it is installed. On Arch with
the helix-toolbox repository in `pacman.conf` (INSTALL.md), the row runs
`pacman -Syu --needed helix-toolbox` (and `helix-toolbox-gui`) instead, which
updates the system with them. A copy from `cargo install` runs `cargo install
--locked helix-toolbox` again. A build in a checkout fast-forwards it from
GitHub and builds it, and leaves a checkout with local changes alone. A copy
anywhere else that the user owns (`~/.local/bin`, say) is replaced by the
release's program, fetched and checked the same way, with `install -m 0755`;
one the user does not own is left to whoever put it there. The window run
as an AppImage (the `APPIMAGE` variable names the file) has the release's
AppImage fetched, checked and put in its place, whatever updates the command
line. The launcher and `start.sh` fetch the latest release every time they
start, so there is nothing to run for them.

**Update notifications** (Settings in the menu and the window, or
`helix-toolbox notifications on`) write two units of the user's own,
`helix-toolbox-updates.timer` and `helix-toolbox-updates.service`, into
`$XDG_CONFIG_HOME/systemd/user`, enable the timer with the same link
`systemctl --user enable` makes, and start it; no password is asked. The
timer starts the service 10 minutes after the user's systemd starts (at
login) and every 6 hours after that, each time up to 10 minutes later at
random. The service runs `helix-toolbox notifications check`: the Updates
overview, online, as `helix-toolbox updates --online` reads it. When
something waits that the last notification did not tell of (the package
names and new versions it told of are kept in
`$XDG_STATE_HOME/helix-toolbox/notified.txt`), it shows a notification with
`notify-send`, or with `gdbus` where libnotify is missing. With libnotify
0.7.10 or later and the window installed, the notification has a **Show
updates** button and waits up to 30 minutes for it, which opens the window
with `--updates`, on the Updates screen; the service names the window it
was turned on from (an AppImage by its file), else `helix-toolbox-gui` on
the PATH. The check never updates anything. `helix-toolbox notifications
off` stops the timer and removes the units and `notified.txt`. A machine
without systemd gets no files and says so.

The same timer can watch more, each read-only and told only when something
is new: **Tell me of new health problems** (`helix-toolbox notifications
health on`) runs Helix Doctor's health checks and tells of a check that has
become a problem since it last told (a failed service, a disk filling up,
checks after a reboot that did not hold); the network and DNS checks, which
come and go, are left out. **Tell me when it drifts from a baseline**
(`helix-toolbox notifications drift <baseline>`, or `off`) compares the
machine with a saved Drift baseline and tells of each new difference once.
Their buttons open the window on Helix Doctor (`--doctor`) or Drift
(`--drift`). Nothing is repaired or put back by itself. The choices are kept
under `[watch]` in `settings.toml`, what was told in `health-told.json` and
`drift-told.json` beside `notified.txt`; the timer is on while any choice
is, and `helix-toolbox notifications` says which.

Script entries such as Steam, VS Code or Firefox show the same status when
they say what they install (most application scripts do), and `u` updates
that package; a script entry without it shows only its flags and is updated
by running it again. When a command is on the PATH but in none of an entry's
packages, the package that owns the file is looked up (`pacman -Qo`, `dpkg
-S`, `rpm -qf`), so `/usr/bin/github-desktop` shows as `pacman: github-desktop
3.4.14-2`.

## The header banner

In terminals that show pictures through Kitty's graphics (Ghostty, Kitty) or
Sixel (foot, WezTerm, xterm with Sixel on), the header is the Helix Toolbox
banner. Elsewhere, including Konsole, tmux and the console, with icons off or
in the compatible theme, it is the text header. Helix Toolbox asks the terminal
once at start and waits at most 0.4 seconds for the answer.

## Profiles

**Profiles** of your own: queue what a workstation needs (applications,
the Myfish shell setup, dotfiles configurations, system tweaks; all are
entries) and press `s` on the Install Plan to save it under a name. It is a
TOML file in `~/.config/helix-toolbox/profiles/` and appears under Profiles,
**Your profiles**, where `Enter` runs it and `Space` queues it, like the
built-in ones; its entries run in order, after what they depend on.
In the menu's Profiles area, `x` on one of your profiles exports it to a
file, `Delete` deletes it (after you type yes) and `o` imports a profile
file; the window has the same as buttons. On the command line:
`helix-toolbox profile list`, `export <name> [file]`, `import <file>` (which
refuses a profile naming entries this catalog does not have) and `delete
<name>`; `helix-toolbox run profile-<name>` runs one.

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
desktop, and the entries without a package Helix Toolbox has run successfully.
`s` on the Drift screen, or `helix-toolbox drift save <name>`, saves one in
`~/.config/helix-toolbox/baselines/`; `--path` adds files of your own.

Comparing lists, most important first: **Missing** applications and
configuration files, entries that ran there and are **Not run** here,
**Replaced** paths (a file that became a Home Manager link), **Changed**
files and settings, then what was **Added** since and other **Versions**.
`b` compares with another baseline or with a profile instead, which checks
each of its entries: installed, its files present, or run here. An entry
with no recorded run here still counts as set up when what it sets is in
place: the files it writes are all there, or its installer's own renderer
(`configure.py`, which only prints) finds the boot or login-screen settings
it would write already in the file, as after a run from before Helix Toolbox
1.1 recorded runs, or a change made by hand. `Enter`
explains a difference; `Space` adds the entry that puts it back to the
Install Plan and `a` adds all of them, to be reviewed and confirmed there.

To set up a new machine like an old one: `helix-toolbox drift save "Old
laptop"` and `helix-toolbox drift export "Old laptop"` there, then `helix-toolbox
drift check old-laptop.toml` on the new one; it ends with the `helix-toolbox
run` that would restore what is missing (planned and confirmed first), and
names what this distribution cannot install. `drift list`, `import` and
`delete` manage the saved ones; `--exit-code` makes `check` exit with 1
when anything differs.

## The setup wizard

The first time Helix Toolbox starts, it offers the setup wizard (Settings,
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
`helix-toolbox setup` lists the workstations; `helix-toolbox setup gaming` prints
what it would add and its plan, and the `helix-toolbox run` that would run it.

## The desktop

**Desktop** shows the desktop environment and its version (KDE Plasma,
GNOME, Xfce, Cinnamon, MATE, Hyprland, Sway), the session, the login screen
with its configuration files and SDDM theme, and the appearance settings
read with the desktop's own tools (`kreadconfig6`, `gsettings`,
`xfconf-query`): color scheme, style, icons, cursor, global theme, font.
It warns when something else sets them: Home Manager's Plasma or dconf
steps, or Stylix, which may set them again at the next switch, and refuses
to change them where Home Manager links the settings file itself. `s`
saves the look in `~/.config/helix-toolbox/looks/`; `Enter` on a saved look
puts back what differs with the desktop's own commands
(`plasma-apply-colorscheme`, `gsettings set`, …), after confirmation. The
entries for this desktop (its login screen theme, terminal configuration,
boot splash) are listed below, to run or queue. `helix-toolbox desktop`,
`desktop save <name>`, `desktop apply <name>`, `list` and `delete`.

## Offline bundles

For a machine without reliable internet, `helix-toolbox bundle create <folder>
<entries…>` downloads, on a machine of the same distribution and
architecture, the packages those entries need into a new folder, with a copy
of Helix Toolbox (the catalog and installers are inside it), a manifest
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

On the other machine, `./helix-toolbox bundle verify .` checks that the bundle
was made for this distribution and architecture and that every file matches
its checksum. `./helix-toolbox bundle install .`, after confirmation, copies the
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

**Offline bundles** in the menu and the window does the same from the
Install Plan: it says what a bundle of the queued entries would download,
makes one in a folder you name (with every dependency or only what this
machine lacks), and checks a bundle's folder or installs from it, with the
bundle's word taken only when you switch that on.

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

`Enter` on a file shows it now and afterward, side by side; on any other
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
`helix-toolbox --effects plan --fetch-sources ID`, downloads it into
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
record and needs your password. `helix-toolbox hardware`, `--smart <disk>` and
`--firmware <device>` do the same from the command line.

## Services and startup

**Services** lists systemd's units of the machine and of your own session:
every installed service, timer and socket with whether it is enabled
(starts at boot or login) and whether it runs now, and those running or
failed without a unit file of their own (the per-application units desktops
start are left out). `i` shows the machine's services, timers and sockets,
your session's, what starts at login, or what failed; `f` narrows them to a
name or description. `Enter` shows a unit's state, what needs it (`systemctl
list-dependencies --reverse`) and, for a unit the machine or the session
needs, what stopping or disabling it costs: the login screen (your session
ends, or the next boot stops at a text console), networking and name
lookups, the firewall, SSH while you are connected through it, D-Bus,
polkit and systemd's own units, and in your session sound and the desktop
portals. `s` starts or stops it, `e` enables or disables it (not static,
masked or generated units, which something else starts), `R` restarts it;
each is a plan with that cost as a warning first, high risk for those units,
and the machine's own ask for your password. Masking is not offered.

**At login** lists the XDG autostart applications (`/etc/xdg/autostart` and
`~/.config/autostart`), whether each starts and whether it applies to this
desktop. `Enter` turns one off for you only, with a file of yours in
`~/.config/autostart` that says `Hidden=true` (the system's stays as it
is), or on again by taking that out. `helix-toolbox services` (`list
[filter] [--user] [--failed]`, `show <unit>`, `start`, `stop`, `restart`,
`enable`, `disable` with `--user`, `--dry-run` and `--yes`) and
`helix-toolbox startup` (`list`, `on <file>`, `off <file>`) do the same.

## Storage

**Storage** shows the space on `/` (and the home folder's file system when
it is apart), then what can be cleaned, each with what it takes and what
cleaning it frees: the package cache (pacman keeps the two newest versions
of each installed package through `paccache`, whose own dry run says what
it frees; apt, dnf and zypper clean theirs), unused packages (pacman's
orphans, apt's and dnf's autoremove, zypper's unneeded, by name, never a
kernel), unused Flatpak runtimes (`flatpak uninstall --unused`), system logs
older than four weeks (`journalctl --vacuum-time=4weeks`), the trash,
thumbnails and, on Arch, paru's and yay's build folders. `Enter` shows one's
plan, which asks first. The kernels the machine can boot, snapshots and
Helix Toolbox's configuration backups are listed and never offered: what
could undo a mistake stays. `helix-toolbox storage` and `storage clean
<item>` (`--dry-run`, `--yes`) do the same.

## Performance

**Performance** reads systemd's record of this boot (`systemctl show` of
the manager's timestamps, as `systemd-analyze time` counts them): the
firmware, the boot loader, the kernel, the initial RAM disk and the
services, then what the login screen waited for (`systemd-analyze
critical-chain`) and the slowest services since the boot (`systemd-analyze
blame`). Below it, the machine now: the load against its processors,
memory and swap (zram or a disk), the kernel's pressure figures for the
processor, memory and the disk over the last minute, the frequency
governor and power profile, and the programs holding the most memory.

Each boot is recorded once, the first time Helix Toolbox reads it or the
notifications' timer runs, in `boots.json` beside the history (the last
50). This boot is set beside the one before it; `b` (or the window's
choice) sets it beside an earlier one. The comparison lists each part and
each unit that moved by half a second or more, a kernel that changed, and
the Helix Toolbox runs between the two boots. Hints say what is worth
knowing: the firmware or the boot menu taking long, a service the login
screen waited two seconds or more for (for `NetworkManager-wait-online` and
`systemd-networkd-wait-online`, `Enter` plans disabling it, which asks
first), memory nearly full, swapping to disk without zram, and programs
waiting. It only reads. `helix-toolbox performance` (`boots`, `compare
<before> [after]`, a boot by its number in `boots` or the start of its id)
does the same.

## Recovery

**Recovery** checks what would bring the machine back, reading only:

- **System snapshots**: Snapper (with a root configuration) or Timeshift,
  and Snapper's count and newest where this user may list them; a notice
  when the newest is over a month old, or when nothing takes snapshots,
  with the entry that sets Snapper or Timeshift up.
- **Starting from a snapshot**, on Btrfs: whether the boot menu lists
  snapshots (grub-btrfs, limine-snapper-sync, openSUSE's GRUB entries), so a
  system that no longer starts can start from one.
- **A second kernel**: the kernels in `/boot` with their boot images, and on
  Arch the advice to keep `linux-lts` beside the kernel.
- **Configuration backups**: how many Helix Toolbox's entries left, and
  the newest.
- **Your own files**: a backup program for the home folder (Borg, restic,
  Kopia, Déjà Dup, Pika Backup, Vorta, Back In Time and others), since
  system snapshots leave `/home` out and share the disk.
- **A record of this machine**: the newest Drift baseline, a notice when
  there is none or it is over 90 days old.

`c` runs the snapshot check: as root, it lists the snapshots and, for
Snapper, checks the newest is a Btrfs subvolume holding `/etc/os-release`,
`/usr/bin` and `/usr/lib/modules`; for Timeshift, `timeshift --list`. It
rolls nothing back and takes or removes nothing, and asks first. `f` runs
the entry a check names. `helix-toolbox recovery` (`--check-snapshots`
with `--dry-run` and `--yes`, confirmed by typing CHECK) does the same.

## Reliability and trust

**Reliability** lists every entry with how far it has been tested, the
critical ones first (marked `!`: boot loaders, the boot splash, the login
screen, kernel security modules, graphics drivers and whole-system or disk
changes):

| Level | Means |
|---|---|
| Hardware Tested | a tester ran it on a real machine and reported it (`helix-toolbox report --worked`) |
| VM Tested | the Distributions workflow ran it for real in a virtual machine (Arch, Debian or Fedora, on BIOS or UEFI firmware, or the Arch live ISO), rebooted into the result and checked it |
| Container Tested | the Distributions workflow ran it in distribution containers and Helix Toolbox verified the result there |
| Stand-in Tested | installer tests run its script against stand-in commands and files, on every change and in each container |
| Not Yet Validated | nothing tests it yet beyond the checks every entry gets |

Beside each are the systems it was tested on, the date it was last verified
and when it last ran successfully on this machine. Release builds carry the
record of the Distributions run they were built after; other builds show the
stand-in tests and reports only. `i` shows only critical or only untested
entries, `Enter` the evidence. Before a critical entry with no more than
stand-in tests runs, its confirmation says so. `helix-toolbox reliability
[--critical]` prints the same.

An entry that ran here can be reported as working: `w` on it, or on its run
in History (**Report it worked…** in the window), or `helix-toolbox report
--worked <entry> [--note "what you checked"]`. The report gives the entry,
the date and version of its last successful run here, and the
distribution, desktop, model and graphics cards with their drivers (no
serial numbers, host name or user), with the `[[hardware]]` record it asks
for and a link that opens the issue form filled in. It is refused for a run
that failed and in a virtual machine or a container. Nothing is sent until
you submit the issue.

`v` on any entry (or `helix-toolbox trust <id>`) opens the **trust inspector**:
the installer's origin (Helix Toolbox's own installers, Linutil's, or commands
built from package names), its file and SHA-256 checksum, its license and
the software's (as the package manager reports it), and three kinds of
assurance kept apart:

- **Release provenance**: whether this Helix Toolbox, which ships the catalog
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
The Helix Doctor reports are listed below the checks. `helix-toolbox doctor`
prints the checks and `helix-toolbox doctor --repair <id>` runs one repair.

The **Backups** screen (and `helix-toolbox backups`) lists the configuration
backups Helix Toolbox's entries leave beside a file before changing it
(`<file>.commander-backup-<time>`, `<file>.helix-toolbox-backup-<time>`), newest
first: your own in `~/.config`, `~/.local/share` and `~`, and system ones in
`/etc`, `/boot` and the SDDM theme and font folders. `Enter` (or `helix-toolbox
restore <backup>`) puts one back after a confirmation: the file now in its
place is moved aside as its own backup first, so a restore can be undone the
same way; system files ask for your password. Only backups Helix Toolbox finds
can be restored.

On Btrfs with Snapper (a `root` configuration) or with Timeshift, a snapshot
is taken first before runs that change system files, the kernel or disks
(flags PFM, K, D, SI, or high risk): **Snapshot before system changes** in
Settings, `--snapshot` on the command line. Helix Toolbox does not roll
snapshots or packages back itself; the Backups screen says where to (Btrfs
Assistant, `snapper rollback`, Timeshift).

## The command line


```sh
helix-toolbox system                     # the distribution and package manager it sees
helix-toolbox list                       # the catalog as tabs, groups and entries
helix-toolbox search rip grep            # entries by name, id or description, closest first
helix-toolbox info git                   # one entry
helix-toolbox status                     # every package entry: installed, update, managed or not
helix-toolbox update --dry-run           # what updating every entry with an update would run
helix-toolbox updates --online           # what each package tool has waiting
helix-toolbox updates --apply --dry-run  # what updating those channels would run
helix-toolbox notifications on           # tell me on the desktop when updates wait; off, check, or nothing for what is on
helix-toolbox notifications health on    # and of new health problems; drift <baseline> for Drift
helix-toolbox plan git system-report     # what running entries would do
helix-toolbox run git --dry-run          # the plan, nothing changed
helix-toolbox run git                    # runs after you type RUN, asked on the terminal even when piped
helix-toolbox remove ardour              # removes a package entry that offers it
helix-toolbox profile list               # your profiles; also export, import, delete
helix-toolbox drift save "Old laptop"    # record this machine as a baseline
helix-toolbox drift check                # how it differs now; --profile <name> for a profile
helix-toolbox setup gaming               # what the Gaming workstation would add here, as a plan
helix-toolbox bundle create ~/b git htop # packages and Helix Toolbox, for a machine offline
helix-toolbox desktop                    # the desktop, its look and who else sets it
helix-toolbox plan sddm --effects        # what it would change, files side by side
helix-toolbox --effects plan --fetch-sources dotfiles-nvim  # with the dotfiles source to compare
helix-toolbox hardware                   # firmware, disks, temperatures, batteries, devices
helix-toolbox services list --failed     # systemd units; show, start, stop, restart, enable, disable
helix-toolbox startup off <file>         # what the desktop starts at login, for you
helix-toolbox storage                    # where the space goes; storage clean <item> frees some
helix-toolbox performance                # this boot, the machine now, and the boot before; boots, compare
helix-toolbox recovery                   # snapshots, kernels, backups; --check-snapshots looks inside, as root
helix-toolbox reliability --critical     # how far the boot, login and driver entries are tested
helix-toolbox trust sddm                 # where an entry comes from and what checks it
helix-toolbox doctor                     # health checks, read-only
helix-toolbox doctor --repair permissions  # one repair, after confirmation
helix-toolbox backups                    # configuration backups, newest first
helix-toolbox restore <backup>           # put one back, keeping the current file
helix-toolbox history                    # recent runs
helix-toolbox docgen docs/walkthrough.md # the catalog as Markdown
helix-toolbox status --json              # any reading command, or a --dry-run plan, as JSON
helix-toolbox report                     # what a bug report needs, to paste into an issue
helix-toolbox report --worked sddm       # that an entry worked here, for its Hardware Tested record
helix-toolbox verify sddm grub-dracula   # whether entries left the machine as they should, read-only
```

`--json` prints one JSON document for other programs instead of text; it
works with the commands that read and with `--dry-run` plans, and refuses
anything that would change the machine. The shapes are in
[docs/JSON.md](docs/JSON.md).
