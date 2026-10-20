# Changelog

Each release is tagged `vMAJOR.MINOR.PATCH` on GitHub, with the program, its
packages and their checksums. A release whose workflow only adopted updated
dotfiles or Myfish sources raises the patch number and is not listed here.

## v1.11.0

- **The setup wizard in the window.** It opens by itself on the first
  start, as in the terminal interface: this machine, a choice of
  workstation, then its entries for this machine, ticked where they can
  run and are not installed, put in the Install Plan for review. The
  Dashboard and Settings open it again.
- **The command palette in the window.** `Ctrl K`, or Search in the
  header: the screens, the actions and every entry in one list, ranked as
  the terminal interface ranks its search.
- **A profile's entries in the plan.** A profile's details put its
  entries in the Install Plan one by one, to change before it runs.
- **The window is tested on every change.** The Checks workflow opens a
  debug build on a virtual display and drives it through the wizard, the
  palette, every screen, a run and a saved profile against the fixture
  catalog (`scripts/gui-smoke.sh`).
- **In the engine**, for both interfaces: `setup::lines`,
  `setup::laptop_here`, `settings::set_up` and `settings::mark_set_up`.

## v1.10.0

- **Staying up to date.** An Arch package of the window, and a pacman
  repository on GitHub Pages that every release rebuilds with both packages
  for x86_64 and aarch64: listed in `pacman.conf`, it updates them with the
  rest of the system ([INSTALL.md](docs/INSTALL.md#the-arch-repository)).
- **The Helix Toolbox update row does more.** With the repository it runs
  pacman; a distribution package is updated together with the window's
  package when that is installed; a copy of the program the user keeps
  somewhere (`~/.local/bin`, say) is replaced by the release's, checked
  against its SHA256SUMS; and the window run as an AppImage replaces its
  own file the same way.

## v1.9.1

- **The icon at menu size.** Both packages ship the icon in every size a
  menu or panel asks for; at 16 to 48 pixels it is a flat drawing of the
  same tile, letters and strand (`assets/icon-small.svg`), where the full
  picture turned into a dark blur. `scripts/icons.sh` renders the sizes.

## v1.9.0

- **The window has every screen.** Helix Doctor (the checks, each with
  what it found and its repair), Updates (the channels, checked online on
  request, and the entries with an update), Backups (restore one), Drift
  (compare with a baseline or a profile, put back what is missing, save
  this machine as a baseline), Reliability (how far each entry is tested,
  and where it comes from), Hardware (firmware updates, disk health,
  temperatures, batteries, devices), Desktop (the appearance, saved looks
  put back, the entries for this desktop) and Settings (the run choices,
  shared with the terminal interface). The Install Plan saves as a
  profile. Every action goes through the same confirmation as before.
- **A new icon** for the window and the menu entries.
- **In the engine**, for both interfaces: `reliability::rows`,
  `trust::text`, `drift::available` and `settings::save_run_choices`.

## v1.8.0

- **A window.** `helix-toolbox-gui` opens the catalog in a Tauri window:
  the Dashboard, the areas with search, status and flags, the details of
  an entry, Favorites, the Install Plan, History, the confirmation with
  each step's command, and the run in an embedded terminal, where a step's
  questions and the password prompt are typed into. The same engine plans,
  checks, confirms, verifies and records; the terminal interface's
  favorites and history are shared. Packages for Debian and Ubuntu, Fedora
  and openSUSE, and an AppImage, on x86_64 and ARM64, each release; see
  docs/GUI.md. The other screens stay in the terminal interface for now.
- **For interfaces**: `confirmation::prepare()`, `overview::Overview` and
  `settings::run_choices()` in the engine, which the terminal interface
  uses too.

## v1.7.5

- **A clearer header.** Where the terminal shows pictures, the header now
  shows the HT emblem with the name and tagline beside it in the theme's
  cyan and purple, instead of the whole banner shrunk to four rows, where
  its lettering could not be read.

## v1.7.0

- **HelixForge is now Helix Toolbox.** The command is `helix-toolbox`, or
  `htb` for short, and the repository is
  `Commanderx-code/Helix-toolbox` (the old address forwards). New artwork
  for the banner, the icon and the logo.
- **Nothing to do by hand.** The first start moves `~/.config/helixforge`
  and `~/.local/state/helixforge` to `helix-toolbox`, leaving a link at the
  old names. The `helix-toolbox` packages replace `helixforge` and keep a
  `helixforge` command. Each release also carries its files under the old
  names, so HelixForge copies and launchers update to it.
- **What other programs read stays.** `--json` keeps its `helixforge`
  version field, beside a new `helix_toolbox`; the Updates channel keeps the
  id `helixforge`; runs set `HELIXFORGE_LIB` as well as `HELIX_TOOLBOX_LIB`;
  `HELIXFORGE_LOG` still sets the log level; bundles and backups made
  before 1.7 are read as before. See docs/MIGRATION.md.
- **Crates**: `helix-toolbox` and `helix-toolbox-core`, `-catalog`,
  `-platform` and `-engine` on crates.io. The `helixforge` crates' last
  version says where they went.

## v1.6.3

- **Execution events**: the engine reports a run in its pseudo-terminal as
  typed events on a bounded channel: the run and each step starting and
  ending, the terminal's output, progress, warnings, and how the run ended
  with its checks. A consumer that falls behind loses nothing and never holds
  the run up. The terminal interface's running window is drawn from these
  events and works as before; a graphical interface will use the same ones
  ([docs/EVENTS.md](docs/EVENTS.md)). The command line and `--json` do not
  change.

## v1.6.0

- **ARM64**: every release has a build for Linux on aarch64 beside the
  x86_64 one: `helixforge-linux-aarch64`, an Arch package (`aarch64`), a
  Debian/Ubuntu package (`arm64`) and an RPM (`aarch64`), built and tested on
  ARM64 machines, with their checksums in `SHA256SUMS` and signed provenance.
  The Distributions containers run on ARM64 too; Arch Linux ARM, which has no
  image on Docker Hub, runs from its own root file system, checked against
  the project's build key. The Reliability screen shows those results "on
  ARM64". `start.sh`, the Updates screen and `fetch-release` take the files
  for the machine's architecture.
- **Entries for x86_64 only**: a new `architectures` key in an entry's
  `[needs]` table. Steam, Proton CachyOS, the CachyOS gaming lists and
  repository, Chaotic-AUR, the Steam Deck kernel, Arch Server Setup, DaVinci
  Resolve, the Jagex Launcher, Google Chrome, FreeOffice, PDF Studio and the
  pinned terminal tools (TFM, Cassette, NetWatch) are left out on ARM64, and
  blocked if named there anyway.

## v1.5.9

- **The last British spellings go**: pinned entries are kept in
  `favorites.toml`. Pins in `favourites.toml` are read and moved on the
  next pin, and the old name is left as a link to the new file, so an
  earlier 1.x still finds them.
- **Clearer license fields**: `trust --json` names whose license each one
  is: `installer_license` for HelixForge's installer script and
  `package_license` for the software it installs. They replace `license`
  and `software_license` (1.5.8) and `licence` and `software_licence`
  (before it), which are gone. This is the one exception to the promise
  that schema 1 fields are never removed.
- **US forms**: toward, afterward and backward replace towards, afterwards
  and backwards in the interface, catalog and documentation.

## v1.5.8

- **US English**: the interface, messages, catalog, installers' messages and
  documentation are spelled in US English (color, license, optimize,
  canceled, favorites). `trust --json` gains `license` and
  `software_license`, and keeps `licence` and `software_licence` with the
  same values for programs that read them. Pinned entries stay in
  `favourites.toml`, so an earlier 1.x still finds them.

## v1.5.7

- **Drift counts what is in place**: an entry with no recorded run on this
  machine is no longer reported **Not run** when what it sets is there: the
  files it writes, or the GRUB, kernel command line, login screen or
  mkinitcpio settings its installer's own renderer would write, already in
  the file. Runs before HelixForge 1.1 were not recorded, so such entries
  showed as not run though they had been. Where nothing tells, the note
  says so.

## v1.5.6

- **openSUSE bundles**: `bundle create` works with zypper, which downloads
  the packages as you, without root, into a scratch root of its own that
  shares the machine's repositories: with an empty package database for
  `--complete`, with the machine's own for the dependencies it lacks. The
  install checks each package's signature with `rpm -K`, then installs them
  with zypper's remote repositories set aside. An openSUSE Tumbleweed
  virtual machine makes both kinds of bundle as its user and installs one
  with no network.

## v1.5.5

- **Flatpaks in offline bundles**: an entry with no package for the
  distribution whose Flatpak is installed goes into the bundle as
  `flatpak create-usb` copies it, with its runtimes, and installs from that
  copy with no network; flatpak checks each commit's signature against the
  remote's key on the installing machine. `bundle create` names the
  collection ID the remote needs (Flathub's, `org.flathub.Stable`) rather
  than changing your remotes, and `bundle install` gives the installing
  machine's remote that published ID when it has none. Any other ID, and a
  remote that machine lacks, added with the bundle's copy of its key, need
  `--trust-bundle`. A virtual
  machine makes such a bundle, then installs from it cut off from the
  network.

## v1.5.4

- **The dotfiles entries' files before they are written**: the change
  preview compares each file the Neovim, Fastfetch, Starship, Konsole and
  Ghostty entries would write with the one here, side by side, once their
  pinned dotfiles source is on the machine; a folder replaced whole says
  which files change, which are added and which stay only in its backup.
  The installer's own code lists them without writing anything. `d` on the
  changes, or `plan --fetch-sources`, downloads the source, and nothing
  else, when it is not there yet.

## v1.5.3

- **Every boot entry but NVIDIA's in a virtual machine**: a machine of its
  own runs the other eleven GRUB themes (CachyOS, Catppuccin, Tela, Vimix and
  the six CTT themes) and the Garuda theme, from Garuda's repository added
  as a Garuda machine has it; each boot menu is checked on the screen. The
  Arch machine also runs Boot Splash Theme and Dotfiles Boot Splash, which
  says where to get the KDE Store theme dotfiles names when it is missing
  and selects it once installed. All of them are VM Tested.

## v1.5.2

- **More virtual machines**: the Distributions workflow adds a Fedora cloud
  image, the Arch image on UEFI firmware, the Arch live ISO with an empty
  disk, and an Arch machine whose firmware says it is a Steam Deck; the
  cloud images' disks take TRIM, and the Arch one has a USB stick. Newly VM
  Tested: the CachyOS and Garuda Limine boot menus (on a machine made to
  boot through Limine), Fedora SELinux Tools, Enable SSD TRIM, Bootable USB
  Creator (the stick boots on BIOS and UEFI), Arch Server Setup (the
  installed system boots and reports a clean start) and Linux Neptune for
  SteamDeck; the GRUB, boot splash and SDDM entries on Fedora and UEFI too.
- Found by them and fixed: switching from one Limine look to another kept
  the previous look's settings the new one does not set (the CachyOS
  palette stayed on the Garuda menu). Arch Server Setup asked to confirm an
  empty timezone forever when its lookup service did not answer; it offers
  UTC then. Linux Neptune for SteamDeck installed Valve's 5.13 kernel from
  2021, which cannot mount today's btrfs file systems and which GRUB lists
  first, so the machine did not boot; it installs the newest released
  Neptune kernel now. It also failed copying speaker firmware that Valve no
  longer ships there, and installs linux-firmware-cirrus, which has it,
  instead.

## v1.5.1

- **Virtual machine tests**: the Distributions workflow boots Arch and
  Debian cloud images under QEMU, runs the GRUB theme and default
  appearance, the boot splash on and off, AppArmor and the SDDM theme for
  real, reboots into each result and checks it, keeping the boot screens.
  Entries verified there are VM Tested on the Reliability screen.
- Found by them and fixed: the SDDM Theme entry refused to run on Arch with
  SDDM 0.21 and Qt 6.12, where `sddm --version` crashes outside a desktop
  session; it reads the version from the package manager now. AppArmor
  Setup reported a failure on Arch right after installing the tools,
  because systemd still held its answer from before the package and
  skipped the service; it asks systemd again and starts it. The SDDM Theme
  entry, on a machine with no display manager enabled yet, took the name of
  the missing link for one and failed to disable it. On Debian, whose users
  have no system folders on their PATH, the GRUB and boot splash entries
  could not find grub-mkconfig or update-initramfs; runs add those folders.

## v1.5.0

- **Offline installation bundles**: `helixforge bundle create <folder>
  <entries>` downloads the packages entries need for this distribution and
  architecture (the dependencies this machine lacks, or `--complete`), with
  a copy of HelixForge, a manifest and `SHA256SUMS`; pacman's signatures come
  along, apt's downloads are checked against the repository's signed index,
  dnf's are signed. `bundle verify` checks a bundle; `bundle install`
  copies its files to a private folder, checks them against the verified
  sums and their signatures (`pacman-key --verify`, `rpm -K`; Debian packages
  against this machine's signed index, or `--trust-bundle`), and installs
  them from its files, after confirmation. Entries that need the network are named, not bundled.
  pacman, apt and dnf are supported; zypper not yet.
- The README's screenshots show the new sidebar.

## v1.4.0

- **Setup wizard**: on the first start (and from Settings or `Ctrl-K`),
  HelixForge shows what it found on the machine (distribution, package
  tools, desktop, graphics, laptop or not) and offers Developer, Gaming,
  Privacy, Minimal and Custom workstations from `catalog/setup.toml`, fitted
  to the machine (NVIDIA drivers for the distribution, SELinux or AppArmor).
  The ticked entries go to the Install Plan for review; it runs nothing.
  `helixforge setup [workstation]` prints the same as a plan.
- **Desktop environment manager**: a Desktop screen and `helixforge desktop`
  show the desktop and its version, the session, the login screen and its
  configuration, and the appearance settings read with the desktop's own
  tools; save a look and put it back with them, after confirmation. It warns
  where Home Manager's Plasma or dconf steps or Stylix set the look, refuses
  to change it where Home Manager links its files, and lists the entries for
  that desktop.
- The sidebar keeps its main menu cards and scrolls the rest when there are
  more screens than room.

## v1.3.0

- **System change preview**: `e` on the Install Plan, in the confirmation or
  on a backup, and `--effects` on `plan` and the dry runs, show what a plan is
  expected to change, marked known, expected or not known in advance: the
  packages the package manager says come with it (asked without root), files
  side by side as the installer's own code would write them (GRUB, the boot
  splash's kernel command line and initramfs hooks, SDDM, AppArmor), a
  backup against the file it replaces, services, and when it takes effect.
  The boot, login-screen and AppArmor entries declare their effects in a new
  `[effects]` catalog table.
- **Hardware and firmware center**: a Hardware screen and `helixforge
  hardware` list fwupd's firmware updates apart from its devices, disks with
  their temperature, sensors, battery health and cycles, PCI devices with
  their drivers and USB devices, read without root. A firmware update asks
  for the word FIRMWARE before its plan; a SMART check runs with your
  password, after confirmation.

## v1.2.0

- **Reliability**: a new screen and `helixforge reliability` show how far
  each entry has been tested: Container Tested (installed and verified in the
  Distributions workflow's containers), Stand-in Tested (its installer run
  against stand-in commands), VM Tested or Hardware Tested (from reports), or
  Not Yet Validated, with the systems, the date it was last verified and its
  last run on this machine. Boot, login-screen, driver and whole-system
  entries come first, and running one that has no more than stand-in tests
  says so in its confirmation.
- **Trust inspector**: `v` on any entry, or `helixforge trust <id>`, shows
  the installer's origin, file, checksum and license, the software's
  license, and keeps release provenance, package integrity checks (pacman's
  SigLevel, apt, dnf's gpgcheck, Flathub, the unreviewed AUR) and the
  installer's own downloads (pinned, checked, or unverified) apart, with the
  privileges it uses.
- Releases wait for the Distributions workflow on their commit and are not
  published when it fails; each carries the record of what the containers
  verified, also published as `validation.json`. The containers now also
  install Neovim and btop.

## v1.1.0

- **HELIX Drift**: `helixforge drift save <name>` (or `s` on the new Drift
  screen) records a baseline of the machine: what is installed and from
  where, the configuration files entries write (by checksum), the login
  shell, display manager, repositories and Flatpak remotes, and what
  HelixForge ran. Drift then lists what is missing, changed, replaced or new
  compared with it, or with a profile, and queues the entries that put the
  missing parts back for review. Baselines export and import, so a fresh
  installation can be checked against the old machine.
- **Command palette**: `Ctrl-K` finds any screen, action (check updates
  online, DNS diagnostics, save a baseline, change the theme) or entry by
  name, from anywhere in the menu.
- The sidebar drops its spacing, then scrolls, when the terminal is too
  short for every screen, and the hint line leaves out what does not fit.

## v1.0.4 to v1.0.6

- The Favorites screen is now spelled **Favorites**; pins stay in
  `favourites.toml`, so nothing needs moving.
- The Updates screen has a **HelixForge** row that updates HelixForge
  itself, the way it was installed: a package is downloaded, checked against
  the release's SHA256SUMS and installed with the package manager; a Cargo
  install runs `cargo install` again; a checkout is fast-forwarded and
  rebuilt. The launcher and `start.sh` already run the latest release.
- A new README with screenshots, an install table and a documentation index;
  the detail it held moved to docs/REFERENCE.md, docs/CATALOG.md and
  CONTRIBUTING.md. A code of conduct, a pull request template and a security
  policy for private vulnerability reports.

## v1.0.1 to v1.0.3

- v1.0.3: HelixForge is on [crates.io](https://crates.io/crates/helixforge):
  `cargo install --locked helixforge` builds it on any machine with Rust,
  and every release publishes the crates at its own version
  ([RELEASING.md](docs/RELEASING.md)).
- v1.0.2: `cargo install --git` works ([INSTALL.md](docs/INSTALL.md)), and
  the crates are ready for crates.io.
- v1.0.1: Helix Doctor treats a failed service whose unit file was removed
  (its package uninstalled) as a notice, "removed, but its last failure is
  still remembered", with `systemctl reset-failed` as the repair instead of a
  restart that cannot work.

## v1.0.0

The first stable release. What stays the same across 1.x is written down in
[STABILITY.md](docs/STABILITY.md).

- Tested on openSUSE Tumbleweed and Ubuntu 22.04 as well as Arch, Debian,
  Ubuntu 24.04 and Fedora, on every change; HelixForge's own packages are
  installed, used and removed on each.
- 37 more entries know their openSUSE package (first in v0.7.1), each checked
  against the openSUSE repositories.
- A **monochrome** theme, with no color, used for the run when `NO_COLOR`
  is set.
- Settings are read one by one: a setting or value from a later version is
  ignored instead of resetting the others.
- Damaged settings, favorites, profiles and history are skipped or replaced
  by defaults, and never stop the menu or the commands.
- The release workflows pin every action to a commit.
- docs/BETA.md became docs/REPORTING.md.

## v0.7.0

Beta release: packaging, documentation and testing.

- Packages for Arch, Debian/Ubuntu and Fedora/openSUSE with every release,
  carrying shell completions, manual pages, a menu entry and icon, each with
  a SHA-256 sum and a signed build provenance attestation
  ([INSTALL.md](docs/INSTALL.md)). First published with v0.6.1.
- `helixforge completions bash|zsh|fish`.
- `helixforge report` prints what a bug report needs, for pasting into an
  issue, with `--json`.
- A user guide ([GUIDE.md](docs/GUIDE.md)), a beta testing guide
  ([REPORTING.md](docs/REPORTING.md)), issue forms and this changelog.
- Fixed: unpacking the built-in catalog could fail on some file systems,
  depending on the order the build machine listed the scripts folder.

## v0.6.0

- `--json` on the commands that read and on every `--dry-run` plan, in
  versioned shapes ([JSON.md](docs/JSON.md)); refused for anything that
  changes the machine.
- Command-line output piped into `head` ends quietly.
- v0.5.1: the **Updates** screen and `helixforge updates`: what pacman, apt,
  dnf or zypper, the AUR helper, Flatpak, Home Manager, NixOS and a Nix
  profile have waiting, checked offline or online; updates delegated to
  those tools after confirmation.

## v0.5.0

- Ranked search with loose spellings, and `helixforge search`.
- `i` filters a list to installed entries or ones with an update.
- `*` pins favorites; the **Favorites** screen lists them and recently run
  entries.
- Clearing a search returns to the row you were on.
- v0.4.1: your own profiles: `s` saves the Install Plan as a named profile;
  `helixforge profile list|export|import|delete`.

## v0.4.0

- **Helix Doctor 2.0**: checks for the network, DNS, failed services,
  graphics drivers, the package database, home folder permissions, boot
  images, GRUB, the EFI partition and disk space, with evidence and repairs
  that run after their own confirmation; `helixforge doctor [--repair]`.
- v0.3.1: the **Backups** screen and `helixforge backups|restore`; a Snapper
  or Timeshift snapshot before system changes where the machine has one.

## v0.3.0

- The Install Plan as a queue: steps ordered by what they need, an option
  to keep going after a failure, each step's outcome shown, and unfinished
  entries kept to run again.
- v0.2.2: checks before a run: missing tools, repositories, graphics,
  free space, conflicts.

## v0.2.0

- Script entries say what they install, so their status shows too; commands
  installed some other way are traced to their package.
- v0.2.1: Pacman Config changes only what is off and says what it found.

## v0.1.x

- v0.1.3: `u` and `U` update entries; `helixforge update`.
- v0.1.2: status for every package entry: installed, update available,
  managed by Nix, or not installed, with source and version.
- v0.1.1: installed state from each entry's packages and the packages it
  conflicts with (WINE and WINE Staging).
- v0.1.0: numbered releases. Before them: the rebuild of Commander Toolbox
  in Rust with every entry carried across; tests in Arch, Debian, Ubuntu and
  Fedora containers; verified installs with fallback to the next source; the
  dashboard design, logo and banner; gaming entries (WINE and Proton,
  CachyOS's gaming sets, Itch, Minigalaxy, Rare, Faugus, Prism and the
  RuneScape launchers).
