# The catalog and plans

How entries are written, and how Helix Toolbox turns the ones you choose into a
plan, checks it and runs it.

## Entries

`catalog/tabs.toml` names the areas: Applications, Configurations, Helix
Doctor, Maintenance, Security and Profiles. Each is a directory with a
`tab.toml`; inside it, entries are `<id>.toml` files and groups are
directories with a `group.toml`. An entry is one of six kinds:

```toml
name = "Ardour"
description = "A digital audio workstation."
flags = "I"                      # what it does; g in the interface explains the codes

[package]                        # tried in order: the family's package, the AUR, Flathub
binary = "ardour"                # optional: its command, for installs made another way
debian = "ardour"                # also arch, fedora, suse, alpine, void, solus
fedora = "ardour"
aur = "ardour"                   # Arch, through paru or yay, when arch is not given
flatpak = "org.ardour.Ardour"    # for families with no package
removable = true                 # offer removal: r in the interface, helix-toolbox remove
```

Whether an entry is installed is asked of the package manager (`pacman -Q`,
`dpkg-query`, `rpm -q` and the like) for its own packages, then of Flatpak.
Only when none of them is installed does `binary` count, so a tool installed
through Nix, Home Manager or a download is not installed twice. An entry
that replaces another package names it in `conflicts` (WINE Staging has
`conflicts = ["wine"]`): when that package is installed, the plan says it
will be replaced instead of taking the entry as installed.

A script or command entry says what it installs in a `[detect]` table with
the same fields (no `removable` or `conflicts`); its script still runs when
chosen, and the plan notes that the software is already there:

```toml
[script]
id = "gaming/steam-launcher"

[detect]
binary = "steam"
arch = "steam"
debian = "steam-installer"
fedora = "steam"
flatpak = "com.valvesoftware.Steam"
```

```toml
[script]                         # scripts/installers/system-report.sh, run from its directory
id = "system-report"
interactive = true               # the default; false for scripts that never prompt
```

```toml
[script]                         # or a script beside the entry's TOML file
path = "setup.sh"
```

```toml
[command]                        # an inline shell command
line = "echo hello"
```

```toml
[nix]                            # one of the two
profile_install = "nixpkgs#ripgrep"
home_manager_switch = "~/dotfiles/home-manager#commander"
```

```toml
[native]                         # implemented in Rust by the engine
action = "system-report"
```

```toml
[profile]                        # other entries, as one reviewable plan
entries = ["git", "ripgrep", "fzf", "neovim"]
```

Groups are listed before entries at every level. Entries and groups can carry
`order` to sort before their name among their own kind, `multi_select = false`
to keep them out of the queue, and `preconditions` that hide them on machines
they do not apply to:

```toml
[[preconditions]]
matches = true
check = "command_exists"         # or "file_exists", { containing_file = "..." }, { environment = "..." }
values = ["sddm"]
```

Scripts source the shared helpers with `. "$HELIX_TOOLBOX_LIB/common-script.sh"`. The installers brought across use its name before 1.7, `HELIXFORGE_LIB`, which every run sets too.
The catalog and scripts are embedded into the binary at build time and
unpacked to a private temporary directory when it starts, so scripts run as
real files. The copy is removed on exit; one left behind by a Helix Toolbox that
was killed is removed the next time Helix Toolbox starts.

A script entry can say what it changes in an `[effects]` table, which the
change preview shows before it runs:

```toml
[effects]
packages = ["plymouth (with plymouth-themes on Debian and Ubuntu)"]
services = ["enables sddm.service from the next reboot, if you agree"]
takes_effect = "the boot splash shows from the next reboot"
unknown = ["the regenerated boot menu, written from the installed kernels"]

[[effects.files]]
path = "/etc/default/grub"
change = "adds splash to the kernel command line; backed up first"
render = ["grub-cmdline", "/etc/default/grub", "add", "splash"]
when = "the machine boots with GRUB"
```

`render` names one of the installers' `configure.py` actions that only
print a file's new contents (`grub`, `grub-cmdline`, `sddm`,
`mkinitcpio-hooks`, `limine-cmdline`), then the file it reads, which must be
`path`; the preview runs it on the file as it is and shows the two side by
side. `{grub}` stands for `/boot/grub` (`/boot/grub2` on Fedora) and `{time}`
for the date and time the installer puts in a name.

A script entry's exit status alone does not show that it worked. After its
run, what its `[detect]` table says it installs is looked for, as for a
package entry, and its `[[verify]]` checks say what it leaves, each read
from the machine as you, changing nothing:

```toml
# The setting selects a theme that is installed.
[[verify]]
kind = "setting"
files = ["/etc/sddm.conf.d/zz-commander-theme.conf", "/etc/sddm.conf"]
section = "Theme"
key = "Current"
equals = "silent"
points_to = "/usr/share/sddm/themes/{value}/metadata.desktop"

[[verify]]
kind = "text"
path = "/proc/cmdline"
word = "splash"
after_reboot = true

[[verify]]
kind = "service"
unit = "fstrim.timer"
enabled = true
active = true

[[verify]]
kind = "path"
path = "/efi/dotfiles-splash.png"
is = "file"
when_exists = "/efi/limine.conf"
```

| Kind | Takes | Holds when |
|---|---|---|
| `path` | `path`, `is` (`file` or `directory`), `absent` | it is there (of that kind), or gone with `absent` |
| `text` | `path`, one `word` or `line`, `absent` | the file has the word (between spaces) or the whole line, or lacks it |
| `setting` | `files`, `key`, `section`, then `equals`, `contains` and `points_to`, or `unset` alone | the key's value, from the last of the files that sets it (as programs read drop-ins, then the main file), matches; `points_to` is a path made from it with `{value}` that must exist |
| `service` | `unit`, `enabled`, `active`, `user` | systemd says so, of the system's unit or your own |

Any check takes `when_exists` (it applies only where that path is) and
`after_reboot` (it holds only once the machine has started again: kept
beside the history, checked by Helix Doctor and `helix-toolbox verify` after
the reboot). Paths are absolute, under `~/` or under `{grub}/`. What cannot
be read here, a file only root can read or a container without systemd, is
reported as not checked, never as passed; a check that fails makes the run
not verified. The VM tests run `helix-toolbox verify` after each of their
cases, so the checks of the entries they cover are proven on real systems.

`catalog/setup.toml` holds the setup wizard's workstations, each with
entries and `[[workstation.when]]` additions for a graphics vendor
(`gpu`), a distribution family (`family`, `not_family`) or a desktop, and
the entries the Desktop screen offers for each desktop (`[[desktop]]`, `any`
for all).

`catalog/validation.toml` says how far entries are tested beyond the checks
every entry gets: `[critical]` names the entries whose failure can stop a
machine booting, logging in or showing a display; each `[[stand-ins]]`
names entries, the installer tests in `tests/installers/` that cover them
and what they cover; `[[hardware]]` and `[[vm]]` record runs reported from
real and virtual machines (`entry`, `system`, `version`, `date`, `note`).
A new installer test or a tester's report goes there; the Reliability
screen reads it.

## Plans

Running entries first makes a plan: the steps in order, what they need,
whether any asks for your password, and the risk from the flags. The
interface shows it in the confirmation; `helix-toolbox plan` and `helix-toolbox run
--dry-run` print it. Every run is recorded with its outcome, including runs
that failed or were interrupted.

Before anything runs, Helix Toolbox checks the plan on this machine. A
**blocker** stops the run (the menu says why instead of asking, the command
line exits without changing anything); a **warning** is shown with the
confirmation:

- a tool the plan needs is missing (an AUR helper, `flatpak`), unless
  another source of that package can do without it (blocker)
- two queued entries replace each other, such as WINE and WINE Staging
  (blocker: queue only one)
- an entry replaces an installed package (warning: the package manager asks
  before removing it)
- an entry needs an Arch repository that is off, such as Steam and multilib
  (blocker)
- an entry is for another processor, such as Steam on an ARM64 machine
  (blocker)
- an entry is for graphics hardware the machine does not have, such as the
  Nvidia drivers without an Nvidia card (warning)
- the root file system has less free space than the packages' installed
  sizes (`pacman -Si`, `apt-cache show`, `dnf repoquery`) plus what scripts
  say they need (blocker), or would be left with less than 1 GiB (warning)

- another Helix Toolbox run is still going (blocker): one run at a time, so
  package managers are never driven twice at once; their own locks guard
  against other tools

Steps run one at a time, in an order that puts each after what it depends
on: an entry that installs a command another step needs (Flatpak / Flathub
before a Flatpak, an AUR helper before an AUR package), or one it names in
`needs.after`; otherwise in the order queued. A tool an earlier step installs
is not reported as missing. When a step fails the run stops, or with **After a
failed step: continue** in Settings (`--keep-going` on the command line)
goes on with the steps that do not depend on it, skipping those that do; a
`Ctrl-C` always stops. Each step's result is shown afterward (done, failed
with its exit status, skipped, not run), the history records "1 of 3 steps
done", what did not finish stays in the Install Plan to run again, and the
command line prints the `helix-toolbox run …` that retries it.

Entries say what they need from the machine in an optional `[needs]` table:

```toml
[needs]
gpu = ["nvidia"]              # nvidia, amd or intel; any one will do
repositories = ["multilib"]   # Arch repositories that must be enabled
space_mb = 6000               # free space beyond its packages, for downloads
after = ["paru-aur-helper"]   # entries that run first when both are queued
architectures = ["x86_64"]    # x86_64 or aarch64; any when left out
```

An entry with `architectures` is left out of the menu and the lists on other
processors, like one whose preconditions fail. Named on the command line
anyway, it is blocked before anything runs. Give it to entries whose
programs are only built for one, such as Steam or DaVinci Resolve.

A package entry lists every source it has on this machine, in order: the
distribution's package, the AUR on Arch, then Flathub. The plan shows the
first with the others after it (`sudo pacman -S --needed meld  (else
Flathub)`). When a source fails, or finishes without the package being
installed, the run says so and tries the next one; a source whose tool is
missing is skipped. A command stopped with `Ctrl-C` stops the run instead.

After a run, Helix Toolbox checks each package step: an install passes when its
package manager lists one of the entry's packages or its Flatpak is
installed (the command counts only where nothing can be asked, as on Solus);
a removal passes when the package is gone. The
running window's title says `FINISHED · VERIFIED`, the command line prints a
`Verified:` line for each package, and the history records `exit 0,
verified`. A run whose program succeeded but whose package cannot be found
is recorded as failed, with what was missing.
