# Security

## Reporting a vulnerability

Please report security problems privately, not in a public issue: use
[**Report a vulnerability**](https://github.com/Commanderx-code/Helix-toolbox/security/advisories/new)
on the repository's Security tab, which only the maintainer can read.
Include what an attacker could do, the Helix Toolbox version
(`helix-toolbox --version`), and how to reproduce it. A fix goes into the next
release, and the advisory is published with it, crediting you unless you
would rather not be named.

Fixes go into the latest release only ([STABILITY.md](STABILITY.md)), so
update before reporting.

## Offline bundles

A bundle's manifest is checked before anything in it is used: plain text,
made for this distribution family and architecture, every file inside the
bundle under a plain name and matching its SHA-256, and every pacman package
with its signature, a detached one made only of signature packets. The
install copies the files into a new private folder
and checks the copies against the sums it verified, carried in the plan
itself, so a bundle changed after the check (on a shared drive, say) is not
installed. Each pacman package's signature is checked with `pacman-key
--verify` and each RPM's with `rpm -K` before the package manager runs; a
Debian package must match this machine's signed repository index unless
`--trust-bundle` is given. Flatpaks install from the bundle's copy of their
repository, and flatpak checks each commit's signature against the remote's
key on the installing machine; the bundle's copy of that key is used only
to add a remote the machine lacks, with `--trust-bundle`, and is checked
against the sum verified first. A collection ID is given to a remote that
has none only when it is the one published for the remote's address, or
with `--trust-bundle`. A bundle from someone else is only as
trustworthy as the Helix Toolbox copy inside it; prefer your own.

## How Helix Toolbox keeps runs safe

- **Trust boundary.** Only the catalog embedded in the binary runs. There is
  no remote catalog, no plugin loading and no downloaded script. Entries that
  fetch sources at run time pin a revision and say so in their description.
  The hidden `--catalog-dir` option, for tests with a fixture catalog, reads
  a catalog from a directory given on the command line; it gives nothing the
  person typing it could not already run, and no environment variable or
  file turns it on.
- **Unpacked copy.** The embedded catalog is unpacked into a private (0700)
  temporary directory that holds a lock for as long as its process lives.
  At start, Helix Toolbox removes only its own user's copies whose lock nobody
  holds; anything without the lock file is left alone.
- **No free text in the shell.** The run is a shell program assembled from
  single-quoted literals: validated package names, script paths from the
  unpacked catalog, Nix installables and command lines written in the catalog.
  Nothing typed by the user reaches it; keys typed into the running window go
  to the program's own standard input through the pseudo-terminal.
- **Privileges.** The application starts unprivileged and warns when it is
  root. Package entries call `sudo` inside the run, where the prompt is
  visible. Scripts ask for `sudo` themselves through the shared helpers.
  Nothing stays elevated between runs.
- **Fallback sources.** A package entry may be installed from a later
  source (the AUR, then Flathub) when an earlier one fails. Every source is
  in the plan before the confirmation, built from the same validated names,
  and each one's own `sudo` prompt is visible; a wrong password lets the next
  source ask again, and `Ctrl-C` stops the run rather than moving on.
- **Review.** Every plan shows its steps, its risk level and whether it asks
  for a password. The interface asks `[Y]`; the command line asks for the
  word `RUN` unless `--yes` is given deliberately.
- **Stopping.** `Ctrl-C` in the running window sends a hangup to the run's
  whole process group, not only its shell. Processes that ignore it are
  counted and shown, recorded in the history if the window is closed while
  they run, and killed by a second `Ctrl-C`. A command run through `sudo`
  gets the hangup from `sudo`, which passes it on; a process running as
  root cannot be killed by the second `Ctrl-C`, and stays counted.
- **Records.** Runs are appended to a private history file (mode 0600 in a
  0700 directory) with their outcome; interrupted runs are recorded as
  interrupted, never as success. A run whose Helix Toolbox died before it was
  recorded is listed as not finished, never as success. Run markers live in
  a private `running/` directory beside the history and name only the owning
  process. When the history cannot be written, the
  run still happens and Helix Toolbox says it was not recorded. Saved logs
  contain whatever the program printed, which can include file names; typed
  passwords are not echoed by the terminal and so are not in them. The
  tracing log records plans and outcomes, not output.
- **Verification.** After a run, package installs and removals are checked
  against the PATH, the package manager's own list and Flatpak's directories.
  The check reads; it changes nothing. It shows a package is present or gone,
  not that the package is genuine or works.
- **Releases.** Every published file has a SHA-256 sum and a signed build
  provenance attestation from the release workflow
  ([INSTALL.md](INSTALL.md) shows how to check both). The workflows pin
  every third-party action to a commit, and the packaging tool nfpm to a
  checksum, so a moved tag cannot change what builds a release.
- **Not provided.** Rollback, verification of scripts, commands and Nix
  entries beyond their exit status, and isolation of what an entry's script
  does once it runs.
- Text Helix Toolbox shows but did not write (journal lines, unit, file and
  backup names, error messages, paths found on the PATH) goes through
  `printable()`, which turns control characters into `?`, so it cannot start
  terminal escape sequences in the menu or on the command line. The programs
  that run keep the real names, quoted.
- Helix Doctor's permissions repair only names paths whose real location is
  exactly where they appear in the home: it does not look inside a folder
  that is a link or reached through one, and an item that resolves elsewhere
  is only mentioned. Just before `chown` runs, a guard checks each path again
  with `realpath` and stops, changing nothing, if one has moved or is reached
  through a link. `chown -R -h -P` then changes a link itself, never what it
  points at, and follows none while recursing, so the repair cannot hand a
  system file to the user. Backups are found
  without following links, and links are never offered for restore.
- An imported or saved profile whose name or description contains control
  characters is refused, and entry ids the catalog does not have are shown
  through `printable()` in the error, so a shared profile file cannot carry
  escape sequences into the terminal. The name typed in the menu accepts no
  control characters either.
