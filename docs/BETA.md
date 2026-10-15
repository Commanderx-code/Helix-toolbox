# Beta testing

HelixForge v0.7 is the beta before v1.0, the first release meant for
everyone. Before then it needs to have run on more machines than its
developer's: other distributions, other terminals, other hardware, and
people who use it differently. This page says what to try, how to try it
without risking a machine you depend on, and what makes a report useful.

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
6. **Your terminal.** Does the menu draw correctly in it: colours, icons,
   the banner, resizing, small windows? Try the themes (`t`) and Settings'
   icons switch.
7. **The command line and JSON**, if you script things
   ([JSON.md](JSON.md)).

Distributions it is tested on in CI: Arch, Debian, Ubuntu and Fedora.
Reports from derivatives (Garuda, CachyOS, EndeavourOS, Manjaro, Mint,
Pop!_OS), openSUSE and NixOS with Home Manager are especially useful.

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

A run that went wrong is easier to fix with the exact entry, its log and the
report than with any description. Ideas and missing entries are welcome too:
the second issue form asks for package names and install instructions.

## Before v1.0

v1.0 follows when the beta has run on several distributions and terminals
with no open problems that lose data or leave a machine half-configured, and
the documentation answers the questions testers asked.
