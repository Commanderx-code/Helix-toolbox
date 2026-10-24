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
