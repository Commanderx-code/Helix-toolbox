# Security notes

- **Trust boundary.** Only the catalog embedded in the binary runs. There is
  no remote catalog, no plugin loading and no downloaded script. Entries that
  fetch sources at run time pin a revision and say so in their description.
  The hidden `--catalog-dir` option, for tests with a fixture catalog, reads
  a catalog from a directory given on the command line; it gives nothing the
  person typing it could not already run, and no environment variable or
  file turns it on.
- **Unpacked copy.** The embedded catalog is unpacked into a private (0700)
  temporary directory that holds a lock for as long as its process lives.
  At start, HelixForge removes only its own user's copies whose lock nobody
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
  interrupted, never as success. When the history cannot be written, the
  run still happens and HelixForge says it was not recorded. Saved logs
  contain whatever the program printed, which can include file names; typed
  passwords are not echoed by the terminal and so are not in them. The
  tracing log records plans and outcomes, not output.
- **Not provided.** Rollback, package verification beyond the package
  manager's own, and isolation of what an entry's script does once it runs.
