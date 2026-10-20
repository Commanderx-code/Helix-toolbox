# Installing Helix Toolbox

Every release on the
[releases page](https://github.com/Commanderx-code/Helix-toolbox/releases)
has the same tested build for Linux on x86_64 and on aarch64 (ARM64), each in
four forms: an Arch package, a Debian/Ubuntu package, an RPM package for
Fedora and openSUSE, and the bare program, and the window beside them
(below). With Rust installed, `cargo install` builds it on your machine
instead (below). It needs glibc 2.35 or newer (Debian 12, Ubuntu 22.04, Fedora 36,
current Arch and openSUSE Tumbleweed, or later).

## A package

A package puts `helix-toolbox` in `/usr/bin`, with shell completions for bash,
zsh and fish, manual pages (`man helix-toolbox`), and a menu entry that opens
it in a terminal. Your package manager then knows it and removes it cleanly.
Download the file for your distribution from the release, check it (below),
then:

```sh
sudo pacman -U helix-toolbox-1.7.0-1-x86_64.pkg.tar.zst     # Arch, Garuda, CachyOS, Manjaro
sudo apt install ./helix-toolbox_1.7.0-1_amd64.deb          # Debian, Ubuntu, Mint, Pop!_OS
sudo dnf install ./helix-toolbox-1.7.0-1.x86_64.rpm         # Fedora
sudo zypper install --allow-unsigned-rpm ./helix-toolbox-1.7.0-1.x86_64.rpm   # openSUSE
```

On ARM64 (a Raspberry Pi 4 or 5 with a 64-bit system, an Apple Silicon Mac
with Fedora Asahi Remix, an ARM laptop or server):

```sh
sudo pacman -U helix-toolbox-1.7.0-1-aarch64.pkg.tar.zst    # Arch Linux ARM
sudo apt install ./helix-toolbox_1.7.0-1_arm64.deb          # Debian, Ubuntu, Raspberry Pi OS
sudo dnf install ./helix-toolbox-1.7.0-1.aarch64.rpm        # Fedora, Fedora Asahi Remix
sudo zypper install --allow-unsigned-rpm ./helix-toolbox-1.7.0-1.aarch64.rpm  # openSUSE
```

Entries for programs built only for x86_64, such as Steam, Google Chrome,
DaVinci Resolve and the CachyOS and Chaotic-AUR repositories, are not shown
on ARM64.

The packages are not signed with a distribution key; zypper needs
`--allow-unsigned-rpm` to accept that. Check the file first instead (below).

A newer release is installed the same way, over the old one, or from inside
Helix Toolbox: the **Helix Toolbox** row of the Updates screen downloads it, checks
it and installs it for you (`helix-toolbox updates --online --apply helix-toolbox`),
the window's package with it when that is installed. On Arch, the repository
below does away with the downloads. Remove it with
`sudo pacman -R helix-toolbox`, `sudo apt remove helix-toolbox`, `sudo dnf remove
helix-toolbox` or `sudo zypper remove helix-toolbox`; your settings, profiles and
history in `~/.config/helix-toolbox` and `~/.local/state/helix-toolbox` stay.

## The Arch repository

Arch and its derivatives can take both packages from a pacman repository
instead, which every release rebuilds on GitHub Pages. Then `pacman -Syu`,
`garuda-update` or whatever updates the system updates Helix Toolbox and its
window too, and the Updates screen's **Helix Toolbox** row runs that. Add it
to `/etc/pacman.conf`, after the distribution's own repositories:

```ini
[helix-toolbox]
SigLevel = Optional TrustAll
Server = https://commanderx-code.github.io/Helix-toolbox/$arch
```

Then:

```sh
sudo pacman -Syu helix-toolbox helix-toolbox-gui   # or helix-toolbox alone
```

The repository carries x86_64 and aarch64 packages and only the latest
release. Its packages are the release's own files, so the checksums and
build attestations on the release page apply to them; the repository itself
is not signed, which the `SigLevel` line tells pacman. A package installed
from a downloaded file is taken over by the repository at its next version.

## The window

`helix-toolbox-gui` opens the same catalog in a window ([GUI.md](GUI.md)).
Each release has it for x86_64 and aarch64 as an Arch package, a Debian
package, an RPM and an AppImage. Install the `helix-toolbox` package above
first: the window runs the command line for the built-in steps, and the
packages require it.

```sh
sudo pacman -U helix-toolbox-gui-1.10.0-1-x86_64.pkg.tar.zst   # Arch and derivatives (aarch64 on ARM64), or the repository above
sudo apt install ./helix-toolbox-gui_1.10.0_amd64.deb          # Debian, Ubuntu (arm64 on ARM64)
sudo dnf install ./helix-toolbox-gui-1.10.0-1.x86_64.rpm       # Fedora (aarch64 on ARM64)
sudo zypper install --allow-unsigned-rpm ./helix-toolbox-gui-1.10.0-1.x86_64.rpm   # openSUSE
chmod +x helix-toolbox-gui_1.10.0_amd64.AppImage && ./helix-toolbox-gui_1.10.0_amd64.AppImage   # any distribution
```

The Updates screen's **Helix Toolbox** row updates the window's package with
the command line's, and replaces an AppImage in place.

The packages add a menu entry, **Helix Toolbox**. The window is drawn by
WebKitGTK 4.1, which the Debian and RPM packages install with it; the
AppImage carries its libraries. It is not on crates.io: building it needs
the frontend, as [GUI.md](GUI.md) describes.

## Without installing

`start.sh` in this repository downloads the latest release's program, checks
it against the release's `SHA256SUMS`, and runs it, without installing
anything:

```sh
curl -fsSL https://raw.githubusercontent.com/Commanderx-code/Helix-toolbox/main/start.sh | sh
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
gh attestation verify helix-toolbox_1.0.0-1_amd64.deb --repo Commanderx-code/Helix-toolbox
```

It succeeds only for a file built by this repository's release workflow,
and says which workflow run built it.

## With Cargo

With Rust 1.89 or newer ([rustup](https://rustup.rs)), Cargo builds
Helix Toolbox on your machine and puts it in `~/.cargo/bin`, with `htb`
beside it:

```sh
cargo install --locked helix-toolbox
```

That takes the latest release from [crates.io](https://crates.io/crates/helix-toolbox).
Straight from GitHub instead, a release by its tag or the newest code:

```sh
cargo install --locked --git https://github.com/Commanderx-code/Helix-toolbox --tag v1.7.0 helix-toolbox
cargo install --locked --git https://github.com/Commanderx-code/Helix-toolbox helix-toolbox
```

`cargo install` again updates it (the Updates screen's Helix Toolbox row runs it
for you), and `cargo install-update -a` from the
[cargo-update](https://crates.io/crates/cargo-update) crate updates every
program installed this way; Cargo never updates anything by itself.
`cargo uninstall helix-toolbox` removes it.
A copy built this way has no manual pages or menu entry (the packages have
those); `helix-toolbox completions bash|zsh|fish` prints its shell completions.
A build from GitHub reports the version in `Cargo.toml`, which can be a
patch behind the tag it was built from; the crates.io and release builds
report the release's own.

## Coming from HelixForge

HelixForge is Helix Toolbox from 1.7. A HelixForge package updates to the
`helix-toolbox` package from the Updates screen, or by installing the new
package as above: it replaces `helixforge` and keeps a `helixforge` command
that runs Helix Toolbox. A Cargo install moves with
`cargo install --locked helix-toolbox` (then `cargo uninstall helixforge`).
Settings, profiles and history move to their new folders by themselves the
first time it starts; [MIGRATION.md](MIGRATION.md) lists what changed.

## From source

```sh
git clone https://github.com/Commanderx-code/Helix-toolbox.git
cd Helix-toolbox
cargo build --release --locked -p helix-toolbox
./target/release/helix-toolbox
```

`scripts/package.sh VERSION target/release/helix-toolbox dist` makes the three
packages from such a build, with [nfpm](https://nfpm.goreleaser.com) on the
PATH.
