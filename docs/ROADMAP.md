# Roadmap

## Done

- Cargo workspace: core, catalog, platform, engine and the application
- Catalog tree of tabs, groups and entries from TOML, embedded at build time
- Package, script and command entries; preconditions; flags; ordering
- Engine: plans, one shell program per run, pseudo-terminal session, history
- Terminal interface with the previous toolbox's layout and keys: tabs, groups,
  search across tabs, description and preview, confirmation, the run in an
  embedded terminal, multi-select queue, flags guide, two themes
- Command line: system, list, info, plan, run, history, docgen
- Tests for every crate, including screen tests of the interface

## Next

1. Bring the Commander installers across from the previous toolbox with their
   shared helpers and Python tests, one group at a time, verifying each in the
   interface.
2. Port the dotfiles and Myfish source sync: adopt tested revisions and
   regenerate their catalog entries.
3. Bring the general catalog across: package entries as data, the rest as
   scripts.
4. Releases: a static Linux x86_64 build with a checksum, and a launcher.
5. A dashboard screen and a history screen in the interface.
6. The graphical interface on the same engine.
