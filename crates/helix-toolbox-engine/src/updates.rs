//! One overview of pending updates across the tools that keep this machine:
//! the distribution's package manager, an AUR helper, Flatpak and Nix. It
//! reads what each tool already knows; an online check asks the network only
//! through read-only commands. Updating is delegated to the tools themselves.

use std::path::{Path, PathBuf};

use helix_toolbox_core::{PackageManager, printable};
use helix_toolbox_platform::LinuxInfo;

use crate::doctor::System;
use crate::{Plan, Risk, Source, Step};

/// One package waiting for an update.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pending {
    pub name: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

/// One tool that updates part of the machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Channel {
    /// `pacman`, `aur`, `apt`, `dnf`, `zypper`, `flatpak-system`,
    /// `flatpak-user`, `home-manager`, `nixos` or `nix-profile`.
    pub id: String,
    pub name: String,
    /// What is waiting; None when the tool cannot say without updating, or
    /// only online.
    pub pending: Option<Vec<Pending>>,
    /// Where the answer comes from, or what is known instead.
    pub note: String,
    /// What updating runs, in order.
    pub commands: Vec<Vec<String>>,
    pub requires: Vec<String>,
    /// What the update changes beyond packages, said before it runs.
    pub caveat: Option<String>,
}

impl Channel {
    /// How many updates are known to wait.
    pub fn count(&self) -> Option<usize> {
        self.pending.as_ref().map(Vec::len)
    }

    /// A short status: "12 updates", "up to date" or "unknown".
    pub fn summary(&self) -> String {
        match self.count() {
            Some(0) => "up to date".to_string(),
            Some(1) => "1 update".to_string(),
            Some(count) => format!("{count} updates"),
            None => "not counted".to_string(),
        }
    }

    /// The commands as shown, safe to print.
    pub fn command_line(&self) -> String {
        printable(
            &self
                .commands
                .iter()
                .map(|command| command.join(" "))
                .collect::<Vec<_>>()
                .join(" && "),
        )
    }
}

/// Where to look, apart from the commands.
pub struct Places {
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config: PathBuf,
    pub home: PathBuf,
    /// Seconds since 1970, for how old the Nix inputs are.
    pub now: u64,
    /// The Helix Toolbox asking, for its own update; none leaves it out.
    pub this: Option<ThisCopy>,
}

/// The running Helix Toolbox: its version, where it runs from, and so how it
/// was installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ThisCopy {
    pub version: String,
    pub exe: PathBuf,
    pub install: Install,
    /// Where a downloaded package waits to be installed.
    pub cache: PathBuf,
    /// The window asking, run as an AppImage: the file to replace.
    pub window: Option<PathBuf>,
}

/// How this copy of Helix Toolbox was installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Install {
    /// A distribution package, in /usr/bin.
    Package,
    /// `cargo install`, in Cargo's bin directory.
    Cargo,
    /// Built in a checkout of the repository, whose root this is.
    Source(PathBuf),
    /// The launcher's copy, which it replaces with the latest release when
    /// it starts.
    Launcher,
    /// Downloaded by start.sh into a temporary directory for one run.
    Download,
    /// Somewhere else.
    Other,
}

/// The directories that say how a copy was installed.
pub struct Homes {
    pub cargo: PathBuf,
    pub data: PathBuf,
    pub temp: PathBuf,
}

/// How the copy at `exe` was installed; `checkout` says whether a directory
/// is the root of a Helix Toolbox checkout.
pub fn install_of(exe: &Path, homes: &Homes, checkout: &dyn Fn(&Path) -> bool) -> Install {
    if exe.starts_with("/usr/bin") || exe.starts_with("/usr/sbin") {
        Install::Package
    } else if exe.starts_with(homes.cargo.join("bin")) {
        Install::Cargo
    } else if exe.starts_with(homes.data.join("helix-toolbox"))
        || exe.starts_with(homes.data.join("helixforge"))
    {
        Install::Launcher
    } else if let Some(root) = exe
        .ancestors()
        .find(|dir| dir.file_name().is_some_and(|name| name == "target"))
        .and_then(Path::parent)
        .filter(|root| checkout(root))
    {
        Install::Source(root.to_path_buf())
    } else if exe.starts_with(&homes.temp) {
        Install::Download
    } else {
        Install::Other
    }
}

impl Places {
    pub fn here() -> Self {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|elapsed| elapsed.as_secs())
            .unwrap_or(0);
        Places {
            config,
            home,
            now,
            this: None,
        }
    }

    /// The same, with the running Helix Toolbox, `version` and at `exe`, for
    /// its own update.
    pub fn with_this(mut self, version: &str, exe: PathBuf) -> Self {
        let variable = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let homes = Homes {
            cargo: variable("CARGO_HOME").unwrap_or_else(|| self.home.join(".cargo")),
            data: variable("XDG_DATA_HOME").unwrap_or_else(|| self.home.join(".local/share")),
            temp: std::env::temp_dir(),
        };
        let checkout =
            |root: &Path| root.join("Cargo.toml").is_file() && root.join(".git").exists();
        let cache = variable("XDG_CACHE_HOME").unwrap_or_else(|| self.home.join(".cache"));
        self.this = Some(ThisCopy {
            version: version.to_string(),
            install: install_of(&exe, &homes, &checkout),
            exe,
            cache: cache.join("helix-toolbox/release"),
            window: None,
        });
        self
    }

    /// The same, when the window asks from an AppImage at `path` (the
    /// `APPIMAGE` variable), which its update then replaces.
    pub fn with_window(mut self, path: Option<PathBuf>) -> Self {
        if let Some(this) = &mut self.this {
            this.window = path.filter(|path| path.is_absolute());
        }
        self
    }
}

/// Where Helix Toolbox's releases are.
pub const RELEASES: &str = "https://github.com/Commanderx-code/Helix-toolbox";
const LATEST: &str = "https://api.github.com/repos/Commanderx-code/Helix-toolbox/releases/latest";

/// A release version's parts, for comparing.
fn release(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.trim_start_matches('v').split('.');
    let part = |part: Option<&str>| part?.parse::<u64>().ok();
    let found = (
        part(parts.next())?,
        part(parts.next())?,
        part(parts.next())?,
    );
    parts.next().is_none().then_some(found)
}

/// The release file a family's package manager installs on this machine.
pub fn package_asset(manager: PackageManager, version: &str) -> Option<String> {
    package_asset_for(manager, version, std::env::consts::ARCH)
}

/// The release file a family's package manager installs on `architecture`
/// (x86_64 or aarch64). Debian names the architectures its own way.
pub fn package_asset_for(
    manager: PackageManager,
    version: &str,
    architecture: &str,
) -> Option<String> {
    release(version)?;
    let debian = match architecture {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        _ => return None,
    };
    match manager {
        PackageManager::Arch => Some(format!(
            "helix-toolbox-{version}-1-{architecture}.pkg.tar.zst"
        )),
        PackageManager::Debian => Some(format!("helix-toolbox_{version}-1_{debian}.deb")),
        PackageManager::Fedora | PackageManager::Suse => {
            Some(format!("helix-toolbox-{version}-1.{architecture}.rpm"))
        }
        _ => None,
    }
}

/// The release's program on its own, for `architecture`.
pub fn program_asset(architecture: &str) -> Option<String> {
    helix_toolbox_core::ARCHITECTURES
        .contains(&architecture)
        .then(|| format!("helix-toolbox-linux-{architecture}"))
}

/// Debian's name for `architecture` (x86_64 or aarch64).
fn debian_architecture(architecture: &str) -> Option<&'static str> {
    match architecture {
        "x86_64" => Some("amd64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

/// The release file a family's package manager installs the window from on
/// `architecture`. Tauri numbers the Debian package without a release.
pub fn window_package_asset_for(
    manager: PackageManager,
    version: &str,
    architecture: &str,
) -> Option<String> {
    release(version)?;
    let debian = debian_architecture(architecture)?;
    match manager {
        PackageManager::Arch => Some(format!(
            "helix-toolbox-gui-{version}-1-{architecture}.pkg.tar.zst"
        )),
        PackageManager::Debian => Some(format!("helix-toolbox-gui_{version}_{debian}.deb")),
        PackageManager::Fedora | PackageManager::Suse => {
            Some(format!("helix-toolbox-gui-{version}-1.{architecture}.rpm"))
        }
        _ => None,
    }
}

/// The release's AppImage of the window for `architecture`; Tauri names the
/// x86_64 one amd64.
pub fn window_asset_for(version: &str, architecture: &str) -> Option<String> {
    release(version)?;
    let name = match architecture {
        "x86_64" => "amd64",
        "aarch64" => "aarch64",
        _ => return None,
    };
    Some(format!("helix-toolbox-gui_{version}_{name}.AppImage"))
}

/// Whether the window's package is installed, by the family's package tool.
fn window_installed(system: &dyn System, manager: PackageManager) -> bool {
    let command: &[&str] = match manager {
        PackageManager::Arch => &["pacman", "-Q", "helix-toolbox-gui"],
        PackageManager::Debian => &["dpkg-query", "-W", "helix-toolbox-gui"],
        PackageManager::Fedora | PackageManager::Suse => &["rpm", "-q", "helix-toolbox-gui"],
        _ => return false,
    };
    system.run(command).is_some_and(|(ok, _)| ok)
}

/// Whether pacman knows the helix-toolbox repository (INSTALL.md), which
/// then updates the packages with the rest of the system.
fn repository(system: &dyn System) -> bool {
    read(system, &["pacman-conf", "--repo-list"])
        .is_some_and(|text| text.lines().any(|line| line.trim() == "helix-toolbox"))
}

/// Helix Toolbox itself, updated the way it was installed.
fn this_channel(
    system: &dyn System,
    platform: &LinuxInfo,
    this: &ThisCopy,
    online: bool,
) -> Channel {
    let mut channel = Channel {
        // The id other programs know it by, from before 1.7.
        id: "helixforge".into(),
        name: "Helix Toolbox".into(),
        pending: None,
        note: String::new(),
        commands: Vec::new(),
        requires: Vec::new(),
        caveat: None,
    };
    // The latest release, asked of GitHub only in an online check.
    let latest = || -> Option<String> {
        let text = read(
            system,
            &[
                "curl",
                "-fsSL",
                "--max-time",
                "15",
                "--proto",
                "=https",
                LATEST,
            ],
        )?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let tag = json["tag_name"].as_str()?;
        release(tag)?;
        Some(tag.trim_start_matches('v').to_string())
    };
    let behind = |latest: &str| {
        let pending = match (release(&this.version), release(latest)) {
            (Some(current), Some(newest)) if newest > current => vec![Pending {
                name: "helix-toolbox".into(),
                from: Some(this.version.clone()),
                to: Some(latest.to_string()),
            }],
            _ => Vec::new(),
        };
        Some(pending)
    };
    let exe = this.exe.to_string_lossy().into_owned();
    let cache = this.cache.to_string_lossy().into_owned();
    let architecture = std::env::consts::ARCH;
    // A copy outside any package tool's reach that the user owns, and so can
    // put a newer release in place of.
    let own = this.install == Install::Other && system.owner(&this.exe) == Some(system.user().0);
    // GitHub is asked only in an online check, and only for a copy that can
    // act on the answer.
    let asks =
        matches!(this.install, Install::Package | Install::Cargo) || own || this.window.is_some();
    let newest = if online && asks { latest() } else { None };
    let fetch = |newest: &str, asset: &str| {
        words(&[exe.as_str(), "fetch-release", newest, asset, cache.as_str()])
    };
    let fetched = |asset: &str| this.cache.join(asset).to_string_lossy().into_owned();
    match &this.install {
        Install::Launcher | Install::Download => {
            channel.pending = Some(Vec::new());
            channel.note = format!(
                "{}; it fetches the latest release each time it starts",
                this.version
            );
        }
        Install::Other if !own => {
            channel.note = format!(
                "{} at {}, installed in a way Helix Toolbox cannot update; update it the way you installed it",
                this.version,
                this.exe.display()
            );
        }
        Install::Package | Install::Cargo | Install::Other if !online => {
            channel.note = format!("{}; r checks GitHub for a newer release", this.version);
        }
        Install::Package | Install::Cargo | Install::Other => match &newest {
            None => {
                channel.note = format!(
                    "{}; GitHub could not be asked for the latest release",
                    this.version
                );
            }
            Some(newest) => {
                channel.pending = behind(newest);
                channel.note = format!("{}, the latest release is {newest}", this.version);
                let waiting = channel.count().unwrap_or(0) > 0;
                if !waiting {
                } else if this.install == Install::Cargo {
                    if system.on_path("cargo") {
                        channel.commands =
                            vec![words(&["cargo", "install", "--locked", "helix-toolbox"])];
                        channel.requires = strings(&["cargo"]);
                    }
                } else if this.install == Install::Other {
                    // The user's own copy of the program: the newer one goes
                    // in its place.
                    if let Some(asset) = program_asset(architecture) {
                        channel.commands = vec![
                            fetch(newest, &asset),
                            words(&["install", "-m", "0755", &fetched(&asset), &exe]),
                        ];
                        channel.requires = strings(&["curl", "sha256sum"]);
                        channel.caveat = Some(format!(
                            "downloads the {newest} program from {RELEASES}/releases, checks it against the release's SHA256SUMS and puts it in place of {exe}"
                        ));
                    }
                } else if let Some(manager) = platform.manager {
                    let window = window_installed(system, manager);
                    if window && let Some(pending) = &mut channel.pending {
                        pending.push(Pending {
                            name: "helix-toolbox-gui".into(),
                            from: Some(this.version.clone()),
                            to: Some(newest.clone()),
                        });
                    }
                    if manager == PackageManager::Arch && repository(system) {
                        // The repository updates them with the system.
                        let mut command =
                            words(&["sudo", "pacman", "-Syu", "--needed", "helix-toolbox"]);
                        if window {
                            command.push("helix-toolbox-gui".into());
                        }
                        channel.commands = vec![command];
                        channel.requires = strings(&["pacman", "sudo"]);
                        channel.note.push_str(", in the helix-toolbox repository");
                        channel.caveat = Some(
                            "updates the whole system, the helix-toolbox repository with it: Arch does not support partial upgrades".into(),
                        );
                    } else if let Some(asset) = package_asset(manager, newest) {
                        let mut assets = vec![asset];
                        if window
                            && let Some(asset) =
                                window_package_asset_for(manager, newest, architecture)
                        {
                            assets.push(asset);
                        }
                        let install: &[&str] = match manager {
                            PackageManager::Arch => &["sudo", "pacman", "-U"],
                            PackageManager::Debian => &["sudo", "apt-get", "install"],
                            PackageManager::Fedora => &["sudo", "dnf", "install"],
                            _ => &["sudo", "zypper", "install", "--allow-unsigned-rpm"],
                        };
                        let mut install = words(install);
                        let mut commands = Vec::new();
                        for asset in &assets {
                            commands.push(fetch(newest, asset));
                            install.push(fetched(asset));
                        }
                        commands.push(install);
                        channel.commands = commands;
                        channel.requires = strings(&["curl", "sha256sum", "sudo"]);
                        let (what, them) = if assets.len() > 1 {
                            ("packages", "them")
                        } else {
                            ("package", "it")
                        };
                        channel.caveat = Some(format!(
                            "downloads the {newest} {what} from {RELEASES}/releases and checks {them} against the release's SHA256SUMS before installing"
                        ));
                    }
                }
            }
        },
        Install::Source(root) => {
            let git = |args: &[&str]| {
                let directory = root.to_string_lossy();
                let mut command = vec!["git", "-C", directory.as_ref()];
                command.extend(args);
                read(system, &command)
            };
            let head = git(&["rev-parse", "HEAD"]).map(|text| text.trim().to_string());
            let short = |hash: &str| hash.chars().take(12).collect::<String>();
            channel.note = format!(
                "a checkout at {}, {}",
                root.display(),
                head.as_deref()
                    .map(short)
                    .unwrap_or_else(|| "unknown".into())
            );
            if online {
                let remote = git(&["ls-remote", "origin", "refs/heads/main"])
                    .and_then(|text| text.split_whitespace().next().map(String::from));
                if let (Some(head), Some(remote)) = (&head, remote) {
                    channel.pending = Some(if *head == remote {
                        Vec::new()
                    } else {
                        vec![Pending {
                            name: "main".into(),
                            from: Some(short(head)),
                            to: Some(short(&remote)),
                        }]
                    });
                }
            } else {
                channel.note.push_str("; r compares it with GitHub");
            }
            let directory = root.to_string_lossy().into_owned();
            let manifest = root.join("Cargo.toml").to_string_lossy().into_owned();
            channel.commands = vec![
                // A checkout with local changes is left alone.
                words(&["git", "-C", &directory, "diff", "--quiet", "HEAD"]),
                words(&["git", "-C", &directory, "pull", "--ff-only"]),
                words(&[
                    "cargo",
                    "build",
                    "--release",
                    "--locked",
                    "--manifest-path",
                    &manifest,
                    "-p",
                    "helix-toolbox",
                ]),
            ];
            channel.requires = strings(&["git", "cargo"]);
            channel.caveat = Some(
                "fast-forwards the checkout and builds it; a checkout with local changes or other commits is left alone".into(),
            );
        }
    }
    // The window run as an AppImage: the newer one is downloaded, checked
    // and put in its place, whatever updates the command line.
    if let (Some(window), Some(newest)) = (&this.window, &newest)
        && let Some(asset) = window_asset_for(newest, architecture)
        && !behind(newest).unwrap_or_default().is_empty()
    {
        let window = window.to_string_lossy().into_owned();
        channel.pending.get_or_insert_with(Vec::new).push(Pending {
            name: "helix-toolbox-gui".into(),
            from: Some(this.version.clone()),
            to: Some(newest.clone()),
        });
        channel.commands.push(fetch(newest, &asset));
        channel
            .commands
            .push(words(&["install", "-m", "0755", &fetched(&asset), &window]));
        for tool in ["curl", "sha256sum"] {
            if !channel.requires.iter().any(|known| known == tool) {
                channel.requires.push(tool.into());
            }
        }
        let note = format!(
            "the window's AppImage at {window} is downloaded from the release, checked and replaced the same way"
        );
        channel.caveat = Some(match channel.caveat.take() {
            Some(caveat) => format!("{caveat}; {note}"),
            None => note,
        });
    }
    channel
}

fn words(command: &[&str]) -> Vec<String> {
    command.iter().map(|word| word.to_string()).collect()
}

fn strings(tools: &[&str]) -> Vec<String> {
    words(tools)
}

/// The standard output of a command, whatever its exit status: pacman -Qu
/// and checkupdates exit non-zero when nothing waits. None when it cannot
/// run.
fn read(system: &dyn System, command: &[&str]) -> Option<String> {
    system.run(command).map(|(_, text)| text)
}

/// Every channel this machine has. `online` also asks the network, through
/// read-only commands (checkupdates, the AUR helper, Flatpak's remotes).
pub fn overview(
    system: &dyn System,
    platform: &LinuxInfo,
    places: &Places,
    online: bool,
) -> Vec<Channel> {
    let mut channels = Vec::new();
    match platform.manager {
        Some(PackageManager::Arch) if system.on_path("pacman") => {
            let (pending, note) = if online && system.on_path("checkupdates") {
                (
                    read(system, &["checkupdates"]).map(|text| arrows(&text)),
                    "checked online with checkupdates",
                )
            } else {
                (
                    read(system, &["pacman", "-Qu"]).map(|text| arrows(&text)),
                    "as of pacman's last sync",
                )
            };
            channels.push(Channel {
                id: "pacman".into(),
                name: "Arch packages".into(),
                pending,
                note: note.into(),
                commands: vec![words(&["sudo", "pacman", "-Syu"])],
                requires: strings(&["pacman", "sudo"]),
                caveat: Some(
                    "updates the whole system: Arch does not support partial upgrades".into(),
                ),
            });
            if let Some(helper) = ["paru", "yay"]
                .into_iter()
                .find(|tool| system.on_path(tool))
            {
                let pending = online
                    .then(|| read(system, &[helper, "-Qua"]).map(|text| arrows(&text)))
                    .flatten();
                channels.push(Channel {
                    id: "aur".into(),
                    name: format!("AUR ({helper})"),
                    note: if online {
                        format!("checked online with {helper} -Qua")
                    } else {
                        "the AUR is only asked in an online check".into()
                    },
                    pending,
                    commands: vec![words(&[helper, "-Sua"])],
                    requires: strings(&[helper]),
                    caveat: Some(
                        "builds AUR packages from their PKGBUILDs; read them when asked".into(),
                    ),
                });
            }
        }
        Some(PackageManager::Debian) if system.on_path("apt") => {
            channels.push(Channel {
                id: "apt".into(),
                name: "Debian packages".into(),
                pending: read(system, &["apt", "list", "--upgradable"]).map(|text| apt(&text)),
                note: "as of the last apt update, which the update runs first".into(),
                commands: vec![
                    words(&["sudo", "apt-get", "update"]),
                    words(&["sudo", "apt-get", "upgrade"]),
                ],
                requires: strings(&["apt-get", "sudo"]),
                caveat: None,
            });
        }
        Some(PackageManager::Fedora) if system.on_path("dnf") => {
            let command: &[&str] = if online {
                &["dnf", "-q", "list", "--upgrades"]
            } else {
                &["dnf", "-C", "-q", "list", "--upgrades"]
            };
            channels.push(Channel {
                id: "dnf".into(),
                name: "Fedora packages".into(),
                pending: read(system, command).map(|text| dnf(&text)),
                note: if online {
                    "checked online with dnf".into()
                } else {
                    "from dnf's cached metadata".into()
                },
                commands: vec![words(&["sudo", "dnf", "upgrade"])],
                requires: strings(&["dnf", "sudo"]),
                caveat: None,
            });
        }
        Some(PackageManager::Suse) if system.on_path("zypper") => {
            // Tumbleweed moves as a whole, through a distribution upgrade.
            let rolling = platform.id.contains("tumbleweed");
            channels.push(Channel {
                id: "zypper".into(),
                name: "openSUSE packages".into(),
                pending: read(system, &["zypper", "-q", "--no-refresh", "list-updates"])
                    .map(|text| zypper(&text)),
                note: "from zypper's cached repositories".into(),
                commands: vec![words(if rolling {
                    &["sudo", "zypper", "dist-upgrade"]
                } else {
                    &["sudo", "zypper", "update"]
                })],
                requires: strings(&["zypper", "sudo"]),
                caveat: None,
            });
        }
        _ => {}
    }
    if system.on_path("flatpak") {
        for (installation, id, name) in [
            ("--system", "flatpak-system", "Flatpak (system)"),
            ("--user", "flatpak-user", "Flatpak (user)"),
        ] {
            let installed = read(
                system,
                &["flatpak", "list", installation, "--columns=application"],
            )
            .unwrap_or_default();
            if installed.trim().is_empty() {
                continue;
            }
            let mut command = vec![
                "flatpak",
                "remote-ls",
                "--updates",
                installation,
                "--columns=application,version",
            ];
            if !online {
                command.push("--cached");
            }
            let system_wide = installation == "--system";
            channels.push(Channel {
                id: id.into(),
                name: name.into(),
                pending: read(system, &command).map(|text| columns(&text)),
                note: if online {
                    "checked online with flatpak remote-ls".into()
                } else {
                    "from Flatpak's cached remote metadata".into()
                },
                commands: vec![if system_wide {
                    words(&["sudo", "flatpak", "update", "-y", "--system"])
                } else {
                    words(&["flatpak", "update", "-y", "--user"])
                }],
                requires: if system_wide {
                    strings(&["flatpak", "sudo"])
                } else {
                    strings(&["flatpak"])
                },
                caveat: None,
            });
        }
    }
    if let Some(channel) = home_manager(system, places) {
        channels.push(channel);
    }
    if system.exists(Path::new("/etc/NIXOS")) {
        let flake = Path::new("/etc/nixos/flake.nix");
        let (commands, caveat) = if system.exists(flake) {
            (
                vec![
                    words(&["sudo", "nix", "flake", "update", "--flake", "/etc/nixos"]),
                    words(&["sudo", "nixos-rebuild", "switch", "--flake", "/etc/nixos"]),
                ],
                Some("updates /etc/nixos/flake.lock".to_string()),
            )
        } else {
            (
                vec![words(&["sudo", "nixos-rebuild", "switch", "--upgrade"])],
                None,
            )
        };
        channels.push(Channel {
            id: "nixos".into(),
            name: "NixOS".into(),
            pending: None,
            note: lock_age(system, Path::new("/etc/nixos/flake.lock"), places.now)
                .unwrap_or_else(|| "NixOS counts nothing before it rebuilds".into()),
            commands,
            requires: strings(&["nixos-rebuild", "sudo"]),
            caveat,
        });
    }
    let manifest = places.home.join(".nix-profile/manifest.json");
    if system.on_path("nix")
        && let Some(text) = system.read(&manifest)
        && let Ok(json) = serde_json::from_str::<serde_json::Value>(&text)
    {
        let count = match &json["elements"] {
            serde_json::Value::Object(map) => map.len(),
            serde_json::Value::Array(list) => list.len(),
            _ => 0,
        };
        if count > 0 {
            channels.push(Channel {
                id: "nix-profile".into(),
                name: "Nix profile".into(),
                pending: None,
                note: format!(
                    "{count} package{} installed with nix profile",
                    if count == 1 { "" } else { "s" }
                ),
                commands: vec![words(&["nix", "profile", "upgrade", "--all"])],
                requires: strings(&["nix"]),
                caveat: None,
            });
        }
    }
    if let Some(this) = &places.this {
        channels.push(this_channel(system, platform, this, online));
    }
    // Package names, versions, flake input names and paths come from outside
    // Helix Toolbox: keep terminal control characters out of what is shown.
    channels.into_iter().map(cleaned).collect()
}

fn cleaned(mut channel: Channel) -> Channel {
    channel.name = printable(&channel.name);
    channel.note = printable(&channel.note);
    channel.caveat = channel.caveat.as_deref().map(printable);
    for pending in channel.pending.iter_mut().flatten() {
        pending.name = printable(&pending.name);
        pending.from = pending.from.as_deref().map(printable);
        pending.to = pending.to.as_deref().map(printable);
    }
    channel
}

/// Home Manager with a flake in its configuration directory.
fn home_manager(system: &dyn System, places: &Places) -> Option<Channel> {
    if !system.on_path("home-manager") {
        return None;
    }
    let directory = places.config.join("home-manager");
    if !system.exists(&directory.join("flake.nix")) {
        return None;
    }
    // A link to a dotfiles checkout is where the lock file really changes.
    let shown = system
        .canonical(&directory)
        .unwrap_or_else(|| directory.clone());
    let flake = directory.to_string_lossy().into_owned();
    Some(Channel {
        id: "home-manager".into(),
        name: "Home Manager".into(),
        pending: None,
        note: lock_age(system, &directory.join("flake.lock"), places.now)
            .unwrap_or_else(|| "its flake inputs are not locked yet".into()),
        commands: vec![
            words(&["nix", "flake", "update", "--flake", &flake]),
            words(&["home-manager", "switch", "--flake", &flake]),
        ],
        requires: strings(&["nix", "home-manager"]),
        caveat: Some(format!(
            "updates {}; commit it there if you keep it in Git",
            shown.join("flake.lock").display()
        )),
    })
}

/// How old the locked nixpkgs (or else the oldest input) is.
fn lock_age(system: &dyn System, lock: &Path, now: u64) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(&system.read(lock)?).ok()?;
    let nodes = json["nodes"].as_object()?;
    let when = |node: &serde_json::Value| node["locked"]["lastModified"].as_u64();
    let (name, time) = match nodes.get("nixpkgs").and_then(when) {
        Some(time) => ("nixpkgs", time),
        None => nodes
            .iter()
            .filter_map(|(name, node)| Some((name.as_str(), when(node)?)))
            .min_by_key(|(_, time)| *time)?,
    };
    let days = now.saturating_sub(time) / 86_400;
    Some(format!(
        "{name} locked at {}, {}",
        date(time),
        match days {
            0 => "today".to_string(),
            1 => "1 day ago".to_string(),
            days => format!("{days} days ago"),
        }
    ))
}

/// A UTC date, YYYY-MM-DD.
pub fn date(seconds: u64) -> String {
    // Days to a civil date (Howard Hinnant's algorithm).
    let days = (seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_index = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_index + 2) / 5 + 1;
    let month = if month_index < 10 {
        month_index + 3
    } else {
        month_index - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `name old -> new` lines: pacman -Qu, checkupdates, paru and yay -Qua.
pub fn arrows(text: &str) -> Vec<Pending> {
    text.lines()
        .filter(|line| !line.contains("[ignored]"))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let from = words.next()?;
            (words.next()? == "->").then_some(())?;
            Some(Pending {
                name: name.to_string(),
                from: Some(from.to_string()),
                to: Some(words.next()?.to_string()),
            })
        })
        .collect()
}

/// `name/suite new arch [upgradable from: old]` lines from apt list.
pub fn apt(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let (head, from) = line.split_once("[upgradable from:")?;
            let mut words = head.split_whitespace();
            let name = words.next()?.split('/').next()?;
            Some(Pending {
                name: name.to_string(),
                from: Some(from.trim().trim_end_matches(']').trim().to_string()),
                to: words.next().map(String::from),
            })
        })
        .collect()
}

/// `name.arch version repository` lines from dnf list --upgrades.
pub fn dnf(text: &str) -> Vec<Pending> {
    let mut found: Vec<Pending> = crate::inventory::parse_dnf_upgrades(text)
        .into_iter()
        .map(|(name, to)| Pending {
            name,
            from: None,
            to: Some(to),
        })
        .collect();
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

/// `v | repository | name | current | available | arch` rows from zypper.
pub fn zypper(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            (cells.len() >= 5 && cells[0] == "v").then(|| Pending {
                name: cells[2].to_string(),
                from: Some(cells[3].to_string()),
                to: Some(cells[4].to_string()),
            })
        })
        .collect()
}

/// Tab-separated `application version` columns from flatpak remote-ls.
pub fn columns(text: &str) -> Vec<Pending> {
    text.lines()
        .filter_map(|line| {
            let mut cells = line.split('\t').map(str::trim);
            let name = cells.next().filter(|name| !name.is_empty())?;
            Some(Pending {
                name: name.to_string(),
                from: None,
                to: cells
                    .next()
                    .filter(|version| !version.is_empty())
                    .map(String::from),
            })
        })
        .collect()
}

/// A plan updating the channels in order, each through its own tool, one
/// after another; a later channel waits for the native packages.
pub fn plan(channels: &[&Channel]) -> Plan {
    let mut plan = Plan::default();
    for channel in channels {
        if channel.commands.is_empty() {
            plan.notes.push(format!(
                "{}: nothing to run ({})",
                channel.name, channel.note
            ));
            continue;
        }
        plan.privileged |= channel.requires.iter().any(|tool| tool == "sudo");
        plan.risk = plan.risk.max(match channel.id.as_str() {
            "pacman" | "apt" | "dnf" | "zypper" | "nixos" => Risk::High,
            _ => Risk::Medium,
        });
        plan.requirements.extend(channel.requires.iter().cloned());
        if let Some(caveat) = &channel.caveat {
            plan.notes.push(format!("{}: {caveat}", channel.name));
        }
        plan.steps.push(Step::Update {
            id: format!("update-{}", channel.id),
            name: format!("Update {}", channel.name),
            source: Source {
                label: channel.name.clone(),
                commands: channel.commands.clone(),
                requires: channel.requires.clone(),
                probe: None,
            },
            targets: Vec::new(),
        });
    }
    // The AUR builds against the repositories' packages: only after them.
    let native = plan.steps.iter().position(|step| {
        matches!(
            step.id(),
            "update-pacman" | "update-apt" | "update-dnf" | "update-zypper"
        )
    });
    plan.depends = plan
        .steps
        .iter()
        .map(|step| match native {
            Some(index) if step.id() == "update-aur" => vec![index],
            _ => Vec::new(),
        })
        .collect();
    plan.requirements.sort();
    plan.requirements.dedup();
    plan
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Fake;

    fn arch() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\n").unwrap()
    }

    fn places() -> Places {
        Places {
            config: PathBuf::from("/home/me/.config"),
            home: PathBuf::from("/home/me"),
            // 2026-10-15
            now: 1_792_022_400,
            this: None,
        }
    }

    fn machine() -> Fake {
        let mut fake = Fake {
            tools: [
                "pacman",
                "checkupdates",
                "paru",
                "flatpak",
                "home-manager",
                "nix",
            ]
            .map(String::from)
            .to_vec(),
            ..Fake::default()
        };
        fake.commands.insert(
            "pacman -Qu".into(),
            (
                true,
                "ripgrep 15.2.0-1 -> 15.3.0-1\nlinux 6.1-1 -> 6.2-1 [ignored]\n".into(),
            ),
        );
        fake.commands.insert(
            "checkupdates".into(),
            (
                true,
                "ripgrep 15.2.0-1 -> 15.3.0-1\nmesa 1-1 -> 2-1\n".into(),
            ),
        );
        // paru exits 1 when nothing waits.
        fake.commands
            .insert("paru -Qua".into(), (false, String::new()));
        fake.commands.insert(
            "flatpak list --system --columns=application".into(),
            (true, "org.zapzap.ZapZap\n".into()),
        );
        fake.commands.insert(
            "flatpak list --user --columns=application".into(),
            (true, String::new()),
        );
        fake.commands.insert(
            "flatpak remote-ls --updates --system --columns=application,version --cached".into(),
            (true, "org.zapzap.ZapZap\t7.6\n".into()),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.nix"),
            "{}".into(),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.lock"),
            r#"{"nodes":{"nixpkgs":{"locked":{"lastModified":1791331200}},"systems":{"locked":{"lastModified":1681000000}},"root":{}}}"#.into(),
        );
        fake.canonical.insert(
            PathBuf::from("/home/me/.config/home-manager"),
            PathBuf::from("/home/me/dotfiles/home-manager"),
        );
        fake
    }

    #[test]
    fn offline_the_overview_reads_what_the_tools_know() {
        let channels = overview(&machine(), &arch(), &places(), false);
        let ids: Vec<&str> = channels.iter().map(|channel| channel.id.as_str()).collect();
        assert_eq!(ids, ["pacman", "aur", "flatpak-system", "home-manager"]);
        assert_eq!(
            channels[0].summary(),
            "1 update",
            "ignored packages are left out"
        );
        assert_eq!(
            channels[0].pending.as_ref().unwrap()[0],
            Pending {
                name: "ripgrep".into(),
                from: Some("15.2.0-1".into()),
                to: Some("15.3.0-1".into()),
            }
        );
        assert_eq!(channels[1].pending, None, "the AUR only online");
        assert_eq!(channels[2].summary(), "1 update");
        let home = &channels[3];
        assert_eq!(home.note, "nixpkgs locked at 2026-10-07, 8 days ago");
        assert_eq!(
            home.command_line(),
            "nix flake update --flake /home/me/.config/home-manager && home-manager switch --flake /home/me/.config/home-manager"
        );
        assert!(
            home.caveat
                .as_deref()
                .unwrap()
                .contains("/home/me/dotfiles/home-manager/flake.lock")
        );
    }

    #[test]
    fn online_the_overview_asks_read_only_commands() {
        let channels = overview(&machine(), &arch(), &places(), true);
        assert_eq!(channels[0].summary(), "2 updates");
        assert!(channels[0].note.contains("checkupdates"));
        assert_eq!(channels[1].summary(), "up to date");
        // The online Flatpak command was not given, so it is unknown.
        assert_eq!(channels[2].pending, None);
    }

    #[test]
    fn control_characters_from_the_tools_are_not_shown() {
        let mut fake = machine();
        fake.commands.insert(
            "pacman -Qu".into(),
            (true, "evil\x1b]0;x\x07 1 -> 2\x1b[2J\n".into()),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/home-manager/flake.lock"),
            r#"{"nodes":{"bad\u001b[31m":{"locked":{"lastModified":1791331200}},"root":{}}}"#
                .into(),
        );
        let channels = overview(&fake, &arch(), &places(), false);
        let shown = format!("{channels:?}");
        assert!(
            !shown.contains('\x1b') && !shown.contains('\x07'),
            "{shown}"
        );
        let pending = &channels[0].pending.as_ref().unwrap()[0];
        assert_eq!(pending.name, "evil?]0;x?");
        assert_eq!(pending.to.as_deref(), Some("2?[2J"));
        assert!(channels[3].note.starts_with("bad?[31m locked at"));
        // The plan's commands are shown cleaned, though they run as given.
        let mut odd = channels[3].clone();
        odd.commands = vec![vec!["nix".into(), "/home/\x1b[2Jme".into()]];
        let plan = plan(&[&odd]);
        assert_eq!(plan.steps[0].display(), "nix /home/?[2Jme");
    }

    fn this(install: Install) -> ThisCopy {
        ThisCopy {
            version: "1.0.3".into(),
            exe: PathBuf::from("/usr/bin/helix-toolbox"),
            install,
            cache: PathBuf::from("/home/me/.cache/helix-toolbox/release"),
            window: None,
        }
    }

    fn latest(fake: &mut Fake, tag: &str) {
        fake.commands.insert(
            format!("curl -fsSL --max-time 15 --proto =https {LATEST}"),
            (true, format!(r#"{{"tag_name": "{tag}", "name": "x"}}"#)),
        );
    }

    fn only_this(fake: &Fake, install: Install, online: bool) -> Channel {
        let mut places = places();
        places.this = Some(this(install));
        overview(fake, &arch(), &places, online)
            .into_iter()
            .find(|channel| channel.id == "helixforge")
            .unwrap()
    }

    #[test]
    fn where_it_runs_from_says_how_it_was_installed() {
        let homes = Homes {
            cargo: PathBuf::from("/home/me/.cargo"),
            data: PathBuf::from("/home/me/.local/share"),
            temp: PathBuf::from("/tmp"),
        };
        let checkout = |root: &Path| root == Path::new("/home/me/Helix-toolbox");
        let of = |path: &str| install_of(Path::new(path), &homes, &checkout);
        assert_eq!(of("/usr/bin/helix-toolbox"), Install::Package);
        assert_eq!(of("/home/me/.cargo/bin/helix-toolbox"), Install::Cargo);
        assert_eq!(
            of("/home/me/.local/share/helixforge/helix-toolbox-linux-x86_64"),
            Install::Launcher
        );
        assert_eq!(
            of("/home/me/Helix-toolbox/target/release/helix-toolbox"),
            Install::Source(PathBuf::from("/home/me/Helix-toolbox"))
        );
        assert_eq!(
            of("/tmp/tmp.abc/helix-toolbox-linux-x86_64"),
            Install::Download
        );
        assert_eq!(
            of("/tmp/other/target/release/helix-toolbox"),
            Install::Download
        );
        assert_eq!(of("/opt/hf/helix-toolbox"), Install::Other);
    }

    #[test]
    fn a_package_install_downloads_checks_and_installs_the_newer_package() {
        let mut fake = machine();
        latest(&mut fake, "v1.0.4");
        let offline = only_this(&fake, Install::Package, false);
        assert_eq!(offline.pending, None);
        assert!(offline.note.contains("r checks GitHub"), "{}", offline.note);
        let channel = only_this(&fake, Install::Package, true);
        assert_eq!(channel.summary(), "1 update");
        assert_eq!(channel.note, "1.0.3, the latest release is 1.0.4");
        // The package for this machine's architecture.
        let file = format!(
            "helix-toolbox-1.0.4-1-{}.pkg.tar.zst",
            std::env::consts::ARCH
        );
        assert_eq!(
            channel.command_line(),
            format!(
                "/usr/bin/helix-toolbox fetch-release 1.0.4 {file} /home/me/.cache/helix-toolbox/release && sudo pacman -U /home/me/.cache/helix-toolbox/release/{file}"
            )
        );
        // Up to date: nothing to run.
        latest(&mut fake, "v1.0.3");
        let current = only_this(&fake, Install::Package, true);
        assert_eq!(current.summary(), "up to date");
        assert!(current.commands.is_empty());
        let plan = plan(&[&current]);
        assert!(plan.steps.is_empty());
        assert!(plan.notes[0].contains("nothing to run"), "{:?}", plan.notes);
        // A tag that is not a release version is not believed.
        latest(&mut fake, "v1.0.4; rm -rf /");
        assert_eq!(only_this(&fake, Install::Package, true).pending, None);
    }

    #[test]
    fn the_window_package_is_fetched_and_installed_with_the_program() {
        let mut fake = machine();
        latest(&mut fake, "v1.0.4");
        fake.commands.insert(
            "pacman -Q helix-toolbox-gui".into(),
            (true, "helix-toolbox-gui 1.0.3-1\n".into()),
        );
        let channel = only_this(&fake, Install::Package, true);
        assert_eq!(channel.summary(), "2 updates");
        assert_eq!(
            channel.pending.as_ref().unwrap()[1].name,
            "helix-toolbox-gui"
        );
        let arch = std::env::consts::ARCH;
        let cache = "/home/me/.cache/helix-toolbox/release";
        let program = format!("helix-toolbox-1.0.4-1-{arch}.pkg.tar.zst");
        let window = format!("helix-toolbox-gui-1.0.4-1-{arch}.pkg.tar.zst");
        assert_eq!(
            channel.command_line(),
            format!(
                "/usr/bin/helix-toolbox fetch-release 1.0.4 {program} {cache} && /usr/bin/helix-toolbox fetch-release 1.0.4 {window} {cache} && sudo pacman -U {cache}/{program} {cache}/{window}"
            )
        );
        assert!(channel.caveat.as_deref().unwrap().contains("packages"));
    }

    #[test]
    fn with_the_repository_pacman_updates_them_with_the_system() {
        let mut fake = machine();
        latest(&mut fake, "v1.0.4");
        fake.commands.insert(
            "pacman-conf --repo-list".into(),
            (true, "garuda\ncore\nextra\nhelix-toolbox\n".into()),
        );
        let channel = only_this(&fake, Install::Package, true);
        assert_eq!(channel.summary(), "1 update");
        assert_eq!(
            channel.command_line(),
            "sudo pacman -Syu --needed helix-toolbox"
        );
        assert!(
            channel.note.ends_with("in the helix-toolbox repository"),
            "{}",
            channel.note
        );
        assert_eq!(channel.requires, vec!["pacman", "sudo"]);
        fake.commands.insert(
            "pacman -Q helix-toolbox-gui".into(),
            (true, "helix-toolbox-gui 1.0.3-1\n".into()),
        );
        let both = only_this(&fake, Install::Package, true);
        assert_eq!(both.summary(), "2 updates");
        assert_eq!(
            both.command_line(),
            "sudo pacman -Syu --needed helix-toolbox helix-toolbox-gui"
        );
        // Up to date: the repository has nothing to do either.
        latest(&mut fake, "v1.0.3");
        assert!(only_this(&fake, Install::Package, true).commands.is_empty());
    }

    #[test]
    fn the_windows_appimage_is_replaced_beside_whatever_updates_the_program() {
        let mut fake = machine();
        latest(&mut fake, "v1.0.4");
        let mut places = places();
        let mut copy = this(Install::Package);
        copy.window = Some(PathBuf::from(
            "/home/me/AppImages/helix-toolbox-gui.AppImage",
        ));
        places.this = Some(copy);
        let find = |fake: &Fake, online| {
            overview(fake, &arch(), &places, online)
                .into_iter()
                .find(|channel| channel.id == "helixforge")
                .unwrap()
        };
        let channel = find(&fake, true);
        assert_eq!(channel.summary(), "2 updates");
        let arch = std::env::consts::ARCH;
        let cache = "/home/me/.cache/helix-toolbox/release";
        let appimage = window_asset_for("1.0.4", arch).unwrap();
        assert!(appimage.ends_with(".AppImage"), "{appimage}");
        let line = channel.command_line();
        assert!(
            line.starts_with("/usr/bin/helix-toolbox fetch-release 1.0.4 helix-toolbox-1.0.4-1-"),
            "{line}"
        );
        assert!(line.ends_with(&format!(
            "&& /usr/bin/helix-toolbox fetch-release 1.0.4 {appimage} {cache} && install -m 0755 {cache}/{appimage} /home/me/AppImages/helix-toolbox-gui.AppImage"
        )), "{line}");
        assert_eq!(channel.requires, vec!["curl", "sha256sum", "sudo"]);
        assert!(channel.caveat.as_deref().unwrap().contains("AppImage"));
        // Offline, or up to date: the AppImage is left alone.
        assert!(find(&fake, false).commands.is_empty());
        latest(&mut fake, "v1.0.3");
        let current = find(&fake, true);
        assert_eq!(current.summary(), "up to date");
        assert!(current.commands.is_empty());
        assert_eq!(
            window_asset_for("1.0.4", "x86_64").as_deref(),
            Some("helix-toolbox-gui_1.0.4_amd64.AppImage")
        );
        assert_eq!(
            window_asset_for("1.0.4", "aarch64").as_deref(),
            Some("helix-toolbox-gui_1.0.4_aarch64.AppImage")
        );
        assert_eq!(window_asset_for("1.0", "x86_64"), None);
        assert_eq!(
            window_package_asset_for(PackageManager::Debian, "1.0.4", "aarch64").as_deref(),
            Some("helix-toolbox-gui_1.0.4_arm64.deb")
        );
        assert_eq!(
            window_package_asset_for(PackageManager::Fedora, "1.0.4", "x86_64").as_deref(),
            Some("helix-toolbox-gui-1.0.4-1.x86_64.rpm")
        );
    }

    #[test]
    fn package_files_follow_each_family_and_architecture() {
        let file = |manager, architecture| package_asset_for(manager, "1.2.3", architecture);
        assert_eq!(
            file(PackageManager::Debian, "x86_64").as_deref(),
            Some("helix-toolbox_1.2.3-1_amd64.deb")
        );
        assert_eq!(
            file(PackageManager::Suse, "x86_64").as_deref(),
            Some("helix-toolbox-1.2.3-1.x86_64.rpm")
        );
        assert_eq!(
            file(PackageManager::Arch, "aarch64").as_deref(),
            Some("helix-toolbox-1.2.3-1-aarch64.pkg.tar.zst")
        );
        assert_eq!(
            file(PackageManager::Debian, "aarch64").as_deref(),
            Some("helix-toolbox_1.2.3-1_arm64.deb")
        );
        assert_eq!(
            file(PackageManager::Fedora, "aarch64").as_deref(),
            Some("helix-toolbox-1.2.3-1.aarch64.rpm")
        );
        assert_eq!(file(PackageManager::Debian, "riscv64"), None);
        assert_eq!(file(PackageManager::Alpine, "x86_64"), None);
        assert_eq!(package_asset(PackageManager::Arch, "1.2.3/../x"), None);
        // This machine's own.
        assert_eq!(
            package_asset(PackageManager::Fedora, "1.2.3"),
            file(PackageManager::Fedora, std::env::consts::ARCH)
        );
        assert_eq!(
            program_asset("aarch64").as_deref(),
            Some("helix-toolbox-linux-aarch64")
        );
        assert_eq!(program_asset("../x"), None);
    }

    #[test]
    fn cargo_launcher_source_and_unknown_copies_each_say_how_they_update() {
        let mut fake = machine();
        fake.tools.push("cargo".into());
        latest(&mut fake, "v2.0.0");
        let cargo = only_this(&fake, Install::Cargo, true);
        assert_eq!(cargo.command_line(), "cargo install --locked helix-toolbox");

        let launcher = only_this(&fake, Install::Launcher, true);
        assert_eq!(launcher.summary(), "up to date");
        assert!(launcher.commands.is_empty());
        assert!(launcher.note.contains("each time it starts"));

        // A copy the user does not own (root's, in /usr/local/bin) is left
        // to whoever put it there.
        fake.owners
            .insert(PathBuf::from("/usr/bin/helix-toolbox"), 0);
        let other = only_this(&fake, Install::Other, true);
        assert_eq!(other.pending, None);
        assert!(other.note.contains("cannot update"), "{}", other.note);
        fake.owners.clear();
        // The user's own copy: the newer program goes in its place.
        let own = only_this(&fake, Install::Other, true);
        assert_eq!(own.summary(), "1 update");
        let program = format!("helix-toolbox-linux-{}", std::env::consts::ARCH);
        assert_eq!(
            own.command_line(),
            format!(
                "/usr/bin/helix-toolbox fetch-release 2.0.0 {program} /home/me/.cache/helix-toolbox/release && install -m 0755 /home/me/.cache/helix-toolbox/release/{program} /usr/bin/helix-toolbox"
            )
        );
        assert!(own.caveat.as_deref().unwrap().contains("in place of"));
        let offline = only_this(&fake, Install::Other, false);
        assert!(offline.note.contains("r checks GitHub"), "{}", offline.note);

        let root = PathBuf::from("/home/me/Helix-toolbox");
        fake.commands.insert(
            "git -C /home/me/Helix-toolbox rev-parse HEAD".into(),
            (true, "aaaaaaaaaaaaaaaaaaaa\n".into()),
        );
        fake.commands.insert(
            "git -C /home/me/Helix-toolbox ls-remote origin refs/heads/main".into(),
            (true, "bbbbbbbbbbbbbbbbbbbb\trefs/heads/main\n".into()),
        );
        let source = only_this(&fake, Install::Source(root), true);
        assert_eq!(source.summary(), "1 update");
        assert_eq!(
            source.command_line(),
            "git -C /home/me/Helix-toolbox diff --quiet HEAD && git -C /home/me/Helix-toolbox pull --ff-only && cargo build --release --locked --manifest-path /home/me/Helix-toolbox/Cargo.toml -p helix-toolbox"
        );
    }

    #[test]
    fn a_machine_without_the_tools_has_no_channels() {
        let fake = Fake::default();
        assert!(overview(&fake, &arch(), &places(), false).is_empty());
    }

    #[test]
    fn the_parsers_read_each_tool() {
        let apt = apt("Listing...\nbash/stable 5.2.15-2+b3 amd64 [upgradable from: 5.2.15-2+b2]\n");
        assert_eq!(apt[0].name, "bash");
        assert_eq!(apt[0].from.as_deref(), Some("5.2.15-2+b2"));
        assert_eq!(apt[0].to.as_deref(), Some("5.2.15-2+b3"));
        let zypper = zypper(
            "S | Repository | Name | Current Version | Available Version | Arch\n--+---\nv | Main | vim | 9.0-1 | 9.1-1 | x86_64\n",
        );
        assert_eq!(zypper.len(), 1);
        assert_eq!(zypper[0].to.as_deref(), Some("9.1-1"));
        let dnf = dnf("Available Upgrades\ngit.x86_64 2.47.0-1.fc41 updates\n");
        assert_eq!(dnf[0].name, "git");
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_792_022_400), "2026-10-15");
    }

    #[test]
    fn the_plan_runs_each_tool_in_order_and_the_aur_after_pacman() {
        let channels = overview(&machine(), &arch(), &places(), false);
        let chosen: Vec<&Channel> = channels.iter().collect();
        let plan = plan(&chosen);
        let ids: Vec<&str> = plan.steps.iter().map(|step| step.id()).collect();
        assert_eq!(
            ids,
            [
                "update-pacman",
                "update-aur",
                "update-flatpak-system",
                "update-home-manager"
            ]
        );
        assert_eq!(plan.depends[1], [0]);
        assert!(plan.privileged);
        assert_eq!(plan.risk, Risk::High);
        assert!(
            plan.notes
                .iter()
                .any(|note| note.contains("partial upgrades"))
        );
        assert_eq!(plan.steps[0].display(), "sudo pacman -Syu");
        assert_eq!(plan.steps[1].display(), "paru -Sua");
    }
}
