# Installing HelixForge

Every release on the
[releases page](https://github.com/Commanderx-code/Helix-Forge/releases)
has the same tested Linux x86_64 build in four forms: an Arch package, a
Debian/Ubuntu package, an RPM package for Fedora and openSUSE, and the bare
program. It needs glibc 2.35 or newer (Debian 12, Ubuntu 22.04, Fedora 36,
current Arch and openSUSE Tumbleweed, or later).

## A package

A package puts `helixforge` in `/usr/bin`, with shell completions for bash,
zsh and fish, manual pages (`man helixforge`), and a menu entry that opens
it in a terminal. Your package manager then knows it and removes it cleanly.
Download the file for your distribution from the release, check it (below),
then:

```sh
sudo pacman -U helixforge-0.7.0-1-x86_64.pkg.tar.zst     # Arch, Garuda, CachyOS, Manjaro
sudo apt install ./helixforge_0.7.0-1_amd64.deb          # Debian, Ubuntu, Mint, Pop!_OS
sudo dnf install ./helixforge-0.7.0-1.x86_64.rpm         # Fedora
sudo zypper install --allow-unsigned-rpm ./helixforge-0.7.0-1.x86_64.rpm   # openSUSE
```

The packages are not signed with a distribution key; zypper needs
`--allow-unsigned-rpm` to accept that. Check the file first instead (below).

A newer release is installed the same way, over the old one. Remove it with
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
gh attestation verify helixforge_0.7.0-1_amd64.deb --repo Commanderx-code/Helix-Forge
```

It succeeds only for a file built by this repository's release workflow,
and says which workflow run built it.

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
