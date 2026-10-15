# HELIXFORGE Milestones

## Milestone 1 — Bootstrap (this scaffold)

- [x] Cargo workspace: app + core, catalog, platform, engine crates
- [x] Ratatui terminal shell with catalog, search, preview, doctor
- [x] CLI with `list`, `info`, `system`, `plan`, `install`, `history`
- [x] Built-in TOML entries and validation
- [x] Garuda/Arch, Debian/Ubuntu, Fedora family identification
- [x] Explicit CLI confirmation before privileged package operations
- [x] Unit tests and CI workflow (execution pending verification on Rust toolchain)
- [ ] Compile, test and generate/commit `Cargo.lock` on a Rust-equipped system
- [ ] Manual testing in disposable VMs

## Milestone 2 — Real install workflow inside TUI

- [ ] Plan confirmation dialog in TUI
- [ ] Safe terminal suspend/resume for sudo and interactive package-manager prompts
- [ ] Runner event stream for progress / status / failure
- [ ] Accurate installed-state detection from package database
- [ ] Expanded package mappings and CI build artifacts

## Milestone 3 — Cross-distro reliability

- [ ] Container/VM integration tests for pacman, apt and dnf
- [ ] Native-package vs Flatpak source selection
- [ ] Download integrity / provenance policies for external assets
- [ ] System-level failure diagnosis with actionable logs

## Milestone 4 — Configuration & profiles

- [ ] Configuration previews and diffs
- [ ] Versioned backups and verifiable restoration
- [ ] Home Manager / Nix-owned configuration safeguards
- [ ] Opt-in installation profiles and system doctor checks

## Milestone 5 — GUI

- [ ] Tauri 2 frontend sharing core services
- [ ] Async task + progress subscription API
- [ ] Accessible UI and multi-distro packaging
