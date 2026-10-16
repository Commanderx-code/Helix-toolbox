# The catalog and plans

How entries are written, and how HelixForge turns the ones you choose into a
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
removable = true                 # offer removal: r in the interface, helixforge remove
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

Scripts source the shared helpers with `. "$HELIXFORGE_LIB/common-script.sh"`.
The catalog and scripts are embedded into the binary at build time and
unpacked to a private temporary directory when it starts, so scripts run as
real files. The copy is removed on exit; one left behind by a HelixForge that
was killed is removed the next time HelixForge starts.

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
interface shows it in the confirmation; `helixforge plan` and `helixforge run
--dry-run` print it. Every run is recorded with its outcome, including runs
that failed or were interrupted.

Before anything runs, HelixForge checks the plan on this machine. A
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
- an entry is for graphics hardware the machine does not have, such as the
  Nvidia drivers without an Nvidia card (warning)
- the root file system has less free space than the packages' installed
  sizes (`pacman -Si`, `apt-cache show`, `dnf repoquery`) plus what scripts
  say they need (blocker), or would be left with less than 1 GiB (warning)

- another HelixForge run is still going (blocker): one run at a time, so
  package managers are never driven twice at once; their own locks guard
  against other tools

Steps run one at a time, in an order that puts each after what it depends
on: an entry that installs a command another step needs (Flatpak / Flathub
before a Flatpak, an AUR helper before an AUR package), or one it names in
`needs.after`; otherwise in the order queued. A tool an earlier step installs
is not reported as missing. When a step fails the run stops, or with **After a
failed step: continue** in Settings (`--keep-going` on the command line)
goes on with the steps that do not depend on it, skipping those that do; a
`Ctrl-C` always stops. Each step's result is shown afterwards (done, failed
with its exit status, skipped, not run), the history records "1 of 3 steps
done", what did not finish stays in the Install Plan to run again, and the
command line prints the `helixforge run …` that retries it.

Entries say what they need from the machine in an optional `[needs]` table:

```toml
[needs]
gpu = ["nvidia"]              # nvidia, amd or intel; any one will do
repositories = ["multilib"]   # Arch repositories that must be enabled
space_mb = 6000               # free space beyond its packages, for downloads
after = ["paru-aur-helper"]   # entries that run first when both are queued
```

A package entry lists every source it has on this machine, in order: the
distribution's package, the AUR on Arch, then Flathub. The plan shows the
first with the others after it (`sudo pacman -S --needed meld  (else
Flathub)`). When a source fails, or finishes without the package being
installed, the run says so and tries the next one; a source whose tool is
missing is skipped. A command stopped with `Ctrl-C` stops the run instead.

After a run, HelixForge checks each package step: an install passes when its
package manager lists one of the entry's packages or its Flatpak is
installed (the command counts only where nothing can be asked, as on Solus);
a removal passes when the package is gone. The
running window's title says `FINISHED · VERIFIED`, the command line prints a
`Verified:` line for each package, and the history records `exit 0,
verified`. A run whose program succeeded but whose package cannot be found
is recorded as failed, with what was missing.
