# Security model for the development scaffold

- **Trust boundary:** only six bundled, audited package manifests are loaded. User-supplied manifests and arbitrary scripts are not enabled.
- **Injection protection:** package names and detected executable names are constrained, and `std::process::Command` receives explicit argument arrays. Shell interpolation is never used.
- **Privilege:** browsing, previewing and system checks are unprivileged. Install CLI commands use `sudo` only after an explicit confirmation phrase (or the conscious `--yes` flag).
- **Dry-run:** `plan` and `install --dry-run` do not invoke the package manager.
- **Scope:** only a single named native package is installed in each operation. No system-wide upgrades, boot edits, service stops or partition actions.
- **History:** an append-only JSON-lines file under `$XDG_STATE_HOME/helixforge` or `~/.local/state/helixforge`; it does not intentionally record passwords.
- **Not guaranteed:** native package managers can still run scripts and perform system changes; review package trust. The plan is a summary of the invoked command, not a complete downstream package transaction. Rollback is not provided.
- **Risks to tackle next:** package database verification, secure state-directory handling, subprocess timeouts/cancellation, package provenance, process and output streaming, detailed error metadata, TUI privilege handoff, and end-to-end validation.

Do not use this scaffold as a privileged service or load arbitrary external TOML or scripts without a dedicated trust, permissions, and provenance design.
