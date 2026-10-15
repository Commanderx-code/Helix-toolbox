# ⬡ HELIXFORGE

**BUILD • CONFIGURE • OPTIMIZE**

A from-scratch Linux utility platform in Rust, starting with a Ratatui terminal interface and designed for a future GUI. No Linutil code or shell-script framework is used.

> **v0.1 development scaffold, not a production installer.** Application catalog and basic CLI installation are implemented. The TUI is read-only: browse, search and preview. Test inside a VM before using package installation on an important computer.

## Requirements

- Linux, including Garuda/Arch, Fedora, Debian or Ubuntu
- Rust stable toolchain (Rust 1.88 or newer)
- Cargo, and a terminal for the TUI
- `sudo` plus the native package manager only when actually installing something

For Garuda / Arch:

```sh
sudo pacman -S --needed rustup base-devel
rustup default stable
```

If Rust is already installed, skip those commands. Ensure the active toolchain is Rust 1.88+.

## Run

```sh
cargo run -p helixforge
```

TUI shortcuts: `↑↓` or `j/k` to navigate, `/` to search, `Enter` or `p` to preview, `d` for read-only diagnostics, `Esc` to return, `q` to exit.

### CLI examples

```sh
cargo run -p helixforge -- system
cargo run -p helixforge -- list
cargo run -p helixforge -- info git
cargo run -p helixforge -- plan ripgrep
cargo run -p helixforge -- install ripgrep --dry-run
cargo run -p helixforge -- history
```

`install <id>` executes a native package-manager command **only after** the user types `INSTALL` to confirm. `--yes` bypasses the confirmation for deliberate automation. Neither `plan` nor `--dry-run` modifies the system. Package installation may request sudo authentication and package-manager confirmations. Install only tools you trust.

```sh
# Only run after reviewing the dry run:
cargo run -p helixforge -- install ripgrep
```

### Build and test

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release -p helixforge
./target/release/helixforge
```

When initially downloading the source, Cargo generates `Cargo.lock`. Commit the resulting lockfile before distributing reproducible builds or setting up CI using `--locked`.

## Architecture

```text
apps/helixforge/            CLI and Ratatui TUI binary
crates/helixforge-core/     Shared domain models
crates/helixforge-catalog/  TOML tool catalog and validation
crates/helixforge-platform/ Linux/distro detection
crates/helixforge-engine/   Read-only plans and guarded execution
catalog/applications/      Auditable bundled TOML entries
```

The GUI will be a separate frontend calling these shared crates; it must not duplicate installers. Read [Architecture](docs/ARCHITECTURE.md), [Roadmap](docs/ROADMAP.md) and [Security](docs/SECURITY.md).

## Design limits

- Installation supports six built-in tools: Git, Ripgrep, fzf, Neovim, htop and btop.
- Executable detection uses PATH; it is **not** yet an authoritative package database check.
- Installing uses the system package manager; packages may not exist on every distro release.
- The TUI does not yet execute installs; use the CLI after reviewing a plan.
- Execution history records subprocess outcomes, not guaranteed application verification.
- No plugin system, external script execution, automatic rollback, bootloader editing, or system tuning is implemented.
- This scaffold has not yet been compiled in the generation environment. Run the checks above on a machine with Rust installed.

## License

MIT. See [LICENSE](LICENSE).
