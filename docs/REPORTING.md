# Testing and reporting problems

HelixForge is tested in containers of six distributions on every change, but
real machines differ: other derivatives, terminals, hardware, and people who
use it differently. Reports from them are what make it better. This page
says what to try, how to try it without risking a machine you depend on, and
what makes a report useful.

## Testing safely

HelixForge changes real systems: it installs packages, edits configuration
and can touch boot settings. While testing:

- **Use a spare machine or a virtual machine** where you can, especially for
  entries flagged `K` (kernel), `D` (disks) or `SI` (system integrity); `g`
  explains the flags.
- **Look before running.** `d` describes an entry, `p` shows what it runs,
  and the Install Plan and the confirmation show the plan. On the command
  line, `--dry-run` shows the plan and changes nothing.
- **Take snapshots.** With Snapper or Timeshift set up, turn on snapshots in
  Settings; HelixForge takes one before a run that changes system files.
- **Read-only parts are always safe:** browsing, search, status, Helix
  Doctor's checks, Updates without `u`, History, `helixforge report`.

## What to test

Anything is welcome. These need the most eyes:

1. **Installing it.** The package for your distribution
   ([INSTALL.md](INSTALL.md)): does it install, does `man helixforge` work,
   do completions work in your shell, does the menu entry open it?
2. **Status.** Open Applications: is what you have installed shown as
   installed, from the right place, at the right version? Entries you have
   from Flatpak or Nix? Anything shown wrongly is a bug.
3. **Installing entries.** Queue a few applications you do not have yet and
   run the plan. Did each install, and did HelixForge verify it?
4. **Updates.** Does the Updates screen match what your package manager
   says (`checkupdates`, `apt list --upgradable`, `flatpak remote-ls
   --updates`)?
5. **Helix Doctor.** Do the checks describe your machine correctly? Did a
   check call something a problem that is not one, or miss one that is?
6. **Your terminal.** Does the menu draw correctly in it: colors, icons,
   the banner, resizing, small windows? Try the themes (`t`), Settings'
   icons switch, and `NO_COLOR=1 helixforge`.
7. **The command line and JSON**, if you script things
   ([JSON.md](JSON.md)).

Distributions it is tested on in CI: Arch, Debian, Ubuntu 22.04 and 24.04,
Fedora and openSUSE Tumbleweed. Reports from derivatives (Garuda, CachyOS,
EndeavourOS, Manjaro, Mint, Pop!_OS), openSUSE Leap and NixOS with Home
Manager are especially useful.

## Reporting

Open an issue on GitHub; the form asks for:

- **What happened**, and what you expected instead, naming the entry or
  screen.
- **How to make it happen again**, step by step.
- **What the run printed.** In the running window, `l` saves the log, and
  History lists saved logs. Leave out anything private.
- **`helixforge report`.** It prints the version, how HelixForge was
  installed, the distribution, the tools it found, the terminal, your
  settings, the last few runs and the health checks. It shows your home
  folder as `~` and includes no host name or logs, but read it before
  posting.

A run that went right is worth reporting too, above all for the boot,
login-screen and driver entries, which no container can test: name the
entry, the system and graphics hardware, the HelixForge version and the date,
and it becomes a **Hardware Tested** or **VM Tested** record in
`catalog/validation.toml`, shown on the Reliability screen to everyone.

A run that went wrong is easier to fix with the exact entry, its log and the
report than with any description. Ideas and missing entries are welcome too:
the second issue form asks for package names and install instructions.

## What is promised

[STABILITY.md](STABILITY.md) lists what stays the same across 1.x: the
commands, the JSON, your files and entry ids. Something that breaks one of
those is a bug worth reporting even when nothing else went wrong.
