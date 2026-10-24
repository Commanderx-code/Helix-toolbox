# Glossary

The words Helix Toolbox uses for its own concepts, and what each means here.

**Display row**:
One line of a screen as both interfaces show it, built by the engine's
`rows` module from what the engine found: a label and its detail, the same
on one line for the terminal interface, and whether it wants attention.
The terminal interface draws display rows and the window's `views.rs`
serializes them; neither works out display text of its own.
_Avoid_: view model, row text.

**Attention**:
A display row's mark that something is worth a look: a sensor at its high
mark or within five degrees of critical, a battery under 70% of its design
capacity, a firmware update waiting. Each interface shows it in its warning
color.

**Prepared plan**:
A plan made ready to confirm by the engine's `confirmation::prepare`, the
same for the command line, the terminal interface and the window: the
plan as it would run (the snapshot first where Settings wants one and the
run is worth it, the choice after a failed step applied), what the checks
before a run found, and whether there is nothing to run, something blocks
it, or it is ready with the lines to confirm.
_Avoid_: review, checked plan.

**Run session**:
One confirmed plan's run, from the user's yes to its record: the engine's
`session::RunSession`. It starts the program through a runner, follows it
as execution events, concludes it once (the steps, the checks, the checks
kept for after the reboot), writes the record, and then says which queued
entries remain and what an interface should read again. Every interface
runs plans through one.
_Avoid_: execution, run handle.

**Runner**:
Where a run session's program runs: a pseudo-terminal (`PtyRunner`, for
the terminal interface and the window), the caller's own terminal
(`InheritedRunner`, for the command line), or nowhere (`MockRunner`, for
tests). Everything else about a run is the session's.
