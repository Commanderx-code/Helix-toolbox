# Security notes

- **Trust boundary.** Only the catalog embedded in the binary runs. There is
  no remote catalog, no plugin loading and no downloaded script. Entries that
  fetch sources at run time pin a revision and say so in their description.
- **No free text in the shell.** The run is a shell program assembled from
  single-quoted literals: validated package names, script paths from the
  unpacked catalog, and command lines written in the catalog. Nothing typed by
  the user reaches it; keys typed into the running window go to the program's
  own standard input through the pseudo-terminal.
- **Privileges.** The application starts unprivileged and warns when it is
  root. Package entries call `sudo` inside the run, where the prompt is
  visible. Scripts ask for `sudo` themselves through the shared helpers.
- **Confirmation.** The interface shows the plan and asks `[Y]`; the command
  line asks for the word `RUN` unless `--yes` is given deliberately.
- **Records.** Runs are appended to a private history file (mode 0600 in a
  0700 directory). Saved logs contain whatever the program printed, which can
  include file names; they never contain typed passwords, which the terminal
  does not echo.
- **Not provided.** Rollback, package verification beyond the package
  manager's own, and isolation of what an entry's script does once it runs.
