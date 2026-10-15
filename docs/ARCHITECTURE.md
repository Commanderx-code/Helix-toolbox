# HELIXFORGE Architecture — v0.1

## Design principles

1. Frontends (TUI now, GUI later) orchestrate use cases; they never own platform mutations.
2. The catalog uses trusted, validated, built-in TOML metadata. Arbitrary downloaded scripts are not supported.
3. Core models, OS detection, planning and execution are separate crates.
4. Every install has a user-visible plan and opt-in execution.
5. Privileged commands use argument arrays through `std::process::Command`, never `sh -c`.
6. Commands inherit the terminal for sudo and package-manager prompts. No invisible elevation.
7. We will add configuration backups only after we implement validation, testing and restoration semantics.

## Flow

```text
                         +--------------------+
+-------------+          |   helixforge-core  |
| Ratatui TUI | -------->| domain API & types |
+-------------+          +--------------------+
        |                    ^         ^
        | (preview only)     |         |
+-------------+     +--------+---+   +-+-----------------+
| CLI         |---->| catalog    |   | platform          |
+-------------+     | definitions|   | distro detection  |
        |           +------------+   +-------------------+
        v
+------------------+
| engine           |
| plan -> confirm  |
| -> execute       |
| -> record        |
+------------------+
        |
        v
Native package managers, never a shell interpreter
```

## Platform matrix

- Arch-family: `pacman -S --needed <package>`
- Debian-family: `apt-get install <package>`
- Fedora-family: `dnf install <package>`

`sudo` runs the command in the CLI after user consent; the package manager may request more confirmation. Unsupported distros can use read-only TUI/CLI functionality.

## Planned extension interfaces

- `Action` containing validated, typed operation parameters
- `Plan` containing ordered steps and impact preview
- `Runner` abstraction for actual and test executors
- `ExecutionEvent` stream to report progress to TUI or GUI
- `PlatformCapabilities` for distro feature discovery
- `ConfigBackend` for backup, diff, apply and restoration

Only `Runner` and basic `Plan` exist in v0.1. The remaining interfaces are deliberately not invented as unusable stubs.

## GUI plan

Tauri 2 frontend can invoke a thin command layer in `apps/helixforge-gui` while reusing the same crates. Introduce async execution/progress streams then, preserving the command planner and permissions model. Do not ship a permanently privileged daemon.
