# Migration from Commander Toolbox

HelixForge replaces the Commander Toolbox, a Linutil fork. This matrix tracks
every entry of the old catalog and where it stands here. Status values:

- **done**: available in HelixForge with the same behaviour
- **partial**: available with a difference noted
- **planned**: not yet available; the old toolbox remains the stable way to run it

The old toolbox's own installers (the Commander group) move across first,
with their shared helpers and Python tests; the general catalog follows,
as data-only package entries where an entry is a plain package install and
as scripts otherwise. Scripts from the old catalog keep their MIT notice when
they are brought across (see `THIRD_PARTY_NOTICES.md` once that happens).

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
| Installation History | Utilities / Toolbox Maintenance | Maintenance | done (history command and record; a screen is planned) |
| Driverless Printer Setup | Utilities / Printers | Maintenance | done: `scripts/installers/commander/`, same script and tests |

## General catalog

| Old entry | Old tab / group | HelixForge area | Status |
| :--- | :--- | :--- | :--- |
| Discord | Applications Setup / Communication Apps | Applications | planned |
| Jitsi | Applications Setup / Communication Apps | Applications | planned |
| Signal | Applications Setup / Communication Apps | Applications | planned |
| Slack | Applications Setup / Communication Apps | Applications | planned |
| Telegram | Applications Setup / Communication Apps | Applications | planned |
| Thunderbird | Applications Setup / Communication Apps | Applications | planned |
| ZapZap | Applications Setup / Communication Apps | Applications | planned |
| Zoom | Applications Setup / Communication Apps | Applications | planned |
| Ardour | Applications Setup / Design Tools | Applications | planned |
| Audacity | Applications Setup / Design Tools | Applications | planned |
| Blender | Applications Setup / Design Tools | Applications | planned |
| Darktable | Applications Setup / Design Tools | Applications | planned |
| GNU Image Manipulation Program (GIMP) | Applications Setup / Design Tools | Applications | planned |
| Handbrake | Applications Setup / Design Tools | Applications | planned |
| Inkscape | Applications Setup / Design Tools | Applications | planned |
| Krita | Applications Setup / Design Tools | Applications | planned |
| MyPaint | Applications Setup / Design Tools | Applications | planned |
| Natron | Applications Setup / Design Tools | Applications | planned |
| OBS Studio | Applications Setup / Design Tools | Applications | planned |
| Olive | Applications Setup / Design Tools | Applications | planned |
| OpenShot | Applications Setup / Design Tools | Applications | planned |
| Pinta | Applications Setup / Design Tools | Applications | planned |
| Scribus | Applications Setup / Design Tools | Applications | planned |
| Synfig Studio | Applications Setup / Design Tools | Applications | planned |
| Tenacity | Applications Setup / Design Tools | Applications | planned |
| Cursor | Applications Setup / Developer Tools | Applications | planned |
| Github Desktop | Applications Setup / Developer Tools | Applications | planned |
| JetBrains Toolbox | Applications Setup / Developer Tools | Applications | planned |
| Meld | Applications Setup / Developer Tools | Applications | planned |
| Neovim | Applications Setup / Developer Tools | Applications | planned |
| Ngrok | Applications Setup / Developer Tools | Applications | planned |
| Sublime Text | Applications Setup / Developer Tools | Applications | planned |
| VS Code | Applications Setup / Developer Tools | Applications | planned |
| VS Codium | Applications Setup / Developer Tools | Applications | planned |
| Zed | Applications Setup / Developer Tools | Applications | planned |
| FreeOffice | Applications Setup / Office Suites | Applications | planned |
| LibreOffice | Applications Setup / Office Suites | Applications | planned |
| OnlyOffice | Applications Setup / Office Suites | Applications | planned |
| WPS Office | Applications Setup / Office Suites | Applications | planned |
| Evince | Applications Setup / PDF Suites | Applications | planned |
| Okular | Applications Setup / PDF Suites | Applications | planned |
| PDF Studio | Applications Setup / PDF Suites | Applications | planned |
| PDF Studio Viewer | Applications Setup / PDF Suites | Applications | planned |
| Brave | Applications Setup / Web Browsers | Applications | planned |
| Brave Origin | Applications Setup / Web Browsers | Applications | planned |
| Chromium | Applications Setup / Web Browsers | Applications | planned |
| Google Chrome | Applications Setup / Web Browsers | Applications | planned |
| LibreWolf | Applications Setup / Web Browsers | Applications | planned |
| Lynx | Applications Setup / Web Browsers | Applications | planned |
| Mozilla Firefox | Applications Setup / Web Browsers | Applications | planned |
| Thorium | Applications Setup / Web Browsers | Applications | planned |
| Tor Browser | Applications Setup / Web Browsers | Applications | planned |
| Vivaldi | Applications Setup / Web Browsers | Applications | planned |
| waterfox | Applications Setup / Web Browsers | Applications | planned |
| Zen Browser | Applications Setup / Web Browsers | Applications | planned |
| Helium Browser | Applications Setup / Web Browsers | Applications | planned |
| Alacritty | Applications Setup | Applications | planned |
| Android Debloater | Applications Setup | Applications | planned |
| Dank Shell | Applications Setup | Applications | planned |
| Docker | Applications Setup | Applications | planned |
| Flatpak / Flathub | Applications Setup | Applications | planned |
| Ghostty | Applications Setup | Applications | planned |
| Kitty | Applications Setup | Applications | planned |
| Noctalia Shell | Applications Setup | Applications | planned |
| Podman | Applications Setup | Applications | planned |
| Podman-compose | Applications Setup | Applications | planned |
| Rofi | Applications Setup | Applications | planned |
| Shelly | Applications Setup | Applications | planned |
| RetroArch Game Emulator | Gaming / Multi-System Emulators | Applications | planned |
| bsnes | Gaming / Nintendo Emulators | Applications | planned |
| Dolphin Emulator | Gaming / Nintendo Emulators | Applications | planned |
| Gopher 64 | Gaming / Nintendo Emulators | Applications | planned |
| MelonDS | Gaming / Nintendo Emulators | Applications | planned |
| mGBA | Gaming / Nintendo Emulators | Applications | planned |
| Mupen 64 Plus | Gaming / Nintendo Emulators | Applications | planned |
| Ryujinx | Gaming / Nintendo Emulators | Applications | planned |
| snes9x | Gaming / Nintendo Emulators | Applications | planned |
| PCSX2 | Gaming / PlayStation Emulators | Applications | planned |
| Play! | Gaming / PlayStation Emulators | Applications | planned |
| RPCS3 | Gaming / PlayStation Emulators | Applications | planned |
| Blastem | Gaming / Sega Emulators | Applications | planned |
| Flycast | Gaming / Sega Emulators | Applications | planned |
| Kronos | Gaming / Sega Emulators | Applications | planned |
| XEMU | Gaming / Xbox Emulators | Applications | planned |
| Xenia | Gaming / Xbox Emulators | Applications | planned |
| Heroic | Gaming / Game Launchers | Applications | planned |
| Lutris | Gaming / Game Launchers | Applications | planned |
| RetroArch | Gaming / Game Launchers | Applications | planned |
| Steam | Gaming / Game Launchers | Applications | planned |
| Auto CPU Frequency | Gaming / Tools and Setups | Applications | planned |
| Auto Mount Drive | Gaming / Tools and Setups | Applications | planned |
| Bottles | Gaming / Tools and Setups | Applications | planned |
| Gaming Dependencies | Gaming / Tools and Setups | Applications | planned |
| General GPU Drivers | Gaming / Tools and Setups | Applications | planned |
| Nvidia Drivers & Hardware Acceleration | Gaming / Tools and Setups | Applications | planned |
| ProtonPlus | Gaming / Tools and Setups | Applications | planned |
| ProtonUp-Qt | Gaming / Tools and Setups | Applications | planned |
| Waydroid | Gaming / Tools and Setups | Applications | planned |
| Arc Raiders Titus Mods | Gaming | Applications | planned |
| Diablo II Resurrected Loot Filter | Gaming | Applications | planned |
| EverQuest Legends MIDI Fix | Gaming | Applications | planned |
| Fallout 76 INI and Mods | Gaming | Applications | planned |
| Starfield Titus INI and Mods | Gaming | Applications | planned |
| FirewallD Firewall Baselines (CTT) | Security | Security | planned |
| Lynis Security Audit | Security | Security | planned |
| UFW Firewall Baselines (CTT) | Security | Security | planned |
| Alpine Update | System Setup / Alpine | Maintenance | planned |
| Arch Server Setup | System Setup / Arch | Maintenance | planned |
| DWM-Titus | System Setup / Arch | Maintenance | planned |
| Hyprland JaKooLit | System Setup / Arch | Maintenance | planned |
| Install CachyOS Repository and Kernel | System Setup / Arch | Maintenance | planned |
| Install Chaotic-AUR Repository | System Setup / Arch | Maintenance | planned |
| Linux Neptune for SteamDeck | System Setup / Arch | Maintenance | planned |
| Nvidia Drivers & Hardware Acceleration | System Setup / Arch | Maintenance | planned |
| Omarchy Rice by DHH | System Setup / Arch | Maintenance | planned |
| Pacman Config | System Setup / Arch | Maintenance | planned |
| Paru AUR Helper | System Setup / Arch | Maintenance | planned |
| Snapper Snapshots | System Setup / Arch | Maintenance | planned |
| System Maintenance | System Setup / Arch | Maintenance | planned |
| Virtualization | System Setup / Arch | Maintenance | planned |
| Yay AUR Helper | System Setup / Arch | Maintenance | planned |
| Nala Package Manager | System Setup / Debian / Ubuntu | Maintenance | planned |
| Hyprland JaKooLit | System Setup / Debian / Ubuntu | Maintenance | planned |
| Install Desktop Environment | System Setup / Desktop Environment Setup | Maintenance | planned |
| Uninstall Desktop Environment | System Setup / Desktop Environment Setup | Maintenance | planned |
| Btrfs Assistant, Snapper Config, grub-btrfs | System Setup / Fedora | Maintenance | planned |
| Configure DNF | System Setup / Fedora | Maintenance | planned |
| Hyprland JaKooLit | System Setup / Fedora | Maintenance | planned |
| Multimedia Codecs | System Setup / Fedora | Maintenance | planned |
| Nvidia Proprietary Drivers | System Setup / Fedora | Maintenance | planned |
| RPM Fusion | System Setup / Fedora | Maintenance | planned |
| Upgrade to a New Fedora Release | System Setup / Fedora | Maintenance | planned |
| Virtualization | System Setup / Fedora | Maintenance | planned |
| Hyprland JaKooLit | System Setup / Ubuntu | Maintenance | planned |
| Build Prerequisites | System Setup | Maintenance | planned |
| Enable SSD TRIM | System Setup | Maintenance | planned |
| Full System Cleanup | System Setup | Maintenance | planned |
| Full System Update | System Setup | Maintenance | planned |
| Global Theme | System Setup | Maintenance | planned |
| PipeWire Audio | System Setup | Maintenance | planned |
| Remove Snaps | System Setup | Maintenance | planned |
| TTY Fonts | System Setup | Maintenance | planned |
| Zram Swap | System Setup | Maintenance | planned |
| Auto Detect Displays | Utilities / Monitor Control | Maintenance | planned |
| Change Orientation | Utilities / Monitor Control | Maintenance | planned |
| Disable Monitor | Utilities / Monitor Control | Maintenance | planned |
| Duplicate Displays | Utilities / Monitor Control | Maintenance | planned |
| Enable Monitor | Utilities / Monitor Control | Maintenance | planned |
| Extend Displays | Utilities / Monitor Control | Maintenance | planned |
| Manage Arrangement | Utilities / Monitor Control | Maintenance | planned |
| Reset Scaling | Utilities / Monitor Control | Maintenance | planned |
| Scale Monitors | Utilities / Monitor Control | Maintenance | planned |
| Set Brightness | Utilities / Monitor Control | Maintenance | planned |
| Set Primary Monitor | Utilities / Monitor Control | Maintenance | planned |
| Set Resolution | Utilities / Monitor Control | Maintenance | planned |
| CUPS | Utilities / Printers | Maintenance | planned |
| Epson printer drivers | Utilities / Printers | Maintenance | planned |
| HP printer drivers | Utilities / Printers | Maintenance | planned |
| Add User | Utilities / User Account Manager | Maintenance | planned |
| Add User To Groups | Utilities / User Account Manager | Maintenance | planned |
| Change Password | Utilities / User Account Manager | Maintenance | planned |
| Delete User | Utilities / User Account Manager | Maintenance | planned |
| Remove User From Groups | Utilities / User Account Manager | Maintenance | planned |
| Auto Login | Utilities | Maintenance | planned |
| Bluetooth Manager | Utilities | Maintenance | planned |
| Bootable USB Creator | Utilities | Maintenance | planned |
| Crypto tool | Utilities | Maintenance | planned |
| Locale Setup | Utilities | Maintenance | planned |
| Numlock on Startup | Utilities | Maintenance | planned |
| Ollama | Utilities | Maintenance | planned |
| Ramalama | Utilities | Maintenance | planned |
| Service Manager | Utilities | Maintenance | planned |
| SSH Commands | Utilities | Maintenance | planned |
| SSH Samba | Utilities | Maintenance | planned |
| Timeshift Backup | Utilities | Maintenance | planned |
| US Locale Setup | Utilities | Maintenance | planned |
| WiFi Manager | Utilities | Maintenance | planned |

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
| Dotfiles and Myfish source sync | planned |
| Releases and the `start.sh` launcher | planned |
| Dashboard, history and settings screens | planned (Milestone 2) |
| Graphical interface | planned (Milestone 5) |
