# Migration

## From HelixForge to Helix Toolbox (1.7)

HelixForge is called Helix Toolbox from 1.7.0. Updating carries everything
over; nothing needs to be done by hand.

| What | Before 1.7 | From 1.7 |
| :--- | :--- | :--- |
| Command | `helixforge` | `helix-toolbox`, or `htb` for short; the packages keep `helixforge` as a link to it |
| Repository | `Commanderx-code/Helix-Forge` | `Commanderx-code/Helix-toolbox`; GitHub sends the old address, its downloads and `git` to the new one |
| Packages | `helixforge` | `helix-toolbox`, which replaces `helixforge` when installed |
| Release files | `helixforge-linux-x86_64`, `helixforge_*.deb`, ... | `helix-toolbox-linux-x86_64`, `helix-toolbox_*.deb`, ...; each release also has the same files under the old names, for older copies to update from |
| Crates | `helixforge`, `helixforge-core`, ... | `helix-toolbox`, `helix-toolbox-core`, ...; the old crates' last version says where to go |
| Settings, favorites, profiles, baselines, looks | `~/.config/helixforge` | `~/.config/helix-toolbox` |
| History and logs | `~/.local/state/helixforge` | `~/.local/state/helix-toolbox` |
| Variable for scripts | `HELIXFORGE_LIB` | `HELIX_TOOLBOX_LIB`; runs set both |
| Log level | `HELIXFORGE_LOG` | `HELIX_TOOLBOX_LOG`; the old name still works |
| Configuration backups | `<file>.helixforge-backup-<time>` | `<file>.helix-toolbox-backup-<time>`; the Backups screen lists both |

The first start of 1.7 moves the two folders to their new names and leaves
a link at each old name, so an older release or a script reading the old
path finds the same files. What installed software keeps under
`~/.local/share/helixforge` (AppImages, launchers, downloaded sources) stays
there, because the entries made for it point there.

Some names stay as they were, because other programs read them: the
`helixforge` version field in `--json` output (beside a new
`helix_toolbox`), the Updates channel id `helixforge`, and the `helixforge`
version field in offline bundles. Bundles made before 1.7 install as before.

To update from HelixForge, use the Updates screen or install the new
package; `cargo install --locked helix-toolbox` for a Cargo install.

## From Commander Toolbox

HelixForge (Helix Toolbox's name before 1.7) replaces Commander Toolbox, a
Linutil fork, which is now archived; the dotfiles `toolbox` command opens
Helix Toolbox. This matrix records every entry of the old catalog and where
it went. Status values:

- **done**: available in HelixForge with the same behavior
- **partial**: available with a difference noted
- **dropped**: left out, with the reason

Both parts of the old catalog have come across. The Commander installers
live under `scripts/installers/commander/` with their Python tests; the
general catalog's scripts keep their old directory tree under
`scripts/installers/` and source the shared helpers through two forwarding
shims, so none of them was edited. They keep their MIT notice; see
`THIRD_PARTY_NOTICES.md`.

38 of the general installers were one package per package manager, often
from the AUR on Arch with a Flatpak fallback, and 29 of them wrapped that in
an "install or uninstall" prompt. They are package entries now, and their
scripts are gone. The reason: as data, HelixForge plans them before they run
(showing the exact command and whether a password is needed), reports when
they are already installed, including as a Flatpak, and offers removal with
`r` or `helixforge remove` instead of a prompt inside a script. The package
model grew openSUSE, Alpine, Void and Solus names, `aur` and `flatpak` for
this, so no distribution these scripts supported was dropped. The other 138
installers add repositories, run several steps or configure what they
install, and stay scripts.

## Commander installers

| Old entry | Old tab / group | HelixForge area | Status |
| :--- | :--- | :--- | :--- |
| DaVinci Resolve - Native | Applications Setup / Design Tools | Applications | done: `scripts/installers/commander/`, same script and tests |
| DaVinci Resolve - Davincibox | Applications Setup / Design Tools | Applications | done: `scripts/installers/commander/`, same script and tests |
| DaVinci Resolve Maintenance | Applications Setup / Design Tools | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Workstation Setup | Applications Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| Myfish Fish (Native) | Applications Setup / Myfish Shell Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| Myfish Bash (Native) | Applications Setup / Myfish Shell Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| Myfish Zsh (Native) | Applications Setup / Myfish Shell Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| Myfish Guided Setup | Applications Setup / Myfish Shell Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Neovim Config | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Fastfetch Config | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Starship Config | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Konsole Config | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Ghostty Config | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Zellij Terminal Sessions | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| NetWatch Network Dashboard | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| TFM Visual File Manager | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Cassette Spotify Player | Applications Setup / Dotfiles | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| CachyOS GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Catppuccin Mocha GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Dracula GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Tela GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Vimix GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| CyberRe GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Cyberpunk GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Shodan GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Fallout GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| DedSec GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Minegrub (Minecraft) GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| BSOL GRUB Theme | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Restore Default GRUB Appearance | Applications Setup / Grub Theme | Applications | done: `scripts/installers/commander/`, same script and tests |
| Commander Boot Splash | Applications Setup / Boot Splash | Applications | done: `scripts/installers/commander/`, same script and tests |
| Boot Splash Theme | Applications Setup / Boot Splash | Applications | done: `scripts/installers/commander/`, same script and tests |
| Enable Boot Splash | Applications Setup / Boot Splash | Applications | done: `scripts/installers/commander/`, same script and tests |
| Disable Boot Splash | Applications Setup / Boot Splash | Applications | done: `scripts/installers/commander/`, same script and tests |
| CachyOS Limine Boot Menu | Applications Setup / Limine Boot Menu | Applications | done: `scripts/installers/commander/`, same script and tests |
| Garuda Limine Boot Menu | Applications Setup / Limine Boot Menu | Applications | done: `scripts/installers/commander/`, same script and tests |
| SDDM Theme | Applications Setup | Applications | done: `scripts/installers/commander/`, same script and tests |
| AppArmor Setup and Status | Security | Security | done: `scripts/installers/commander/`, same script and tests |
| Fedora SELinux Tools and Status | Security | Security | done: `scripts/installers/commander/`, same script and tests |
| Commander Topgrade | System Setup | Maintenance | done: `scripts/installers/commander/`, same script and tests |
| System Diagnostics | Utilities / Toolbox Maintenance | Maintenance | done: `scripts/installers/commander/`, same script and tests |
| Config Backup Restore | Utilities / Toolbox Maintenance | Maintenance | done: `scripts/installers/commander/`, same script and tests |
| Toolbox Updater | Utilities / Toolbox Maintenance | Maintenance | dropped: HelixForge updates through its releases (Milestone 3) |
| Installation History | Utilities / Toolbox Maintenance | Maintenance | done: the `history` command and record, and the History screen in the menu and the window |
| Driverless Printer Setup | Utilities / Printers | Maintenance | done: `scripts/installers/commander/`, same script and tests |

## General catalog

| Old entry | Old tab / group | HelixForge area | Status |
| :--- | :--- | :--- | :--- |
| Discord | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Jitsi | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Signal | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Slack | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Telegram | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Thunderbird | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| ZapZap | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Zoom | Applications Setup / Communication Apps | Applications | done: script carried over unchanged |
| Ardour | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Audacity | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Blender | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Darktable | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| GNU Image Manipulation Program (GIMP) | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Handbrake | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Inkscape | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Krita | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| MyPaint | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Natron | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| OBS Studio | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Olive | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| OpenShot | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Pinta | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Scribus | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Synfig Studio | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Tenacity | Applications Setup / Design Tools | Applications | done: script carried over unchanged |
| Cursor | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Github Desktop | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| JetBrains Toolbox | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Meld | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Neovim | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Ngrok | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Sublime Text | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| VS Code | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| VS Codium | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| Zed | Applications Setup / Developer Tools | Applications | done: script carried over unchanged |
| FreeOffice | Applications Setup / Office Suites | Applications | done: script carried over unchanged |
| LibreOffice | Applications Setup / Office Suites | Applications | done: script carried over unchanged |
| OnlyOffice | Applications Setup / Office Suites | Applications | done: script carried over unchanged |
| WPS Office | Applications Setup / Office Suites | Applications | done: script carried over unchanged |
| Evince | Applications Setup / PDF Suites | Applications | done: script carried over unchanged |
| Okular | Applications Setup / PDF Suites | Applications | done: script carried over unchanged |
| PDF Studio | Applications Setup / PDF Suites | Applications | done: script carried over unchanged |
| PDF Studio Viewer | Applications Setup / PDF Suites | Applications | done: script carried over unchanged |
| Brave | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Brave Origin | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Chromium | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Google Chrome | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| LibreWolf | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Lynx | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Mozilla Firefox | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Thorium | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Tor Browser | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Vivaldi | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| waterfox | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Zen Browser | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Helium Browser | Applications Setup / Web Browsers | Applications | done: script carried over unchanged |
| Alacritty | Applications Setup | Applications | done: script carried over unchanged |
| Android Debloater | Applications Setup | Applications | done: script carried over unchanged |
| Dank Shell | Applications Setup | Applications | done: script carried over unchanged |
| Docker | Applications Setup | Applications | done: script carried over unchanged |
| Flatpak / Flathub | Applications Setup | Applications | done: script carried over unchanged |
| Ghostty | Applications Setup | Applications | done: script carried over unchanged |
| Kitty | Applications Setup | Applications | done: script carried over unchanged |
| Noctalia Shell | Applications Setup | Applications | done: script carried over unchanged |
| Podman | Applications Setup | Applications | done: script carried over unchanged |
| Podman-compose | Applications Setup | Applications | done: script carried over unchanged |
| Rofi | Applications Setup | Applications | done: script carried over unchanged |
| Shelly | Applications Setup | Applications | done: script carried over unchanged |
| RetroArch Game Emulator | Gaming / Multi-System Emulators | Applications | done: script carried over unchanged |
| bsnes | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| Dolphin Emulator | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| Gopher 64 | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| MelonDS | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| mGBA | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| Mupen 64 Plus | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| Ryujinx | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| snes9x | Gaming / Nintendo Emulators | Applications | done: script carried over unchanged |
| PCSX2 | Gaming / PlayStation Emulators | Applications | done: script carried over unchanged |
| Play! | Gaming / PlayStation Emulators | Applications | done: script carried over unchanged |
| RPCS3 | Gaming / PlayStation Emulators | Applications | done: script carried over unchanged |
| Blastem | Gaming / Sega Emulators | Applications | done: script carried over unchanged |
| Flycast | Gaming / Sega Emulators | Applications | done: script carried over unchanged |
| Kronos | Gaming / Sega Emulators | Applications | done: script carried over unchanged |
| XEMU | Gaming / Xbox Emulators | Applications | done: script carried over unchanged |
| Xenia | Gaming / Xbox Emulators | Applications | done: script carried over unchanged |
| Heroic | Gaming / Game Launchers | Applications | done: script carried over unchanged |
| Lutris | Gaming / Game Launchers | Applications | done: script carried over unchanged |
| RetroArch | Gaming / Game Launchers | Applications | done: script carried over unchanged |
| Steam | Gaming / Game Launchers | Applications | done: script carried over unchanged |
| Auto CPU Frequency | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Auto Mount Drive | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Bottles | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Gaming Dependencies | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| General GPU Drivers | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Nvidia Drivers & Hardware Acceleration | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| ProtonPlus | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| ProtonUp-Qt | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Waydroid | Gaming / Tools and Setups | Applications | done: script carried over unchanged |
| Arc Raiders Titus Mods | Gaming | Applications | done: script carried over unchanged |
| Diablo II Resurrected Loot Filter | Gaming | Applications | done: script carried over unchanged |
| EverQuest Legends MIDI Fix | Gaming | Applications | done: script carried over unchanged |
| Fallout 76 INI and Mods | Gaming | Applications | done: script carried over unchanged |
| Starfield Titus INI and Mods | Gaming | Applications | done: script carried over unchanged |
| FirewallD Firewall Baselines (CTT) | Security | Security | done: script carried over unchanged |
| Lynis Security Audit | Security | Security | done: script carried over unchanged |
| UFW Firewall Baselines (CTT) | Security | Security | done: script carried over unchanged |
| Alpine Update | System Setup / Alpine | Maintenance | done: script carried over unchanged |
| Arch Server Setup | System Setup / Arch | Maintenance | done: script carried over unchanged |
| DWM-Titus | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Hyprland JaKooLit | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Install CachyOS Repository and Kernel | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Install Chaotic-AUR Repository | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Linux Neptune for SteamDeck | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Nvidia Drivers & Hardware Acceleration | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Omarchy Rice by DHH | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Pacman Config | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Paru AUR Helper | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Snapper Snapshots | System Setup / Arch | Maintenance | done: script carried over unchanged |
| System Maintenance | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Virtualization | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Yay AUR Helper | System Setup / Arch | Maintenance | done: script carried over unchanged |
| Nala Package Manager | System Setup / Debian / Ubuntu | Maintenance | done: script carried over unchanged |
| Hyprland JaKooLit | System Setup / Debian / Ubuntu | Maintenance | done: script carried over unchanged |
| Install Desktop Environment | System Setup / Desktop Environment Setup | Maintenance | done: script carried over unchanged |
| Uninstall Desktop Environment | System Setup / Desktop Environment Setup | Maintenance | done: script carried over unchanged |
| Btrfs Assistant, Snapper Config, grub-btrfs | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Configure DNF | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Hyprland JaKooLit | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Multimedia Codecs | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Nvidia Proprietary Drivers | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| RPM Fusion | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Upgrade to a New Fedora Release | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Virtualization | System Setup / Fedora | Maintenance | done: script carried over unchanged |
| Hyprland JaKooLit | System Setup / Ubuntu | Maintenance | done: script carried over unchanged |
| Build Prerequisites | System Setup | Maintenance | done: script carried over unchanged |
| Enable SSD TRIM | System Setup | Maintenance | done: script carried over unchanged |
| Full System Cleanup | System Setup | Maintenance | done: script carried over unchanged |
| Full System Update | System Setup | Maintenance | done: script carried over unchanged |
| Global Theme | System Setup | Maintenance | done: script carried over unchanged |
| PipeWire Audio | System Setup | Maintenance | done: script carried over unchanged |
| Remove Snaps | System Setup | Maintenance | done: script carried over unchanged |
| TTY Fonts | System Setup | Maintenance | done: script carried over unchanged |
| Zram Swap | System Setup | Maintenance | done: script carried over unchanged |
| Auto Detect Displays | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Change Orientation | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Disable Monitor | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Duplicate Displays | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Enable Monitor | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Extend Displays | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Manage Arrangement | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Reset Scaling | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Scale Monitors | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Set Brightness | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Set Primary Monitor | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| Set Resolution | Utilities / Monitor Control | Maintenance | done: script carried over unchanged |
| CUPS | Utilities / Printers | Maintenance | done: script carried over unchanged |
| Epson printer drivers | Utilities / Printers | Maintenance | done: script carried over unchanged |
| HP printer drivers | Utilities / Printers | Maintenance | done: script carried over unchanged |
| Add User | Utilities / User Account Manager | Maintenance | done: script carried over unchanged |
| Add User To Groups | Utilities / User Account Manager | Maintenance | done: script carried over unchanged |
| Change Password | Utilities / User Account Manager | Maintenance | done: script carried over unchanged |
| Delete User | Utilities / User Account Manager | Maintenance | done: script carried over unchanged |
| Remove User From Groups | Utilities / User Account Manager | Maintenance | done: script carried over unchanged |
| Auto Login | Utilities | Maintenance | done: script carried over unchanged |
| Bluetooth Manager | Utilities | Maintenance | done: script carried over unchanged |
| Bootable USB Creator | Utilities | Maintenance | done: script carried over unchanged |
| Crypto tool | Utilities | Maintenance | done: script carried over unchanged |
| Locale Setup | Utilities | Maintenance | done: script carried over unchanged |
| Numlock on Startup | Utilities | Maintenance | done: script carried over unchanged |
| Ollama | Utilities | Maintenance | done: script carried over unchanged |
| Ramalama | Utilities | Maintenance | done: script carried over unchanged |
| Service Manager | Utilities | Maintenance | done: script carried over unchanged |
| SSH Commands | Utilities | Maintenance | done: script carried over unchanged |
| SSH Samba | Utilities | Maintenance | done: script carried over unchanged |
| Timeshift Backup | Utilities | Maintenance | done: script carried over unchanged |
| US Locale Setup | Utilities | Maintenance | done: script carried over unchanged |
| WiFi Manager | Utilities | Maintenance | done: script carried over unchanged |

## Infrastructure

| Capability | Status |
| :--- | :--- |
| Catalog with tabs, groups, preconditions, flags, multi-select | done |
| Interactive runs in an embedded terminal, with log saving | done |
| Run history | done (JSONL; the old per-run directories are not read) |
| Package entries as data with a dry-run plan | done |
| Native operations implemented in Rust | done (system report) |
| Nix operations | done (profile install, Home Manager switch); untested on a real Nix machine |
| Profiles as reviewable plans | done |
| Shared shell helpers (`common-script.sh`, `common-service-script.sh`) | done, under `scripts/shared/` |
| Commander installer tests (Python) | done, under `tests/installers/`; the three menu-format tests were replaced by the catalog loader's own checks |
| Dotfiles and Myfish source sync | done: `scripts/sync-sources.py` renders catalog files and the workflow publishes releases |
| Releases and the `start.sh` launcher | done: every release is built, tested and published by the release workflow |
| Dashboard, history and settings screens | done (Milestone 2) |
| Graphical interface | done: the window, `helix-toolbox-gui`, since 1.8 ([GUI.md](GUI.md)) |

## Where the general catalog went

| Old tab | HelixForge |
| :--- | :--- |
| Applications Setup | Applications, merged with its groups (Design Tools, Developer Tools) |
| Gaming | Applications > Gaming |
| Security | Security |
| System Setup | Maintenance > System Setup |
| Utilities | Maintenance > Utilities; Printers merged into Maintenance > Printers |

The old Neovim entry installs Chris Titus's Neovim configuration; it is
"Neovim with Titus Config" here, beside the plain Neovim package.
