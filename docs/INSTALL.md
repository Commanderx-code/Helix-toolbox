# Installing HelixForge

Every release on the
[releases page](https://github.com/Commanderx-code/Helix-Forge/releases)
has the same tested build for Linux on x86_64 and on aarch64 (ARM64), each in
four forms: an Arch package, a Debian/Ubuntu package, an RPM package for
Fedora and openSUSE, and the bare program. With Rust installed, `cargo install` builds it on your machine
instead (below). It needs glibc 2.35 or newer (Debian 12, Ubuntu 22.04, Fedora 36,
current Arch and openSUSE Tumbleweed, or later).

## A package

A package puts `helixforge` in `/usr/bin`, with shell completions for bash,
zsh and fish, manual pages (`man helixforge`), and a menu entry that opens
it in a terminal. Your package manager then knows it and removes it cleanly.
Download the file for your distribution from the release, check it (below),
then:

```sh
sudo pacman -U helixforge-1.6.0-1-x86_64.pkg.tar.zst     # Arch, Garuda, CachyOS, Manjaro
sudo apt install ./helixforge_1.6.0-1_amd64.deb          # Debian, Ubuntu, Mint, Pop!_OS
sudo dnf install ./helixforge-1.6.0-1.x86_64.rpm         # Fedora
sudo zypper install --allow-unsigned-rpm ./helixforge-1.6.0-1.x86_64.rpm   # openSUSE
```

On ARM64 (a Raspberry Pi 4 or 5 with a 64-bit system, an Apple Silicon Mac
with Fedora Asahi Remix, an ARM laptop or server):

```sh
sudo pacman -U helixforge-1.6.0-1-aarch64.pkg.tar.zst    # Arch Linux ARM
sudo apt install ./helixforge_1.6.0-1_arm64.deb          # Debian, Ubuntu, Raspberry Pi OS
sudo dnf install ./helixforge-1.6.0-1.aarch64.rpm        # Fedora, Fedora Asahi Remix
sudo zypper install --allow-unsigned-rpm ./helixforge-1.6.0-1.aarch64.rpm  # openSUSE
```

Entries for programs built only for x86_64, such as Steam, Google Chrome,
DaVinci Resolve and the CachyOS and Chaotic-AUR repositories, are not shown
on ARM64.

The packages are not signed with a distribution key; zypper needs
`--allow-unsigned-rpm` to accept that. Check the file first instead (below).

A newer release is installed the same way, over the old one, or from inside
HelixForge: the **HelixForge** row of the Updates screen downloads it, checks
it and installs it for you (`helixforge updates --online --apply helixforge`). Remove it with
`sudo pacman -R helixforge`, `sudo apt remove helixforge`, `sudo dnf remove
helixforge` or `sudo zypper remove helixforge`; your settings, profiles and
history in `~/.config/helixforge` and `~/.local/state/helixforge` stay.

## Without installing

`start.sh` in this repository downloads the latest release's program, checks
it against the release's `SHA256SUMS`, and runs it, without installing
anything:

```sh
curl -fsSL https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/start.sh | sh
```

From a checkout with Rust installed, `./start.sh` builds and runs the source
instead.

## Checking a download

Each release lists the SHA-256 sum of every file in `SHA256SUMS`:

```sh
sha256sum --check --ignore-missing SHA256SUMS
```

That shows the file arrived as published. To check where it was built, each
file also carries a signed build provenance attestation from GitHub Actions:
which repository, workflow and commit produced it. With the GitHub command
line:

```sh
gh attestation verify helixforge_1.0.0-1_amd64.deb --repo Commanderx-code/Helix-Forge
```

It succeeds only for a file built by this repository's release workflow,
and says which workflow run built it.

## With Cargo

With Rust 1.89 or newer ([rustup](https://rustup.rs)), Cargo builds
HelixForge on your machine and puts it in `~/.cargo/bin`:

```sh
cargo install --locked helixforge
```

That takes the latest release from [crates.io](https://crates.io/crates/helixforge).
Straight from GitHub instead, a release by its tag or the newest code:

```sh
cargo install --locked --git https://github.com/Commanderx-code/Helix-Forge --tag v1.0.1 helixforge
cargo install --locked --git https://github.com/Commanderx-code/Helix-Forge helixforge
```

`cargo install` again updates it (the Updates screen's HelixForge row runs it
for you); `cargo uninstall helixforge` removes it.
A copy built this way has no manual pages or menu entry (the packages have
those); `helixforge completions bash|zsh|fish` prints its shell completions.
A build from GitHub reports the version in `Cargo.toml`, which can be a
patch behind the tag it was built from; the crates.io and release builds
report the release's own.

## From source

```sh
git clone https://github.com/Commanderx-code/Helix-Forge.git
cd Helix-Forge
cargo build --release --locked -p helixforge
./target/release/helixforge
```

`scripts/package.sh VERSION target/release/helixforge dist` makes the three
packages from such a build, with [nfpm](https://nfpm.goreleaser.com) on the
PATH.
