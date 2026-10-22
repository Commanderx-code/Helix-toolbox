//! What is installed, where from, at which version, and whether an update is
//! waiting: one snapshot from a few read-only queries (no root, no network:
//! updates come from the repository metadata already on the machine), so a
//! list of entries is answered by lookups.

use crate::{PackageState, Plan, Probe, Source, Step, UpdateTarget};
use helix_toolbox_core::{Entry, PackageManager, Risk};
use helix_toolbox_platform::LinuxInfo;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

/// How an entry stands, for showing it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Status {
    /// One of its own packages is installed and current. `shadowed` names
    /// a Nix install of its command that comes first on the PATH.
    Installed {
        source: String,
        version: Option<String>,
        shadowed: Option<String>,
    },
    /// Installed, and the repository metadata has a newer version.
    Update {
        source: String,
        version: Option<String>,
        latest: Option<String>,
        shadowed: Option<String>,
    },
    /// Its command comes from Nix: a profile, Home Manager or NixOS.
    Managed { by: String, version: Option<String> },
    /// Its command is on the PATH from an install Helix Toolbox cannot place.
    Elsewhere { path: PathBuf },
    /// Not installed; `replaces` lists installed packages it conflicts with.
    Absent { replaces: Vec<String> },
}

impl Status {
    /// The short word the interface shows.
    pub fn label(&self) -> &'static str {
        match self {
            Status::Installed { .. } | Status::Elsewhere { .. } => "Installed",
            Status::Update { .. } => "Update",
            Status::Managed { .. } => "Managed",
            Status::Absent { .. } => "Not installed",
        }
    }

    /// Where it came from and at which version, as one phrase, printable.
    pub fn detail(&self) -> String {
        helix_toolbox_core::printable(&self.raw_detail())
    }

    fn raw_detail(&self) -> String {
        let with = |source: &str, version: &Option<String>| match version {
            Some(version) => format!("{source} {version}"),
            None => source.to_string(),
        };
        let shadow = |text: String, shadowed: &Option<String>| match shadowed {
            Some(shadowed) => format!("{text}; {shadowed} runs first"),
            None => text,
        };
        match self {
            Status::Installed {
                source,
                version,
                shadowed,
            } => shadow(with(source, version), shadowed),
            Status::Update {
                source,
                version,
                latest,
                shadowed,
            } => shadow(
                match (version, latest) {
                    (Some(version), Some(latest)) => format!("{source} {version} → {latest}"),
                    _ => with(source, version),
                },
                shadowed,
            ),
            Status::Managed { by, version } => with(by, version),
            Status::Elsewhere { path } => path.display().to_string(),
            Status::Absent { replaces } if !replaces.is_empty() => {
                format!("would replace {}", replaces.join(", "))
            }
            Status::Absent { .. } => String::new(),
        }
    }
}

/// A snapshot of the machine's packages.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    manager: Option<PackageManager>,
    /// Installed packages of the family's manager and their versions; None
    /// for a family whose list is not read (the probes answer instead).
    native: Option<HashMap<String, String>>,
    /// Installed packages with a newer version in the repository metadata.
    native_updates: HashMap<String, String>,
    /// Installed Flatpak applications and their versions.
    flatpaks: HashMap<String, String>,
    flatpak_updates: HashSet<String>,
    /// Where Home Manager's and NixOS's commands are, to name who manages a
    /// command from the Nix store.
    nix_places: Vec<(String, PathBuf)>,
    /// Which package owns a file, fixed for tests; None asks the manager.
    owners: Option<HashMap<PathBuf, String>>,
}

impl Inventory {
    /// Read this machine.
    pub fn gather(platform: &LinuxInfo) -> Self {
        Self::gather_for(platform.manager)
    }

    /// Read this machine, whose packages `manager` keeps.
    pub fn gather_for(manager: Option<PackageManager>) -> Self {
        let mut inventory = Inventory {
            manager,
            nix_places: nix_places(),
            ..Inventory::default()
        };
        match manager {
            Some(PackageManager::Arch) => {
                inventory.native = output(&["pacman", "-Q"]).map(|text| parse_pacman(&text));
                inventory.native_updates = output(&["pacman", "-Qu"])
                    .map(|text| parse_pacman_updates(&text))
                    .unwrap_or_default();
            }
            Some(PackageManager::Debian) => {
                inventory.native = output(&[
                    "dpkg-query",
                    "-W",
                    "-f=${db:Status-Status} ${Package} ${Version}\n",
                ])
                .map(|text| parse_dpkg(&text));
                inventory.native_updates = output(&["apt", "list", "--upgradable"])
                    .map(|text| parse_apt_upgradable(&text))
                    .unwrap_or_default();
            }
            Some(PackageManager::Fedora | PackageManager::Suse) => {
                inventory.native =
                    output(&["rpm", "-qa", "--qf", "%{NAME} %{VERSION}-%{RELEASE}\n"])
                        .map(|text| parse_pacman(&text));
                if manager == Some(PackageManager::Fedora) {
                    // -C: the metadata already downloaded, no network.
                    inventory.native_updates = output(&["dnf", "-C", "-q", "list", "--upgrades"])
                        .map(|text| parse_dnf_upgrades(&text))
                        .unwrap_or_default();
                }
            }
            _ => {}
        }
        for installation in ["--system", "--user"] {
            if let Some(text) = output(&[
                "flatpak",
                "list",
                "--app",
                installation,
                "--columns=application,version",
            ]) {
                inventory.flatpaks.extend(parse_columns(&text));
            }
            if let Some(text) = output(&[
                "flatpak",
                "remote-ls",
                "--updates",
                "--app",
                "--cached",
                installation,
                "--columns=application",
            ]) {
                inventory.flatpak_updates.extend(
                    text.lines()
                        .map(str::trim)
                        .filter(|id| !id.is_empty())
                        .map(String::from),
                );
            }
        }
        inventory
    }

    /// An inventory from known facts, for tests.
    pub fn from_parts(
        manager: Option<PackageManager>,
        native: Option<HashMap<String, String>>,
        native_updates: HashMap<String, String>,
        flatpaks: HashMap<String, String>,
        flatpak_updates: HashSet<String>,
    ) -> Self {
        Inventory {
            manager,
            native,
            native_updates,
            flatpaks,
            flatpak_updates,
            nix_places: Vec::new(),
            owners: Some(HashMap::new()),
        }
    }

    /// The same, with which package owns each file fixed, for tests.
    pub fn with_owners(mut self, owners: HashMap<PathBuf, String>) -> Self {
        self.owners = Some(owners);
        self
    }

    /// Whether a probe holds, answered from the snapshot where it can be,
    /// and by asking the machine where it cannot (families not read).
    pub fn holds(&self, probe: &Probe) -> bool {
        match probe {
            Probe::Package { manager, name } if Some(*manager) == self.manager => {
                match &self.native {
                    Some(native) => native.contains_key(name),
                    None => probe.holds(),
                }
            }
            Probe::Package { .. } => probe.holds(),
            Probe::Flatpak(id) => self.flatpaks.contains_key(id),
        }
    }

    /// The installed Flatpak applications.
    pub fn flatpak_ids(&self) -> Vec<String> {
        self.flatpaks.keys().cloned().collect()
    }

    /// A package's installed version, when the snapshot has it.
    pub fn version(&self, probe: &Probe) -> Option<String> {
        match probe {
            Probe::Package { manager, name } if Some(*manager) == self.manager => {
                self.native.as_ref()?.get(name).cloned()
            }
            Probe::Package { .. } => None,
            Probe::Flatpak(id) => self
                .flatpaks
                .get(id)
                .filter(|version| !version.is_empty())
                .cloned(),
        }
    }

    /// Plan updating entries: each package with a newer version through its
    /// own package manager or Flatpak, and on Arch one update of the whole
    /// system for all of them, since pacman does not support updating one
    /// package alone. Entries with nothing to update become notes.
    pub fn plan_update<'a>(
        &self,
        entries: impl IntoIterator<Item = &'a Entry>,
        platform: &LinuxInfo,
        on_path: &dyn Fn(&str) -> Option<PathBuf>,
    ) -> Plan {
        let mut plan = Plan::default();
        let mut system = Vec::new();
        for entry in entries {
            let Some(helix_toolbox_core::Identity {
                packages,
                aur,
                flatpak,
                ..
            }) = entry.identity()
            else {
                plan.notes.push(format!(
                    "{}: its installer decides what it installs; run it again to update",
                    entry.name
                ));
                continue;
            };
            let Some(status) = self.status(entry, platform, on_path) else {
                continue;
            };
            match &status {
                Status::Update { .. } => {}
                Status::Installed { .. } => {
                    plan.notes
                        .push(format!("{}: up to date ({})", entry.name, status.detail()));
                    continue;
                }
                Status::Managed { by, .. } => {
                    let how = match by.as_str() {
                        "Home Manager" => "update its flake inputs and switch",
                        "NixOS" => "nixos-rebuild switch --upgrade",
                        _ => "nix profile upgrade",
                    };
                    plan.notes.push(format!(
                        "{}: managed by {by}; update it there ({how})",
                        entry.name
                    ));
                    continue;
                }
                Status::Elsewhere { path } => {
                    plan.notes.push(format!(
                        "{}: installed outside a package manager ({}); update it the way it was installed",
                        entry.name,
                        path.display()
                    ));
                    continue;
                }
                Status::Absent { .. } => {
                    plan.notes.push(format!("{}: not installed", entry.name));
                    continue;
                }
            }
            let probes = crate::own_probes(packages, aur, flatpak, platform.manager);
            let owned = || {
                let binary = entry.identity()?.binary?;
                let path = on_path(binary)?;
                let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                let (manager, name) = self.owner(&path).or_else(|| self.owner(&target))?;
                Some(Probe::Package { manager, name })
            };
            let Some(probe) = probes
                .into_iter()
                .find(|probe| self.holds(probe))
                .or_else(owned)
            else {
                continue;
            };
            let target = UpdateTarget {
                name: entry.name.clone(),
                from: self.version(&probe),
                probe: probe.clone(),
            };
            let source = match &probe {
                Probe::Package {
                    manager: PackageManager::Arch,
                    ..
                } => {
                    system.push(target);
                    continue;
                }
                Probe::Package { manager, name } => Source {
                    label: manager.executable().to_string(),
                    commands: vec![crate::words(update_command(*manager), &[name])],
                    requires: vec![manager.executable().to_string(), "sudo".to_string()],
                    probe: Some(probe.clone()),
                },
                Probe::Flatpak(id) => {
                    // The installation that has it: the system's, else the user's.
                    let system_wide = Path::new("/var/lib/flatpak/app").join(id).is_dir();
                    Source {
                        label: "Flathub".to_string(),
                        commands: vec![if system_wide {
                            crate::words(vec!["sudo", "flatpak", "update", "-y"], &[id])
                        } else {
                            crate::words(vec!["flatpak", "update", "-y", "--user"], &[id])
                        }],
                        requires: if system_wide {
                            vec!["flatpak".to_string(), "sudo".to_string()]
                        } else {
                            vec!["flatpak".to_string()]
                        },
                        probe: Some(probe.clone()),
                    }
                }
            };
            plan.privileged |= source.requires.iter().any(|tool| tool == "sudo");
            plan.risk = plan.risk.max(Risk::Medium);
            plan.requirements.extend(source.requires.iter().cloned());
            plan.steps.push(Step::Update {
                id: entry.id.clone(),
                name: format!("Update {}", entry.name),
                source,
                targets: vec![target],
            });
        }
        if !system.is_empty() {
            let names: Vec<&str> = system.iter().map(|target| target.name.as_str()).collect();
            plan.notes.push(format!(
                "On Arch, updating {} alone would be a partial upgrade, which Arch does not support; this updates the whole system",
                names.join(", ")
            ));
            plan.privileged = true;
            plan.risk = Risk::High;
            plan.requirements
                .extend(["pacman".to_string(), "sudo".to_string()]);
            plan.steps.push(Step::Update {
                id: "system-update".to_string(),
                name: format!("Update the system (for {})", names.join(", ")),
                source: Source {
                    label: "pacman".to_string(),
                    commands: vec![crate::words(vec!["sudo", "pacman", "-Syu"], &[])],
                    requires: vec!["pacman".to_string(), "sudo".to_string()],
                    probe: None,
                },
                targets: system,
            });
        }
        plan.requirements.sort();
        plan.requirements.dedup();
        plan
    }

    /// How a package entry stands; None for other entries.
    pub fn status(
        &self,
        entry: &Entry,
        platform: &LinuxInfo,
        on_path: &dyn Fn(&str) -> Option<PathBuf>,
    ) -> Option<Status> {
        let helix_toolbox_core::Identity {
            binary,
            packages,
            aur,
            flatpak,
            ..
        } = entry.identity()?;
        let holds = |probe: &Probe| self.holds(probe);
        let found = |binary: &str| on_path(binary).is_some();
        let machine = crate::Machine {
            holds: &holds,
            on_path: &found,
        };
        match crate::package_state(entry, platform, machine)? {
            PackageState::Absent { replaces } => Some(Status::Absent { replaces }),
            PackageState::OnPath(_) => {
                let path = binary.and_then(on_path)?;
                Some(self.placed(binary.unwrap_or_default(), path))
            }
            PackageState::Installed(_) => {
                // A Nix install of the same command that the PATH finds first.
                let shadowed =
                    binary.and_then(|binary| match self.placed(binary, on_path(binary)?) {
                        Status::Managed { by, version } => Some(match version {
                            Some(version) => format!("{by}'s {version}"),
                            None => format!("{by}'s"),
                        }),
                        _ => None,
                    });
                // Which of its sources: the family's package, the AUR's, the Flatpak.
                let manager = platform.manager;
                let native = manager.and_then(|manager| {
                    packages
                        .for_manager(manager)
                        .map(|name| (name, source_name(manager).to_string()))
                });
                let from_aur = (manager == Some(PackageManager::Arch))
                    .then_some(aur)
                    .flatten()
                    .map(|name| (name, "AUR".to_string()));
                for (name, source) in native.into_iter().chain(from_aur) {
                    let Some(manager) = manager else { break };
                    let probe = Probe::Package {
                        manager,
                        name: name.to_string(),
                    };
                    if self.holds(&probe) {
                        let version = self
                            .native
                            .as_ref()
                            .and_then(|native| native.get(name))
                            .cloned();
                        return Some(match self.native_updates.get(name) {
                            Some(latest) => Status::Update {
                                source,
                                version,
                                latest: Some(latest.clone()),
                                shadowed,
                            },
                            None => Status::Installed {
                                source,
                                version,
                                shadowed,
                            },
                        });
                    }
                }
                let id = flatpak?;
                let version = self
                    .flatpaks
                    .get(id)
                    .filter(|version| !version.is_empty())
                    .cloned();
                let source = "Flatpak".to_string();
                Some(if self.flatpak_updates.contains(id) {
                    Status::Update {
                        source,
                        version,
                        latest: None,
                        shadowed: None,
                    }
                } else {
                    Status::Installed {
                        source,
                        version,
                        shadowed: None,
                    }
                })
            }
        }
    }

    /// Who provides a command found on the PATH but in no package.
    fn placed(&self, binary: &str, path: PathBuf) -> Status {
        let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let resolve = |path: &Path| std::fs::canonicalize(path).ok();
        if let Some(by) = nix_owner(binary, &target, &self.nix_places, &resolve) {
            return Status::Managed {
                by,
                version: store_version(&target),
            };
        }
        // A package other than the entry's own may have installed it.
        let Some((manager, name)) = self.owner(&path).or_else(|| self.owner(&target)) else {
            return Status::Elsewhere { path };
        };
        let source = format!("{}: {name}", source_name(manager));
        let version = self
            .native
            .as_ref()
            .and_then(|native| native.get(&name))
            .cloned();
        match self.native_updates.get(&name) {
            Some(latest) => Status::Update {
                source,
                version,
                latest: Some(latest.clone()),
                shadowed: None,
            },
            None => Status::Installed {
                source,
                version,
                shadowed: None,
            },
        }
    }

    /// The package of this machine's manager that owns a file.
    pub fn owner(&self, path: &Path) -> Option<(PackageManager, String)> {
        let manager = self.manager?;
        if let Some(owners) = &self.owners {
            return owners.get(path).map(|name| (manager, name.clone()));
        }
        let path = path.to_str()?;
        let name = match manager {
            PackageManager::Arch => output(&["pacman", "-Qoq", path])?,
            PackageManager::Debian => output(&["dpkg", "-S", path])?
                .split(':')
                .next()?
                .to_string(),
            PackageManager::Fedora | PackageManager::Suse => {
                output(&["rpm", "-qf", "--qf", "%{NAME}", path])?
            }
            _ => return None,
        };
        let name = name.lines().next()?.trim().to_string();
        // Errors go to standard error; a valid name is all that counts.
        helix_toolbox_core::valid_package_name(&name).then_some((manager, name))
    }
}

/// The command that updates named packages of a family, without the names;
/// Arch updates the whole system instead.
fn update_command(manager: PackageManager) -> Vec<&'static str> {
    match manager {
        PackageManager::Arch => vec!["sudo", "pacman", "-Syu"],
        PackageManager::Debian => vec!["sudo", "apt-get", "install", "--only-upgrade"],
        PackageManager::Fedora => vec!["sudo", "dnf", "upgrade"],
        PackageManager::Suse => vec!["sudo", "zypper", "update"],
        PackageManager::Alpine => vec!["sudo", "apk", "upgrade"],
        PackageManager::Void => vec!["sudo", "xbps-install", "-u"],
        PackageManager::Solus => vec!["sudo", "eopkg", "upgrade"],
    }
}

/// The name people know a family's packages by.
fn source_name(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Debian => "apt",
        manager => manager.executable(),
    }
}

/// Run a read-only query; its output when it ran. pacman -Qu exits 1 when
/// nothing is outdated, so the status is not checked, only that it ran.
fn output(command: &[&str]) -> Option<String> {
    let output = Command::new(command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .env("LC_ALL", "C")
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// `name version` lines (pacman -Q, and rpm with that format).
pub fn parse_pacman(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| line.split_once(' '))
        .map(|(name, version)| (name.to_string(), version.trim().to_string()))
        .collect()
}

/// `name old -> new` lines from pacman -Qu, skipping ignored packages.
pub fn parse_pacman_updates(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| !line.contains("[ignored]"))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?;
            let _old = words.next()?;
            (words.next()? == "->").then_some(())?;
            Some((name.to_string(), words.next()?.to_string()))
        })
        .collect()
}

/// `status package version` lines from dpkg-query; only installed ones.
pub fn parse_dpkg(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            (words.next()? == "installed").then_some(())?;
            Some((words.next()?.to_string(), words.next()?.to_string()))
        })
        .collect()
}

/// `name/suite version arch [upgradable from: old]` lines from apt list.
pub fn parse_apt_upgradable(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter(|line| line.contains("[upgradable from:"))
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let name = words.next()?.split('/').next()?;
            Some((name.to_string(), words.next()?.to_string()))
        })
        .collect()
}

/// `name.arch version repository` lines from dnf list --upgrades.
pub fn parse_dnf_upgrades(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let package = words.next()?;
            let version = words.next()?;
            let (name, arch) = package.rsplit_once('.')?;
            // Headings such as "Available upgrades" have no architecture.
            (!arch.is_empty() && version.chars().next()?.is_ascii_digit())
                .then(|| (name.to_string(), version.to_string()))
        })
        .collect()
}

/// Tab-separated `id version` columns from flatpak list.
pub fn parse_columns(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let mut columns = line.split('\t');
            let id = columns.next()?.trim();
            (!id.is_empty()).then(|| {
                (
                    id.to_string(),
                    columns.next().unwrap_or("").trim().to_string(),
                )
            })
        })
        .collect()
}

/// The version in a Nix store path: `/nix/store/<hash>-ripgrep-15.2.0/bin/rg`
/// gives 15.2.0.
pub fn store_version(path: &Path) -> Option<String> {
    let rest = path.to_str()?.strip_prefix("/nix/store/")?;
    let name = rest.split('/').next()?;
    let (_hash, name) = name.split_once('-')?;
    let mut parts = name.split('-');
    let start = parts.position(|part| part.starts_with(|c: char| c.is_ascii_digit()))?;
    let version: Vec<&str> = name.split('-').skip(start).collect();
    Some(version.join("-"))
}

/// Home Manager's and NixOS's command directories on this machine.
fn nix_places() -> Vec<(String, PathBuf)> {
    let mut places = Vec::new();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| home.as_ref().map(|home| home.join(".local/state")));
    if let Some(state) = state {
        places.push((
            "Home Manager".to_string(),
            state.join("home-manager/gcroots/current-home/home-path/bin"),
        ));
    }
    if let Ok(user) = std::env::var("USER") {
        places.push((
            "Home Manager".to_string(),
            PathBuf::from(format!("/etc/profiles/per-user/{user}/bin")),
        ));
    }
    places.push((
        "NixOS".to_string(),
        PathBuf::from("/run/current-system/sw/bin"),
    ));
    places
}

/// Who manages a command that resolves into the Nix store: whichever of
/// `places` has the same command resolving to the same file, else Nix (a
/// profile or nix-env). None outside the store. `resolve` follows links.
pub fn nix_owner(
    binary: &str,
    target: &Path,
    places: &[(String, PathBuf)],
    resolve: &dyn Fn(&Path) -> Option<PathBuf>,
) -> Option<String> {
    if !target.starts_with("/nix/store") {
        return None;
    }
    for (name, place) in places {
        if resolve(&place.join(binary)).is_some_and(|other| other == target) {
            return Some(name.clone());
        }
    }
    Some("Nix".to_string())
}

/// Where a command is on the PATH.
pub fn which(binary: &str) -> Option<PathBuf> {
    if binary.is_empty() || binary.contains('/') {
        return None;
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths)
        .map(|dir| dir.join(binary))
        .find(|path| helix_toolbox_platform::binary_exists_at(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use helix_toolbox_core::{Action, PackageNames};

    fn platform(manager: PackageManager) -> LinuxInfo {
        LinuxInfo {
            id: "test".into(),
            pretty_name: "Test Linux".into(),
            id_like: vec![],
            manager: Some(manager),
        }
    }

    fn package(
        id: &str,
        binary: Option<&str>,
        arch: Option<&str>,
        aur: Option<&str>,
        flatpak: Option<&str>,
        conflicts: &[&str],
    ) -> Entry {
        Entry {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            flags: "I".into(),
            multi_select: true,
            writes: Vec::new(),
            action: Action::Package {
                binary: binary.map(Into::into),
                packages: PackageNames {
                    arch: arch.map(Into::into),
                    ..Default::default()
                },
                aur: aur.map(Into::into),
                flatpak: flatpak.map(Into::into),
                conflicts: conflicts.iter().map(|name| name.to_string()).collect(),
                removable: true,
            },
            detect: None,
            needs: Default::default(),
            effects: Default::default(),
        }
    }

    fn map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(name, version)| (name.to_string(), version.to_string()))
            .collect()
    }

    #[test]
    fn package_lists_are_read() {
        assert_eq!(
            parse_pacman("ripgrep 15.2.0-1\nwine 11.19-1\n"),
            map(&[("ripgrep", "15.2.0-1"), ("wine", "11.19-1")])
        );
        assert_eq!(
            parse_pacman_updates("ripgrep 15.2.0-1 -> 15.3.0-1\nlinux 6.1-1 -> 6.2-1 [ignored]\n"),
            map(&[("ripgrep", "15.3.0-1")])
        );
        assert_eq!(
            parse_dpkg("installed ripgrep 14.1.0-1\nconfig-files old 1.0\nnot-installed gone 2\n"),
            map(&[("ripgrep", "14.1.0-1")])
        );
        assert_eq!(
            parse_apt_upgradable(
                "Listing...\nripgrep/noble-updates 14.1.1-1 amd64 [upgradable from: 14.1.0-1]\n"
            ),
            map(&[("ripgrep", "14.1.1-1")])
        );
        assert_eq!(
            parse_dnf_upgrades("Available upgrades\nripgrep.x86_64 14.1.1-2.fc43 updates\n"),
            map(&[("ripgrep", "14.1.1-2.fc43")])
        );
        assert_eq!(
            parse_columns("com.rtosta.zapzap\t7.5\norg.example.NoVersion\t\n"),
            map(&[("com.rtosta.zapzap", "7.5"), ("org.example.NoVersion", "")])
        );
    }

    #[test]
    fn nix_store_paths_give_versions_and_owners() {
        let rg = Path::new("/nix/store/vli6118fdbngwy46j1w741ansmrzfszr-ripgrep-15.2.0/bin/rg");
        assert_eq!(store_version(rg).as_deref(), Some("15.2.0"));
        assert_eq!(
            store_version(Path::new(
                "/nix/store/abc-python3.12-black-24.2.0/bin/black"
            ))
            .as_deref(),
            Some("24.2.0")
        );
        assert_eq!(store_version(Path::new("/usr/bin/rg")), None);
        let places = vec![
            ("Home Manager".to_string(), PathBuf::from("/hm/bin")),
            (
                "NixOS".to_string(),
                PathBuf::from("/run/current-system/sw/bin"),
            ),
        ];
        let from_home_manager =
            |path: &Path| (path == Path::new("/hm/bin/rg")).then(|| rg.to_path_buf());
        assert_eq!(
            nix_owner("rg", rg, &places, &from_home_manager).as_deref(),
            Some("Home Manager")
        );
        assert_eq!(
            nix_owner("rg", rg, &places, &|_| None).as_deref(),
            Some("Nix")
        );
        assert_eq!(
            nix_owner("rg", Path::new("/usr/bin/rg"), &places, &|_| None),
            None
        );
    }

    #[test]
    fn statuses_say_installed_update_managed_and_not_installed() {
        let arch = platform(PackageManager::Arch);
        let inventory = Inventory::from_parts(
            Some(PackageManager::Arch),
            Some(map(&[
                ("ripgrep", "15.2.0-1"),
                ("shelly", "3.1.6-1"),
                ("wine", "11.19-1"),
                ("meld", "3.24.1-1"),
            ])),
            map(&[("ripgrep", "15.3.0-1")]),
            map(&[("com.rtosta.zapzap", "7.5"), ("org.example.Old", "1.0")]),
            ["org.example.Old".to_string()].into_iter().collect(),
        );
        let nowhere = |_: &str| None;
        let status = |entry: &Entry| inventory.status(entry, &arch, &nowhere).unwrap();
        assert_eq!(
            status(&package(
                "meld",
                Some("meld"),
                Some("meld"),
                None,
                None,
                &[]
            )),
            Status::Installed {
                source: "pacman".into(),
                version: Some("3.24.1-1".into()),
                shadowed: None
            }
        );
        let ripgrep = status(&package(
            "ripgrep",
            Some("rg"),
            Some("ripgrep"),
            None,
            None,
            &[],
        ));
        assert_eq!(ripgrep.label(), "Update");
        assert_eq!(ripgrep.detail(), "pacman 15.2.0-1 → 15.3.0-1");
        assert_eq!(
            status(&package("shelly", None, None, Some("shelly"), None, &[])).detail(),
            "AUR 3.1.6-1"
        );
        let zapzap = status(&package(
            "zapzap",
            Some("zapzap"),
            None,
            Some("zapzap"),
            Some("com.rtosta.zapzap"),
            &[],
        ));
        assert_eq!(
            (zapzap.label(), zapzap.detail()),
            ("Installed", "Flatpak 7.5".to_string())
        );
        let old = status(&package(
            "old",
            None,
            None,
            None,
            Some("org.example.Old"),
            &[],
        ));
        assert_eq!(
            (old.label(), old.detail()),
            ("Update", "Flatpak 1.0".to_string())
        );
        let staging = status(&package(
            "wine-staging",
            None,
            Some("wine-staging"),
            None,
            None,
            &["wine"],
        ));
        assert_eq!(
            (staging.label(), staging.detail()),
            ("Not installed", "would replace wine".to_string())
        );
        assert_eq!(
            status(&package(
                "blender",
                Some("blender"),
                Some("blender"),
                None,
                None,
                &[]
            )),
            Status::Absent { replaces: vec![] }
        );
        // A command from the Nix store, with no package: managed by Nix.
        let from_nix = |binary: &str| {
            (binary == "btop").then(|| PathBuf::from("/nix/store/bvxz-btop-1.4.7/bin/btop"))
        };
        let btop = inventory
            .status(
                &package("btop", Some("btop"), Some("btop"), None, None, &[]),
                &arch,
                &from_nix,
            )
            .unwrap();
        assert_eq!(
            (btop.label(), btop.detail()),
            ("Managed", "Nix 1.4.7".to_string())
        );
        // The package installed, and a Nix copy of its command first on the PATH.
        let shadowing = |binary: &str| {
            (binary == "rg").then(|| PathBuf::from("/nix/store/vli6-ripgrep-15.2.0/bin/rg"))
        };
        let both = inventory
            .status(
                &package("ripgrep", Some("rg"), Some("ripgrep"), None, None, &[]),
                &arch,
                &shadowing,
            )
            .unwrap();
        assert_eq!(
            both.detail(),
            "pacman 15.2.0-1 → 15.3.0-1; Nix's 15.2.0 runs first"
        );
        // From somewhere Helix Toolbox cannot place.
        let local = |binary: &str| {
            (binary == "tool").then(|| PathBuf::from("/opt/helix-toolbox-test/tool"))
        };
        let tool = inventory
            .status(
                &package("tool", Some("tool"), Some("tool"), None, None, &[]),
                &arch,
                &local,
            )
            .unwrap();
        assert_eq!(
            (tool.label(), tool.detail()),
            ("Installed", "/opt/helix-toolbox-test/tool".to_string())
        );
        let script = Entry {
            action: Action::Command {
                line: "true".into(),
            },
            ..package("s", None, None, None, None, &[])
        };
        assert_eq!(inventory.status(&script, &arch, &nowhere), None);
    }

    #[test]
    fn updates_are_planned_per_family_and_arch_updates_the_system() {
        let arch = platform(PackageManager::Arch);
        let inventory = Inventory::from_parts(
            Some(PackageManager::Arch),
            Some(map(&[
                ("ripgrep", "15.2.0-1"),
                ("meld", "3.24.1-1"),
                ("git", "2.56.0-1"),
            ])),
            map(&[("ripgrep", "15.3.0-1"), ("meld", "3.24.2-1")]),
            map(&[("org.example.Old", "1.0")]),
            ["org.example.Old".to_string()].into_iter().collect(),
        );
        let ripgrep = package("ripgrep", Some("rg"), Some("ripgrep"), None, None, &[]);
        let meld = package("meld", Some("meld"), Some("meld"), None, None, &[]);
        let git = package("git", Some("git"), Some("git"), None, None, &[]);
        let old = package("old", None, None, None, Some("org.example.Old"), &[]);
        let gone = package("blender", None, Some("blender"), None, None, &[]);
        let script = Entry {
            action: Action::Command {
                line: "true".into(),
            },
            ..package("steam", None, None, None, None, &[])
        };
        let nowhere = |_: &str| None;
        let planned = inventory.plan_update(
            [&ripgrep, &meld, &git, &old, &gone, &script],
            &arch,
            &nowhere,
        );
        assert_eq!(planned.steps.len(), 2);
        // The Flatpak on its own, for the installation that has it.
        assert_eq!(
            planned.steps[0].display(),
            "flatpak update -y --user org.example.Old"
        );
        // Both Arch packages through one whole-system update.
        let Step::Update { targets, .. } = &planned.steps[1] else {
            panic!("expected an update step");
        };
        assert_eq!(planned.steps[1].display(), "sudo pacman -Syu");
        assert_eq!(
            planned.steps[1].name(),
            "Update the system (for ripgrep, meld)"
        );
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].from.as_deref(), Some("15.2.0-1"));
        assert_eq!(planned.risk, Risk::High);
        assert!(planned.privileged);
        assert_eq!(planned.requirements, ["flatpak", "pacman", "sudo"]);
        assert!(
            planned
                .notes
                .iter()
                .any(|note| note.starts_with("git: up to date (pacman 2.56.0-1)"))
        );
        assert!(
            planned
                .notes
                .iter()
                .any(|note| note == "blender: not installed")
        );
        assert!(
            planned
                .notes
                .iter()
                .any(|note| note.contains("partial upgrade"))
        );
        assert!(
            planned
                .notes
                .iter()
                .any(|note| note.starts_with("steam: its installer decides"))
        );

        let debian = platform(PackageManager::Debian);
        let apt = Inventory::from_parts(
            Some(PackageManager::Debian),
            Some(map(&[("ripgrep", "14.1.0-1")])),
            map(&[("ripgrep", "14.1.1-1")]),
            HashMap::new(),
            HashSet::new(),
        );
        let mut deb = ripgrep.clone();
        if let Action::Package { packages, .. } = &mut deb.action {
            packages.debian = Some("ripgrep".into());
        }
        let planned = apt.plan_update([&deb], &debian, &nowhere);
        assert_eq!(
            planned.steps[0].display(),
            "sudo apt-get install --only-upgrade ripgrep"
        );
        assert_eq!(planned.risk, Risk::Medium);
        // Managed by Nix: nothing to run, and where to update it.
        let from_nix = |binary: &str| {
            (binary == "btop").then(|| PathBuf::from("/nix/store/x-btop-1.4.7/bin/btop"))
        };
        let btop = package("btop", Some("btop"), Some("btop"), None, None, &[]);
        let planned = inventory.plan_update([&btop], &arch, &from_nix);
        assert!(planned.steps.is_empty());
        assert_eq!(
            planned.notes,
            ["btop: managed by Nix; update it there (nix profile upgrade)"]
        );
    }

    #[test]
    fn updates_are_verified_by_their_versions() {
        let probe = Probe::Package {
            manager: PackageManager::Arch,
            name: "ripgrep".into(),
        };
        let plan = Plan {
            steps: vec![Step::Update {
                id: "system-update".into(),
                name: "Update the system".into(),
                source: Source {
                    label: "pacman".into(),
                    commands: vec![vec!["sudo".into(), "pacman".into(), "-Syu".into()]],
                    requires: Vec::new(),
                    probe: None,
                },
                targets: vec![UpdateTarget {
                    name: "Ripgrep".into(),
                    probe: probe.clone(),
                    from: Some("15.2.0-1".into()),
                }],
            }],
            ..Plan::default()
        };
        let check = |now: Option<&str>, holds: bool| {
            let now = now.map(String::from);
            crate::verify_updates_with(&plan, &|_| now.clone(), &|_| holds).remove(0)
        };
        let updated = check(Some("15.3.0-1"), true);
        assert!(updated.passed);
        assert_eq!(updated.detail, "updated 15.2.0-1 → 15.3.0-1");
        assert_eq!(updated.name, "Update Ripgrep");
        let same = check(Some("15.2.0-1"), true);
        assert!(!same.passed);
        assert_eq!(
            same.detail,
            "still 15.2.0-1: no newer version was installed"
        );
        assert!(
            check(None, true).passed,
            "a family without versions: installed is enough"
        );
        assert!(!check(None, false).passed);
        // The run itself also stops if the package is gone afterward.
        let program = crate::shell_program(&plan, Path::new("/lib"), Path::new("/hf"));
        assert!(program.contains("'sudo' 'pacman' '-Syu'"), "{program}");
        assert!(program.contains("'pacman' '-Q' 'ripgrep'"));
        assert!(program.contains("is not installed after the update"));
    }

    #[test]
    fn a_command_from_another_package_is_shown_and_updated_through_it() {
        let arch = platform(PackageManager::Arch);
        let inventory = Inventory::from_parts(
            Some(PackageManager::Arch),
            Some(map(&[("github-desktop", "3.4.14-2")])),
            map(&[("github-desktop", "3.4.15-1")]),
            HashMap::new(),
            HashSet::new(),
        )
        .with_owners(
            [(
                PathBuf::from("/usr/bin/github-desktop"),
                "github-desktop".to_string(),
            )]
            .into(),
        );
        let mut entry = package(
            "github-desktop",
            Some("github-desktop"),
            None,
            Some("github-desktop-bin"),
            None,
            &[],
        );
        // A script entry that says what it installs, as the catalog's do.
        let identity = entry.identity().map(|identity| helix_toolbox_core::Detect {
            binary: identity.binary.map(String::from),
            packages: identity.packages.clone(),
            aur: identity.aur.map(String::from),
            flatpak: None,
        });
        entry.action = Action::Script {
            file: PathBuf::from("githubdesktop.sh"),
            interactive: true,
        };
        entry.detect = identity;
        let found = |binary: &str| {
            (binary == "github-desktop").then(|| PathBuf::from("/usr/bin/github-desktop"))
        };
        let status = inventory.status(&entry, &arch, &found).unwrap();
        assert_eq!(status.label(), "Update");
        assert_eq!(
            status.detail(),
            "pacman: github-desktop 3.4.14-2 → 3.4.15-1"
        );
        let planned = inventory.plan_update([&entry], &arch, &found);
        let Step::Update { targets, .. } = &planned.steps[0] else {
            panic!("expected an update step");
        };
        assert_eq!(
            targets[0].probe,
            Probe::Package {
                manager: PackageManager::Arch,
                name: "github-desktop".into()
            }
        );
        assert_eq!(targets[0].from.as_deref(), Some("3.4.14-2"));
        // Not owned by any package: installed elsewhere, nothing to update.
        let unowned = inventory.clone().with_owners(HashMap::new());
        let planned = unowned.plan_update([&entry], &arch, &found);
        assert!(planned.steps.is_empty());
        assert!(planned.notes[0].contains("installed outside a package manager"));
    }
}
