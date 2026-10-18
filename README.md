<p align="center">
  <img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/banner.png" width="640" alt="HelixForge: Linux setup, utilities, tooling">
</p>

<p align="center"><strong>BUILD • CONFIGURE • OPTIMIZE</strong></p>

<p align="center">
  <a href="https://github.com/Commanderx-code/Helix-Forge/actions/workflows/ci.yml"><img src="https://github.com/Commanderx-code/Helix-Forge/actions/workflows/ci.yml/badge.svg" alt="Checks"></a>
  <a href="https://github.com/Commanderx-code/Helix-Forge/actions/workflows/distributions.yml"><img src="https://github.com/Commanderx-code/Helix-Forge/actions/workflows/distributions.yml/badge.svg" alt="Distributions"></a>
  <a href="https://github.com/Commanderx-code/Helix-Forge/releases/latest"><img src="https://img.shields.io/github/v/release/Commanderx-code/Helix-Forge?label=release" alt="Latest release"></a>
  <a href="https://crates.io/crates/helixforge"><img src="https://img.shields.io/crates/v/helixforge" alt="crates.io"></a>
  <a href="https://github.com/Commanderx-code/Helix-Forge/blob/main/LICENSE"><img src="https://img.shields.io/github/license/Commanderx-code/Helix-Forge" alt="MIT license"></a>
</p>

**HelixForge** sets up and looks after a Linux machine from one menu in the
terminal: applications, dotfiles and shell configuration, system tweaks,
updates, backups and health checks. Every change is planned first, shown to
you with its risk, run only after you confirm it, verified afterward and
recorded in a history. It is written in Rust and works the same way on Arch,
Debian, Ubuntu, Fedora and openSUSE.

<p align="center">
  <img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/screenshots/applications.png" width="820" alt="The Applications screen, showing which entries are installed, from where and at which version">
</p>

## Highlights

- **A catalog of 248 entries** for applications, development tools, gaming,
  desktop and shell configuration, maintenance and security, in searchable
  groups, with ready-made profiles that set up a whole workstation.
- **Knows your machine.** Each entry shows whether it is installed, from
  where (pacman, apt, dnf, zypper, the AUR, Flatpak) and at which version,
  whether an update is waiting, and whether Nix or Home Manager provides it.
- **Plans before it acts.** Queue what you want; the Install Plan shows the
  steps in order, what they need, the risk and whether they ask for your
  password, and checks for conflicts, missing repositories, wrong hardware
  and disk space before anything runs.
- **Verified runs.** Installs and removals are checked against the package
  manager afterward; a failing source falls back to the next (the AUR,
  then Flathub); every run, its output and its result are kept in History.
- **One place for updates** from every package tool on the machine,
  delegated to those tools.
- **Helix Doctor** checks the network, services, graphics drivers, package
  databases, permissions, boot images, GRUB, the EFI partition and disk
  space, with repairs that run after their own confirmation.
- **Backups and snapshots**: configuration backups can be restored from one
  screen, and a Snapper or Timeshift snapshot is taken before system changes.
- **A setup wizard** on the first start: it looks at the machine and turns
  a choice of Developer, Gaming, Privacy or Minimal into an Install Plan to
  review.
- **The desktop**: the desktop, login screen and look at a glance, saved
  looks to put back, and a warning where Home Manager or Stylix sets them.
- **HELIX Drift** saves how a machine is set up and shows what has moved
  since, or what a fresh installation still lacks, with the entries that put
  it back queued for review.
- **A command palette** (`Ctrl-K`) reaches every screen, action and entry by
  name.
- **See a change before it happens**: `e` shows what a plan will change on
  this machine, configuration files side by side as the installer would
  write them, and marks what cannot be known in advance.
- **Hardware and firmware**: fwupd firmware updates, disk, temperature and
  battery health, PCI and USB devices, with SMART checks and firmware
  updates behind their own confirmation.
- **Offline bundles**: packages, installed Flatpaks and HelixForge in one
  checked folder, for a machine without reliable internet.
- **Know what you are running**: the Reliability screen shows how far each
  entry has been tested (in distribution containers, with stand-ins, on real
  machines), critical boot and driver entries first, and the trust inspector
  shows where an installer comes from, what checks its packages and what it
  downloads unchecked.
- **Scriptable**: everything is also a command, with `--dry-run` and stable
  `--json` output.

<table>
  <tr>
    <td><img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/screenshots/dashboard.png" alt="Dashboard"></td>
    <td><img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/screenshots/plan.png" alt="Install Plan"></td>
  </tr>
  <tr>
    <td align="center">Dashboard</td>
    <td align="center">Install Plan</td>
  </tr>
  <tr>
    <td><img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/screenshots/doctor.png" alt="Helix Doctor"></td>
    <td><img src="https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/assets/screenshots/updates.png" alt="Updates"></td>
  </tr>
  <tr>
    <td align="center">Helix Doctor</td>
    <td align="center">Updates</td>
  </tr>
</table>

## Install

Download the package for your distribution from the
[latest release](https://github.com/Commanderx-code/Helix-Forge/releases/latest),
then:

| System | Command |
| :--- | :--- |
| Arch, Garuda, CachyOS, EndeavourOS, Manjaro | `sudo pacman -U helixforge-*-x86_64.pkg.tar.zst` |
| Debian, Ubuntu, Mint, Pop!_OS | `sudo apt install ./helixforge_*_amd64.deb` |
| Fedora | `sudo dnf install ./helixforge-*.x86_64.rpm` |
| openSUSE | `sudo zypper install --allow-unsigned-rpm ./helixforge-*.x86_64.rpm` |
| Debian, Ubuntu, Raspberry Pi OS on ARM64 | `sudo apt install ./helixforge_*_arm64.deb` |
| Fedora, Fedora Asahi Remix on ARM64 | `sudo dnf install ./helixforge-*.aarch64.rpm` |
| openSUSE on ARM64 | `sudo zypper install --allow-unsigned-rpm ./helixforge-*.aarch64.rpm` |
| Any distribution, with Rust | `cargo install --locked helixforge` |

The packages add shell completions, manual pages and a menu entry. Every
release file has a SHA-256 sum and a signed build provenance attestation;
[INSTALL.md](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/INSTALL.md)
shows how to check them, and how to build from source.

To try it without installing anything, run the latest release straight from
GitHub, as your normal user:

```sh
curl -fsSL https://raw.githubusercontent.com/Commanderx-code/Helix-Forge/main/start.sh | sh
```

It needs Linux on x86_64 or aarch64 (ARM64) with glibc 2.35 or newer.

## Quick start

Run `helixforge` as your normal user; entries that need root ask for your
password when they run.

| Key | Does |
| :--- | :--- |
| `Tab` / `Shift-Tab`, `j` / `k` | Move between screens, or within a list |
| `Enter` | Open a group, run an entry, or run the Install Plan |
| `Space` | Add the entry to the Install Plan |
| `/` | Search every area |
| `d` / `p` | Describe the entry / show exactly what it runs |
| `u` / `U` | Update the entry / every entry with an update |
| `*` | Pin to Favorites |
| `Ctrl-K` | Find any screen, action or entry by name |
| `t` | Change the theme |
| `?` | Every key |

From the command line:

```sh
helixforge search editor             # find entries
helixforge status                    # what is installed, updated or managed
helixforge run git ripgrep --dry-run # the plan, nothing changed
helixforge updates --online          # what every package tool has waiting
helixforge doctor                    # the health checks
helixforge report                    # what a bug report needs
```

## Documentation

| | |
| :--- | :--- |
| [User guide](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/GUIDE.md) | The screens and the common jobs, step by step |
| [Reference](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/REFERENCE.md) | Everything the menu and the commands do, in detail |
| [Catalog and plans](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/CATALOG.md) | How entries are written, and how a run is planned and checked |
| [JSON output](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/JSON.md) | The `--json` shapes, for scripts and other programs |
| [Installing](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/INSTALL.md) | Packages, Cargo, checking downloads, building from source |
| [Stability](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/STABILITY.md) | What stays the same across 1.x |
| [Security](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/SECURITY.md) | How HelixForge runs things safely, and how to report a vulnerability |
| [Reporting problems](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/REPORTING.md) | Testing safely and writing a useful report |
| [Changelog](https://github.com/Commanderx-code/Helix-Forge/blob/main/CHANGELOG.md) | What each release brought |
| [Roadmap](https://github.com/Commanderx-code/Helix-Forge/blob/main/docs/ROADMAP.md) | What is done and what comes next |

## Supported systems

Every change is tested in containers of **Arch**, **Debian**, **Ubuntu 22.04
and 24.04**, **Fedora** and **openSUSE Tumbleweed**, with real package
installs and removals and HelixForge's own packages installed and removed.
Derivatives of those use the same package managers. Flatpak, the AUR (through
paru or yay), Nix and Home Manager are supported where they are installed.

## Contributing

Bug reports and ideas are welcome through the
[issue forms](https://github.com/Commanderx-code/Helix-Forge/issues/new/choose).
To change the code or the catalog, see
[CONTRIBUTING.md](https://github.com/Commanderx-code/Helix-Forge/blob/main/CONTRIBUTING.md);
everyone taking part follows the
[code of conduct](https://github.com/Commanderx-code/Helix-Forge/blob/main/CODE_OF_CONDUCT.md).

## License

MIT; see [LICENSE](https://github.com/Commanderx-code/Helix-Forge/blob/main/LICENSE).
The general catalog started as [Linutil](https://github.com/ChrisTitusTech/linutil)'s,
also MIT; see
[THIRD_PARTY_NOTICES.md](https://github.com/Commanderx-code/Helix-Forge/blob/main/THIRD_PARTY_NOTICES.md).
HelixForge replaces Commander Toolbox, now archived.
