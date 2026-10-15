//! Turns entries into explicit plans, runs them, and records what happened.
//!
//! Every interface uses this crate the same way: `plan` an entry (or several)
//! into a [`Plan`] of ordered [`Step`]s with what they need, whether they are
//! privileged and how risky they are; show it; then run it through a
//! [`Runner`]. The steps become one POSIX shell program that stops at the
//! first failure. The shell only ever sees quoted literals from the validated
//! catalog, never free text. Native operations run back inside HelixForge
//! itself, so even they go through the same program and the same terminal.
//!
//! A package step lists its sources in order (the distribution's package,
//! the AUR on Arch, Flathub); the program tries the next one only when the
//! one before it fails or leaves the package missing. After a run, [`verify`]
//! checks that each package step left its package installed, or removed.

pub mod history;
pub mod inventory;
pub mod preflight;
pub mod pty;

use helixforge_core::{
    Action, Entry, NativeAction, NixAction, PackageManager, Risk, valid_package_name,
};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
};
use thiserror::Error;
use tracing::{info, warn};

/// The variable scripts use to find the shared helpers.
pub const LIB_VARIABLE: &str = "HELIXFORGE_LIB";

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("this distribution is not supported for package installation")]
    UnsupportedPlatform,
    #[error("{0} has no package for this distribution family")]
    UnsupportedEntry(String),
    #[error("unsafe catalog value: {0}")]
    Invalid(String),
    #[error("{0} is not a package entry that can be removed")]
    NotRemovable(String),
    #[error("profile {0} names an entry that is not in the catalog: {1}")]
    UnknownMember(String, String),
    #[error("required executable not found: {0}")]
    MissingExecutable(String),
    #[error("cannot start the shell: {0}")]
    Spawn(#[source] std::io::Error),
}

/// One thing to run, with everything needed to run it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Step {
    /// Installs a package from the first of its sources that works, or
    /// removes it through its one source.
    Package {
        id: String,
        name: String,
        /// A command the package provides, asked only where no probe can.
        binary: Option<String>,
        sources: Vec<Source>,
        removal: bool,
    },
    Script {
        id: String,
        name: String,
        file: PathBuf,
        program: String,
        args: Vec<String>,
        interactive: bool,
    },
    Command {
        id: String,
        name: String,
        line: String,
    },
    Nix {
        id: String,
        name: String,
        action: NixAction,
    },
    /// Runs inside HelixForge itself, re-invoked from the shell program.
    Native {
        id: String,
        name: String,
        action: NativeAction,
    },
    /// Brings installed packages up to date through one source; on Arch, the
    /// whole system for every target at once.
    Update {
        id: String,
        name: String,
        source: Source,
        targets: Vec<UpdateTarget>,
    },
}

/// A package an update step brings up to date.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpdateTarget {
    /// The entry's name, for the checks.
    pub name: String,
    /// How to find the package, and its version, afterwards.
    pub probe: Probe,
    /// Its version before the update, when known.
    pub from: Option<String>,
}

/// One way to install or remove a package.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Source {
    /// How the run names it: "pacman", "the AUR (paru)", "Flathub".
    pub label: String,
    /// Commands run in order; each must succeed.
    pub commands: Vec<Vec<String>>,
    /// Executables the commands need.
    pub requires: Vec<String>,
    /// What shows the package is installed from this source, when there is a
    /// way to ask.
    pub probe: Option<Probe>,
}

impl Source {
    pub fn command_line(&self) -> String {
        self.commands
            .iter()
            .map(|command| command.join(" "))
            .collect::<Vec<_>>()
            .join(" && ")
    }
}

/// A question about the system whose answer says a package is installed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Probe {
    /// The package manager lists the package as installed.
    Package {
        manager: PackageManager,
        name: String,
    },
    /// The Flatpak application is installed, for the system or the user.
    Flatpak(String),
}

impl Probe {
    /// The probe for a package of this family; Solus has no simple query.
    fn package(manager: PackageManager, name: &str) -> Option<Self> {
        (manager != PackageManager::Solus).then(|| Probe::Package {
            manager,
            name: name.to_string(),
        })
    }

    /// The query, for the families that answer through the exit status.
    fn query(manager: PackageManager, name: &str) -> Vec<String> {
        let base: &[&str] = match manager {
            PackageManager::Arch => &["pacman", "-Q"],
            PackageManager::Fedora | PackageManager::Suse => &["rpm", "-q"],
            PackageManager::Alpine => &["apk", "info", "-e"],
            PackageManager::Void => &["xbps-query"],
            PackageManager::Debian | PackageManager::Solus => &[],
        };
        words(base.to_vec(), &[name])
    }

    /// A shell condition that is true when the probe holds.
    pub fn shell(&self) -> String {
        match self {
            Probe::Package {
                manager: PackageManager::Debian,
                name,
            } => format!(
                "[ \"$(dpkg-query -W -f='${{db:Status-Status}}' {} 2>/dev/null)\" = installed ]",
                quote(name)
            ),
            Probe::Package { manager, name } => {
                format!("{} >/dev/null 2>&1", quoted(&Probe::query(*manager, name)))
            }
            Probe::Flatpak(id) => format!(
                "{{ [ -d {} ] || [ -d \"$HOME\"/{} ]; }}",
                quote(&format!("/var/lib/flatpak/app/{id}")),
                quote(&format!(".local/share/flatpak/app/{id}"))
            ),
        }
    }

    /// Whether the probe holds now.
    pub fn holds(&self) -> bool {
        match self {
            Probe::Package {
                manager: PackageManager::Debian,
                name,
            } => Command::new("dpkg-query")
                .args(["-W", "-f=${db:Status-Status}", name])
                .stderr(Stdio::null())
                .output()
                .is_ok_and(|output| output.stdout == b"installed"),
            Probe::Package { manager, name } => {
                let query = Probe::query(*manager, name);
                Command::new(&query[0])
                    .args(&query[1..])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success())
            }
            Probe::Flatpak(id) => flatpak_installed_in(&flatpak_roots(), id),
        }
    }

    /// What answers the probe, for a sentence saying it did not.
    fn subject(&self) -> String {
        match self {
            Probe::Package {
                manager: PackageManager::Debian,
                name,
            } => format!("dpkg ({name})"),
            Probe::Package { manager, name } => format!("{} ({name})", manager.executable()),
            Probe::Flatpak(id) => format!("Flatpak ({id})"),
        }
    }

    pub fn describe(&self) -> String {
        match self {
            Probe::Package {
                manager: PackageManager::Debian,
                name,
            } => format!("dpkg lists {name}"),
            Probe::Package { manager, name } => {
                format!("{} lists {name}", manager.executable())
            }
            Probe::Flatpak(id) => format!("{id} is installed from Flathub"),
        }
    }
}

impl Step {
    pub fn id(&self) -> &str {
        match self {
            Step::Package { id, .. }
            | Step::Script { id, .. }
            | Step::Command { id, .. }
            | Step::Nix { id, .. }
            | Step::Native { id, .. }
            | Step::Update { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Step::Package { name, .. }
            | Step::Script { name, .. }
            | Step::Command { name, .. }
            | Step::Nix { name, .. }
            | Step::Native { name, .. }
            | Step::Update { name, .. } => name,
        }
    }

    /// The command line as the shell will run it, for previews and plans.
    pub fn display(&self) -> String {
        match self {
            Step::Package { sources, .. } => {
                let mut shown = sources
                    .first()
                    .map(Source::command_line)
                    .unwrap_or_default();
                if sources.len() > 1 {
                    let others: Vec<&str> = sources[1..]
                        .iter()
                        .map(|source| source.label.as_str())
                        .collect();
                    shown.push_str(&format!("  (else {})", others.join(", then ")));
                }
                shown
            }
            Step::Script {
                file,
                program,
                args,
                ..
            } => {
                let mut words = vec![program.clone()];
                words.extend(args.iter().cloned());
                words.push(
                    file.file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                );
                format!(
                    "{}  (in {})",
                    words.join(" "),
                    file.parent().unwrap_or(Path::new(".")).display()
                )
            }
            Step::Command { line, .. } => line.clone(),
            Step::Nix { action, .. } => nix_command(action).join(" "),
            Step::Native { action, .. } => format!("helixforge native {}", action.id()),
            Step::Update { source, .. } => source.command_line(),
        }
    }

    /// The sources tried when the first fails, as lines for a full plan.
    pub fn fallbacks(&self) -> Vec<String> {
        match self {
            Step::Package { sources, .. } => sources
                .iter()
                .skip(1)
                .map(|source| format!("if that fails, {}: {}", source.label, source.command_line()))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// The executables a step needs before it can start; for a package, what
    /// its first source needs.
    fn requirements(&self) -> Vec<String> {
        match self {
            Step::Package { sources, .. } => sources
                .first()
                .map(|source| source.requires.clone())
                .unwrap_or_default(),
            Step::Nix { action, .. } => match action {
                NixAction::ProfileInstall { .. } => vec!["nix".to_string()],
                NixAction::HomeManagerSwitch { .. } => vec!["home-manager".to_string()],
            },
            Step::Script { program, .. } => vec![program.clone()],
            Step::Update { source, .. } => source.requires.clone(),
            Step::Command { .. } | Step::Native { .. } => Vec::new(),
        }
    }
}

/// What running a set of entries means on this machine: the steps in order,
/// plus what a reviewer needs to know before saying yes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Plan {
    pub steps: Vec<Step>,
    /// Entries that need nothing done, and why.
    pub notes: Vec<String>,
    /// Executables the steps need.
    pub requirements: Vec<String>,
    /// Whether any step asks for elevated privileges.
    pub privileged: bool,
    pub risk: Risk,
    /// Entries left out because a path they write is managed by another tool.
    pub blocked: Vec<String>,
}

impl Plan {
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    pub fn entries(&self) -> Vec<(String, String)> {
        self.steps
            .iter()
            .map(|step| (step.id().to_string(), step.name().to_string()))
            .collect()
    }
}

/// The questions planning asks the machine, so tests can answer them.
#[derive(Clone, Copy)]
pub struct Machine<'a> {
    /// Whether a package manager or Flatpak lists a package as installed.
    pub holds: &'a dyn Fn(&Probe) -> bool,
    /// Whether a command is on the PATH.
    pub on_path: &'a dyn Fn(&str) -> bool,
}

impl Machine<'static> {
    /// This machine.
    pub fn real() -> Self {
        Machine {
            holds: &|probe: &Probe| probe.holds(),
            on_path: &binary_exists,
        }
    }
}

/// How a package entry stands on a machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PackageState {
    /// One of its own packages is installed; how it was found.
    Installed(String),
    /// None of its packages is, but its command is on the PATH from another
    /// install (Nix, Home Manager, a download).
    OnPath(String),
    /// Not installed. `replaces` lists the installed packages it conflicts
    /// with, which installing it would replace.
    Absent { replaces: Vec<String> },
}

/// The probes for an entry's own packages on this machine: the family's
/// package, on Arch its AUR package, and its Flatpak.
pub(crate) fn own_probes(
    packages: &helixforge_core::PackageNames,
    aur: Option<&str>,
    flatpak: Option<&str>,
    manager: Option<PackageManager>,
) -> Vec<Probe> {
    let mut probes = Vec::new();
    if let Some(manager) = manager {
        probes.extend(
            packages
                .for_manager(manager)
                .and_then(|name| Probe::package(manager, name)),
        );
        if manager == PackageManager::Arch {
            probes.extend(aur.and_then(|name| Probe::package(manager, name)));
        }
    }
    probes.extend(flatpak.map(|id| Probe::Flatpak(id.to_string())));
    probes
}

/// How a package entry stands: its own packages first, then the packages it
/// conflicts with, and its command only when neither is installed. None for
/// an entry that is not a package.
pub fn package_state(
    entry: &Entry,
    platform: &LinuxInfo,
    machine: Machine,
) -> Option<PackageState> {
    let helixforge_core::Identity {
        binary,
        packages,
        aur,
        flatpak,
        conflicts,
    } = entry.identity()?;
    let probes = own_probes(packages, aur, flatpak, platform.manager);
    if let Some(found) = probes.iter().find(|probe| (machine.holds)(probe)) {
        return Some(PackageState::Installed(found.describe()));
    }
    let replaces: Vec<String> = match platform.manager {
        Some(manager) => conflicts
            .iter()
            .filter(|name| {
                valid_package_name(name)
                    && Probe::package(manager, name).is_some_and(|probe| (machine.holds)(&probe))
            })
            .cloned()
            .collect(),
        None => Vec::new(),
    };
    if replaces.is_empty()
        && let Some(binary) = binary
        && (machine.on_path)(binary)
    {
        return Some(PackageState::OnPath(format!("{binary} is on the PATH")));
    }
    Some(PackageState::Absent { replaces })
}

/// Plan one or more entries. Profiles expand into their members; a package
/// that is already installed becomes a note instead of a step.
pub fn plan<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
) -> Result<Plan, EngineError> {
    plan_with(entries, platform, lookup, Machine::real())
}

/// `plan` with the machine's answers supplied.
pub fn plan_with<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    machine: Machine,
) -> Result<Plan, EngineError> {
    let mut result = Plan::default();
    for entry in entries {
        add_entry(
            entry,
            platform,
            lookup,
            machine,
            &mut result,
            &mut Vec::new(),
        )?;
    }
    result.requirements.sort();
    result.requirements.dedup();
    info!(
        steps = result.steps.len(),
        risk = result.risk.label(),
        privileged = result.privileged,
        "planned"
    );
    Ok(result)
}

fn add_entry<'a>(
    entry: &'a Entry,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    machine: Machine,
    into: &mut Plan,
    trail: &mut Vec<String>,
) -> Result<(), EngineError> {
    let step = match &entry.action {
        Action::Package {
            binary,
            packages,
            aur,
            flatpak,
            ..
        } => {
            if let Some(PackageState::Installed(found) | PackageState::OnPath(found)) =
                package_state(entry, platform, machine)
            {
                into.notes
                    .push(format!("{}: already installed ({found})", entry.name));
                return Ok(());
            }
            let sources = install_sources(
                entry,
                platform,
                packages,
                aur.as_deref(),
                flatpak.as_deref(),
            )?;
            Step::Package {
                id: entry.id.clone(),
                name: entry.name.clone(),
                binary: binary.clone(),
                sources,
                removal: false,
            }
        }
        Action::Script { file, interactive } => {
            // What it installs may be there already; the script still runs,
            // since it may set more up.
            if let Some(PackageState::Installed(found) | PackageState::OnPath(found)) =
                package_state(entry, platform, machine)
            {
                into.notes.push(format!(
                    "{}: already installed ({found}); its script runs anyway",
                    entry.name
                ));
            }
            let (program, args) = helixforge_catalog::interpreter(file);
            Step::Script {
                id: entry.id.clone(),
                name: entry.name.clone(),
                file: file.clone(),
                program,
                args,
                interactive: *interactive,
            }
        }
        Action::Command { line } => Step::Command {
            id: entry.id.clone(),
            name: entry.name.clone(),
            line: line.clone(),
        },
        Action::Nix { action } => Step::Nix {
            id: entry.id.clone(),
            name: entry.name.clone(),
            action: action.clone(),
        },
        Action::Native { action } => Step::Native {
            id: entry.id.clone(),
            name: entry.name.clone(),
            action: *action,
        },
        Action::Profile { entries } => {
            if trail.contains(&entry.id) {
                return Err(EngineError::Invalid(format!(
                    "profile {} includes itself",
                    entry.id
                )));
            }
            trail.push(entry.id.clone());
            for member in entries {
                let found = lookup(member)
                    .ok_or_else(|| EngineError::UnknownMember(entry.id.clone(), member.clone()))?;
                add_entry(found, platform, lookup, machine, into, trail)?;
            }
            trail.pop();
            return Ok(());
        }
    };
    if let Some(path) = managed_path(&entry.writes) {
        into.blocked.push(format!(
            "{}: {} is managed by Home Manager (a symlink into the Nix store); change it there instead",
            entry.name,
            path.display()
        ));
        return Ok(());
    }
    into.privileged |= helixforge_core::privileged(&entry.flags, &entry.action);
    into.risk = into.risk.max(Risk::from_flags(&entry.flags));
    into.requirements.extend(step.requirements());
    into.steps.push(step);
    Ok(())
}

/// The first of the entry's paths that is a Home Manager symlink, or a child
/// of one, with `~` expanded to the user's home.
pub fn managed_path(writes: &[String]) -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    managed_path_in(&home, writes)
}

/// `managed_path` with an explicit home directory. Dangling links count: the
/// store path may be garbage-collected while the link stays.
pub fn managed_path_in(home: &Path, writes: &[String]) -> Option<PathBuf> {
    for written in writes {
        let path = match written.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None if written == "~" => home.to_path_buf(),
            None => PathBuf::from(written),
        };
        for candidate in path.ancestors() {
            if candidate == home {
                break;
            }
            if let Ok(target) = std::fs::read_link(candidate)
                && target.starts_with("/nix/store")
            {
                return Some(candidate.to_path_buf());
            }
        }
    }
    None
}

const FLATHUB: &str = "https://dl.flathub.org/repo/flathub.flatpakrepo";

pub(crate) fn words(base: Vec<&str>, extra: &[&str]) -> Vec<String> {
    base.into_iter()
        .chain(extra.iter().copied())
        .map(str::to_string)
        .collect()
}

/// Where Flatpak keeps installed applications: the system's and the user's.
fn flatpak_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/var/lib/flatpak")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/flatpak"));
    }
    roots
}

/// Whether a Flatpak application is installed under any of the roots.
pub fn flatpak_installed_in(roots: &[PathBuf], id: &str) -> bool {
    roots.iter().any(|root| root.join("app").join(id).is_dir())
}

/// The AUR helper to use, paru before yay.
pub fn aur_helper(exists: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    ["paru", "yay"].into_iter().find(|helper| exists(helper))
}

/// The sources that can install a package entry on this machine, in the
/// order they are tried: the family's package, on Arch the AUR, then Flathub.
fn install_sources(
    entry: &Entry,
    platform: &LinuxInfo,
    packages: &helixforge_core::PackageNames,
    aur: Option<&str>,
    flatpak: Option<&str>,
) -> Result<Vec<Source>, EngineError> {
    let check = |name: &str| {
        if valid_package_name(name) {
            Ok(())
        } else {
            Err(EngineError::Invalid(entry.id.clone()))
        }
    };
    let mut sources = Vec::new();
    if let Some(manager) = platform.manager {
        if let Some(package) = packages.for_manager(manager) {
            check(package)?;
            sources.push(Source {
                label: manager.executable().to_string(),
                commands: vec![words(manager.install_command(), &[package])],
                requires: vec![manager.executable().to_string(), "sudo".to_string()],
                probe: Probe::package(manager, package),
            });
        }
        if manager == PackageManager::Arch
            && let Some(package) = aur
        {
            check(package)?;
            // A missing helper is skipped for the next source, or reported
            // before the run starts when there is none.
            let helper = aur_helper(&|name| binary_exists(name)).unwrap_or("paru");
            sources.push(Source {
                label: format!("the AUR ({helper})"),
                commands: vec![words(vec![helper, "-S", "--needed"], &[package])],
                requires: vec![helper.to_string()],
                probe: Probe::package(manager, package),
            });
        }
    }
    if let Some(id) = flatpak {
        if !helixforge_core::valid_flatpak_id(id) {
            return Err(EngineError::Invalid(entry.id.clone()));
        }
        sources.push(Source {
            label: "Flathub".to_string(),
            commands: vec![
                words(
                    vec![
                        "sudo",
                        "flatpak",
                        "remote-add",
                        "--if-not-exists",
                        "flathub",
                    ],
                    &[FLATHUB],
                ),
                words(vec!["sudo", "flatpak", "install", "flathub"], &[id]),
            ],
            requires: vec!["flatpak".to_string(), "sudo".to_string()],
            probe: Some(Probe::Flatpak(id.to_string())),
        });
    }
    if sources.is_empty() {
        return Err(match platform.manager {
            Some(_) => EngineError::UnsupportedEntry(entry.id.clone()),
            None => EngineError::UnsupportedPlatform,
        });
    }
    Ok(sources)
}

/// Plan removing package entries. An entry is removed the way it is found
/// installed: its Flatpak, the family's package or, on Arch, its AUR package
/// with pacman. Entries that are not installed become notes, as do entries
/// found only by their command, which HelixForge did not install.
pub fn plan_removal<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
) -> Result<Plan, EngineError> {
    plan_removal_with(entries, platform, Machine::real())
}

/// `plan_removal` with the machine's answers supplied.
pub fn plan_removal_with<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
    machine: Machine,
) -> Result<Plan, EngineError> {
    let mut result = Plan::default();
    for entry in entries {
        let Action::Package {
            binary,
            packages,
            aur,
            flatpak,
            removable,
            ..
        } = &entry.action
        else {
            return Err(EngineError::NotRemovable(entry.id.clone()));
        };
        if !removable {
            return Err(EngineError::NotRemovable(entry.id.clone()));
        }
        let probes = own_probes(
            packages,
            aur.as_deref(),
            flatpak.as_deref(),
            platform.manager,
        );
        let found = probes.into_iter().find(|probe| (machine.holds)(probe));
        let on_path = binary
            .as_deref()
            .is_some_and(|binary| (machine.on_path)(binary));
        let source = match found {
            Some(Probe::Flatpak(id)) => Source {
                label: "Flathub".to_string(),
                commands: vec![words(vec!["sudo", "flatpak", "uninstall"], &[&id])],
                requires: vec!["flatpak".to_string(), "sudo".to_string()],
                probe: Some(Probe::Flatpak(id)),
            },
            Some(Probe::Package { manager, name }) => {
                if !valid_package_name(&name) {
                    return Err(EngineError::Invalid(entry.id.clone()));
                }
                Source {
                    label: manager.executable().to_string(),
                    commands: vec![words(manager.remove_command(), &[&name])],
                    requires: vec![manager.executable().to_string(), "sudo".to_string()],
                    probe: Some(Probe::Package { manager, name }),
                }
            }
            // Solus cannot be asked; there the command stands in for its package.
            None if on_path
                && platform.manager == Some(PackageManager::Solus)
                && packages.solus.is_some() =>
            {
                let name = packages.solus.as_deref().unwrap_or_default();
                if !valid_package_name(name) {
                    return Err(EngineError::Invalid(entry.id.clone()));
                }
                Source {
                    label: PackageManager::Solus.executable().to_string(),
                    commands: vec![words(PackageManager::Solus.remove_command(), &[name])],
                    requires: vec![
                        PackageManager::Solus.executable().to_string(),
                        "sudo".to_string(),
                    ],
                    probe: None,
                }
            }
            None if on_path => {
                result.notes.push(format!(
                    "{}: installed, but not by a package HelixForge knows; remove it the way it was installed",
                    entry.name
                ));
                continue;
            }
            None => {
                result
                    .notes
                    .push(format!("{}: not installed, nothing to remove", entry.name));
                continue;
            }
        };
        result.privileged = true;
        result.risk = result.risk.max(Risk::Medium);
        result.requirements.extend(source.requires.iter().cloned());
        result.steps.push(Step::Package {
            id: entry.id.clone(),
            name: format!("Remove {}", entry.name),
            binary: binary.clone(),
            sources: vec![source],
            removal: true,
        });
    }
    result.requirements.sort();
    result.requirements.dedup();
    info!(steps = result.steps.len(), "planned removal");
    Ok(result)
}

fn nix_command(action: &NixAction) -> Vec<String> {
    match action {
        NixAction::ProfileInstall { installable } => {
            vec![
                "nix".into(),
                "profile".into(),
                "install".into(),
                installable.clone(),
            ]
        }
        NixAction::HomeManagerSwitch { flake } => {
            vec![
                "home-manager".into(),
                "switch".into(),
                "--flake".into(),
                flake.clone(),
            ]
        }
    }
}

/// Quote a value for the POSIX shell.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The shell program that runs the steps in order and stops at the first
/// failure. `lib_dir` is exported as `HELIXFORGE_LIB` for scripts; `this` is
/// the HelixForge executable that native steps re-invoke.
pub fn shell_program(plan: &Plan, lib_dir: &Path, this: &Path) -> String {
    let mut text = format!(
        "{LIB_VARIABLE}={}\nexport {LIB_VARIABLE}\n",
        quote(&lib_dir.to_string_lossy())
    );
    for step in &plan.steps {
        let body = match step {
            Step::Package {
                name,
                binary,
                sources,
                removal,
                ..
            } => package_program(name, binary.as_deref(), sources, *removal),
            Step::Script {
                file,
                program,
                args,
                ..
            } => {
                let directory = file.parent().unwrap_or(Path::new("."));
                let mut words = vec![program.clone()];
                words.extend(args.iter().cloned());
                words.push(file.to_string_lossy().into_owned());
                format!(
                    "cd {} || exit 1\n{}",
                    quote(&directory.to_string_lossy()),
                    quoted(&words)
                )
            }
            Step::Command { line, .. } => format!("sh -e -c {}", quote(line)),
            Step::Nix { action, .. } => quoted(&nix_command(action)),
            Step::Native { action, .. } => quoted(&[
                this.to_string_lossy().into_owned(),
                "native".to_string(),
                action.id().to_string(),
            ]),
            Step::Update {
                source, targets, ..
            } => {
                let mut body = source
                    .commands
                    .iter()
                    .map(|command| quoted(command))
                    .collect::<Vec<_>>()
                    .join(" &&\n");
                body.push_str("\nhf_rc=$?\n[ \"$hf_rc\" -eq 0 ] || exit \"$hf_rc\"\n");
                for target in targets {
                    body.push_str(&format!(
                        "if ! {}; then\n  printf 'HelixForge: %s is not installed after the update\\n' {}\n  exit 1\nfi\n",
                        target.probe.shell(),
                        quote(&target.name)
                    ));
                }
                body
            }
        };
        text.push_str(&format!(
            "printf '\\n==> %s\\n' {}\n(\n{body}\n)\nrc=$?\n[ \"$rc\" -eq 0 ] || exit \"$rc\"\n",
            quote(step.name())
        ));
    }
    text
}

/// The body of a package step. An install tries each source in turn until
/// one succeeds and leaves the package installed; an interrupted command
/// stops the run instead. A removal fails if the package is still there.
fn package_program(name: &str, binary: Option<&str>, sources: &[Source], removal: bool) -> String {
    let commands = |source: &Source, indent: &str| {
        source
            .commands
            .iter()
            .map(|command| format!("{indent}{}", quoted(command)))
            .collect::<Vec<_>>()
            .join(" &&\n")
    };
    // The package manager's answer; the command only where it cannot give one.
    let on_path = binary.map(|binary| format!("command -v {} >/dev/null 2>&1", quote(binary)));
    if removal {
        let Some(source) = sources.first() else {
            return "exit 1".to_string();
        };
        let present = source
            .probe
            .as_ref()
            .map(Probe::shell)
            .or(on_path)
            .unwrap_or_else(|| "false".to_string());
        return format!(
            "{}\nhf_rc=$?\n[ \"$hf_rc\" -eq 0 ] || exit \"$hf_rc\"\n\
             if {present}; then\n  printf 'HelixForge: %s is still installed\\n' {}\n  exit 1\nfi\n\
             printf 'HelixForge: %s is removed\\n' {}",
            commands(source, ""),
            quote(name),
            quote(name)
        );
    }
    let mut text = String::from("hf_rc=1\n");
    for (index, source) in sources.iter().enumerate() {
        let available = if source.requires.is_empty() {
            "true".to_string()
        } else {
            source
                .requires
                .iter()
                .map(|tool| format!("command -v {} >/dev/null 2>&1", quote(tool)))
                .collect::<Vec<_>>()
                .join(" && ")
        };
        let installed = match (&source.probe, &on_path) {
            (Some(probe), _) => probe.shell(),
            (None, Some(on_path)) => on_path.clone(),
            (None, None) => "true".to_string(),
        };
        let label = quote(&source.label);
        let trying = if index == 0 {
            String::new()
        } else {
            format!("    printf 'HelixForge: trying %s\\n' {label}\n")
        };
        text.push_str(&format!(
            "if [ \"$hf_rc\" -ne 0 ]; then\n  if {available}; then\n{trying}\
             {}\n    hf_rc=$?\n    [ \"$hf_rc\" -lt 128 ] || exit \"$hf_rc\"\n\
             \x20   if [ \"$hf_rc\" -ne 0 ]; then\n\
             \x20     printf 'HelixForge: %s failed (exit %s)\\n' {label} \"$hf_rc\"\n\
             \x20   elif ! {installed}; then\n\
             \x20     printf 'HelixForge: %s finished, but %s is not installed\\n' {label} {}\n\
             \x20     hf_rc=1\n    fi\n  else\n\
             \x20   printf 'HelixForge: skipping %s: it needs %s\\n' {label} {}\n  fi\nfi\n",
            commands(source, "    "),
            quote(name),
            quote(&source.requires.join(", "))
        ));
    }
    text.push_str(&format!(
        "[ \"$hf_rc\" -eq 0 ] || printf 'HelixForge: %s was not installed\\n' {}\nexit \"$hf_rc\"",
        quote(name)
    ));
    text
}

fn quoted(words: &[String]) -> String {
    words
        .iter()
        .map(|word| quote(word))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Check that what the plan needs is installed before anything starts.
/// A package step whose first source cannot run still starts when another
/// of its sources can.
pub fn check_requirements(plan: &Plan) -> Result<(), EngineError> {
    match missing_requirements(plan, &requirement_present)
        .into_iter()
        .next()
    {
        Some(missing) => Err(EngineError::MissingExecutable(missing)),
        None => Ok(()),
    }
}

/// Whether a requirement, a command or an absolute path, is on this machine.
pub fn requirement_present(requirement: &str) -> bool {
    let path = Path::new(requirement);
    if path.is_absolute() {
        path.is_file()
    } else {
        binary_exists(requirement)
    }
}

/// The plan's requirements that are missing and that no other source of the
/// steps needing them can do without.
pub fn missing_requirements(plan: &Plan, present: &dyn Fn(&str) -> bool) -> Vec<String> {
    let mut missing = Vec::new();
    for requirement in &plan.requirements {
        if present(requirement) {
            continue;
        }
        let needing: Vec<&Step> = plan
            .steps
            .iter()
            .filter(|step| step.requirements().contains(requirement))
            .collect();
        let excused = !needing.is_empty()
            && needing.iter().all(|step| match step {
                Step::Package { sources, .. } => sources
                    .iter()
                    .any(|source| source.requires.iter().all(|tool| present(tool))),
                _ => false,
            });
        if !excused {
            missing.push(requirement.clone());
        }
    }
    missing
}

/// Whether one package step left its package as intended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Check {
    /// The step's name, such as "Ripgrep" or "Remove Ripgrep".
    pub name: String,
    pub passed: bool,
    /// What showed it, or what is wrong.
    pub detail: String,
}

/// Check every package and update step of a finished plan against the system.
pub fn verify(plan: &Plan) -> Vec<Check> {
    let mut checks = verify_with(plan, &Probe::holds, &binary_exists);
    let manager = plan.steps.iter().find_map(|step| match step {
        Step::Update { targets, .. } => targets.iter().find_map(|target| match &target.probe {
            Probe::Package { manager, .. } => Some(*manager),
            Probe::Flatpak(_) => None,
        }),
        _ => None,
    });
    if plan
        .steps
        .iter()
        .any(|step| matches!(step, Step::Update { .. }))
    {
        let after = inventory::Inventory::gather_for(manager);
        checks.extend(verify_updates_with(
            plan,
            &|probe| after.version(probe),
            &|probe| after.holds(probe),
        ));
    }
    checks
}

/// The checks of a plan's update steps: each package's version now against
/// its version before. `version` and `holds` answer for the machine.
pub fn verify_updates_with(
    plan: &Plan,
    version: &dyn Fn(&Probe) -> Option<String>,
    holds: &dyn Fn(&Probe) -> bool,
) -> Vec<Check> {
    let mut checks = Vec::new();
    for step in &plan.steps {
        let Step::Update { targets, .. } = step else {
            continue;
        };
        for target in targets {
            let (passed, detail) = match (version(&target.probe), &target.from) {
                (Some(now), Some(from)) if &now != from => {
                    (true, format!("updated {from} → {now}"))
                }
                (Some(now), Some(_)) => (
                    false,
                    format!("still {now}: no newer version was installed"),
                ),
                (Some(now), None) => (true, format!("now {now}")),
                (None, _) if holds(&target.probe) => {
                    (true, "installed; no version to compare".to_string())
                }
                (None, _) => (false, "not installed after the update".to_string()),
            };
            checks.push(Check {
                name: format!("Update {}", target.name),
                passed,
                detail,
            });
        }
    }
    checks
}

/// `verify` with the questions it asks supplied, for tests.
pub fn verify_with(
    plan: &Plan,
    holds: &dyn Fn(&Probe) -> bool,
    on_path: &dyn Fn(&str) -> bool,
) -> Vec<Check> {
    let mut checks = Vec::new();
    for step in &plan.steps {
        let Step::Package {
            name,
            binary,
            sources,
            removal,
            ..
        } = step
        else {
            continue;
        };
        let on_path = |binary: &Option<String>| binary.as_deref().is_some_and(on_path);
        let shown = binary.as_deref().unwrap_or("its command");
        let check = if *removal {
            let probe = sources.first().and_then(|source| source.probe.as_ref());
            let still = match probe {
                Some(probe) => holds(probe),
                None => on_path(binary),
            };
            Check {
                name: name.clone(),
                passed: !still,
                detail: match (still, probe) {
                    (true, Some(probe)) => format!("still installed: {}", probe.describe()),
                    (true, None) => format!("still installed: {shown} is on the PATH"),
                    (false, _) if on_path(binary) => {
                        format!("removed; {shown} is still on the PATH from another install")
                    }
                    (false, _) => "removed".to_string(),
                },
            }
        } else {
            // Its own packages answer; the command only where none can.
            let probes: Vec<&Probe> = sources
                .iter()
                .filter_map(|source| source.probe.as_ref())
                .collect();
            match probes.iter().find(|probe| holds(probe)) {
                Some(probe) => Check {
                    name: name.clone(),
                    passed: true,
                    detail: probe.describe(),
                },
                None if probes.is_empty() && on_path(binary) => Check {
                    name: name.clone(),
                    passed: true,
                    detail: format!("{shown} is on the PATH"),
                },
                None if probes.is_empty() && binary.is_none() => Check {
                    name: name.clone(),
                    passed: true,
                    detail: "finished; nothing here can confirm it".to_string(),
                },
                None if probes.is_empty() => Check {
                    name: name.clone(),
                    passed: false,
                    detail: format!("not found after the run: {shown} is not on the PATH"),
                },
                None => Check {
                    name: name.clone(),
                    passed: false,
                    detail: format!(
                        "not found after the run: {} {} not list it",
                        probes
                            .iter()
                            .map(|probe| probe.subject())
                            .collect::<Vec<_>>()
                            .join(" and "),
                        if probes.len() == 1 { "does" } else { "do" }
                    ),
                },
            }
        };
        checks.push(check);
    }
    checks
}

/// The outcome of a run, as the history records it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Outcome {
    Success,
    Failed {
        exit_code: u32,
    },
    /// Stopped by a signal or by the user; nothing is assumed about what finished.
    Interrupted,
    CouldNotStart(String),
}

impl Outcome {
    pub fn from_status(status: &ExitStatus) -> Self {
        match status.code() {
            Some(0) => Outcome::Success,
            Some(code) if code >= 128 => Outcome::Interrupted,
            Some(code) => Outcome::Failed {
                exit_code: code as u32,
            },
            None => Outcome::Interrupted,
        }
    }

    pub fn succeeded(&self) -> bool {
        matches!(self, Outcome::Success)
    }

    pub fn describe(&self) -> String {
        match self {
            Outcome::Success => "exit 0".to_string(),
            Outcome::Failed { exit_code } => format!("exit {exit_code}"),
            Outcome::Interrupted => "interrupted".to_string(),
            Outcome::CouldNotStart(reason) => format!("could not start: {reason}"),
        }
    }
}

/// A finished run: how the program ended and, when it succeeded, what the
/// checks of its package steps found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finished {
    pub outcome: Outcome,
    pub checks: Vec<Check>,
}

impl Finished {
    /// Check a run's package steps if the program succeeded.
    pub fn new(plan: &Plan, outcome: Outcome) -> Self {
        let checks = if outcome.succeeded() {
            verify(plan)
        } else {
            Vec::new()
        };
        Self { outcome, checks }
    }

    /// The program succeeded and every check passed.
    pub fn succeeded(&self) -> bool {
        self.outcome.succeeded() && self.checks.iter().all(|check| check.passed)
    }

    pub fn verified(&self) -> bool {
        !self.checks.is_empty() && self.succeeded()
    }

    /// The result as the history records it: "exit 0, verified",
    /// "exit 0, not verified: Meld (…)", or the outcome alone.
    pub fn describe(&self) -> String {
        let mut text = self.outcome.describe();
        if self.checks.is_empty() {
            return text;
        }
        let failed: Vec<String> = self
            .checks
            .iter()
            .filter(|check| !check.passed)
            .map(|check| format!("{} ({})", check.name, check.detail))
            .collect();
        if failed.is_empty() {
            text.push_str(", verified");
        } else {
            text.push_str(&format!(", not verified: {}", failed.join("; ")));
        }
        text
    }
}

/// Something that can run a shell program: the caller's terminal, a
/// pseudo-terminal, or a test double.
pub trait Runner {
    fn run(&mut self, program: &str, lib_dir: &Path) -> Outcome;
}

/// Runs in the caller's terminal, inheriting stdin and stdout.
pub struct InheritedRunner;

impl Runner for InheritedRunner {
    fn run(&mut self, program: &str, lib_dir: &Path) -> Outcome {
        match Command::new("sh")
            .arg("-c")
            .arg(program)
            .env(LIB_VARIABLE, lib_dir)
            .status()
        {
            Ok(status) => Outcome::from_status(&status),
            Err(error) => Outcome::CouldNotStart(error.to_string()),
        }
    }
}

/// Records the program instead of running it, for tests.
#[derive(Default)]
pub struct MockRunner {
    pub programs: Vec<String>,
    pub outcome: Option<Outcome>,
}

impl Runner for MockRunner {
    fn run(&mut self, program: &str, _lib_dir: &Path) -> Outcome {
        self.programs.push(program.to_string());
        self.outcome.clone().unwrap_or(Outcome::Success)
    }
}

/// Run a plan through a runner and record the outcome in the default history.
pub fn execute(
    plan: &Plan,
    lib_dir: &Path,
    this: &Path,
    runner: &mut dyn Runner,
) -> Result<Finished, EngineError> {
    let history = history::state_dir();
    execute_in(plan, lib_dir, this, runner, history.as_deref())
}

/// Run a plan through a runner and record the outcome in `history_dir`.
pub fn execute_in(
    plan: &Plan,
    lib_dir: &Path,
    this: &Path,
    runner: &mut dyn Runner,
    history_dir: Option<&Path>,
) -> Result<Finished, EngineError> {
    check_requirements(plan)?;
    info!(steps = ?plan.entries(), "run started");
    let not_recorded = |directory: &Path, error: std::io::Error| {
        warn!(%error, "the run was not recorded");
        eprintln!(
            "HelixForge could not record this run in {}: {error}",
            directory.display()
        );
    };
    // The marker records the run even if this process dies before it ends.
    let pending = history_dir.and_then(|directory| {
        history::begin_in(directory, &plan.entries())
            .inspect_err(|error| warn!(%error, "the run start was not recorded"))
            .ok()
    });
    let outcome = runner.run(&shell_program(plan, lib_dir, this), lib_dir);
    let finished = Finished::new(plan, outcome);
    info!(outcome = finished.describe(), "run finished");
    if let Some(directory) = history_dir {
        let written = match pending {
            Some(pending) => pending.finish(finished.succeeded(), &finished.describe(), None),
            None => history::record_in(
                directory,
                &plan.entries(),
                finished.succeeded(),
                &finished.describe(),
                None,
            ),
        };
        if let Err(error) = written {
            not_recorded(directory, error);
        }
    }
    Ok(finished)
}

/// Run a native action and return its text.
pub fn run_native(action: NativeAction, platform: &LinuxInfo) -> String {
    match action {
        NativeAction::SystemReport => {
            let mut lines = vec![
                format!(
                    "Distribution:    {} ({})",
                    platform.pretty_name, platform.id
                ),
                format!(
                    "Package manager: {}",
                    platform
                        .manager
                        .map(|manager| manager.label())
                        .unwrap_or("none supported for package entries")
                ),
                format!(
                    "Kernel:          {}",
                    std::fs::read_to_string("/proc/sys/kernel/osrelease")
                        .unwrap_or_default()
                        .trim()
                ),
                format!("Architecture:    {}", std::env::consts::ARCH),
            ];
            for tool in ["sudo", "nix", "home-manager", "flatpak"] {
                lines.push(format!(
                    "{:<16} {}",
                    format!("{tool}:"),
                    if binary_exists(tool) {
                        "present"
                    } else {
                        "missing"
                    }
                ));
            }
            lines.join("\n") + "\n"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helixforge_core::PackageNames;

    fn platform(manager: Option<PackageManager>) -> LinuxInfo {
        LinuxInfo {
            id: "test".into(),
            pretty_name: "Test Linux".into(),
            id_like: vec![],
            manager,
        }
    }

    fn entry(id: &str, flags: &str, action: Action) -> Entry {
        Entry {
            id: id.into(),
            name: id.to_uppercase(),
            description: String::new(),
            flags: flags.into(),
            multi_select: true,
            writes: Vec::new(),
            action,
            detect: None,
            needs: Default::default(),
        }
    }

    fn package_entry() -> Entry {
        entry(
            "sample",
            "I",
            Action::Package {
                binary: Some("helixforge-nonexistent-binary-12345".into()),
                packages: PackageNames {
                    arch: Some("sample".into()),
                    debian: Some("sample-deb".into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
                conflicts: Vec::new(),
                removable: true,
            },
        )
    }

    fn none<'a>(_: &str) -> Option<&'a Entry> {
        None
    }

    #[test]
    fn package_plans_use_the_native_manager_and_say_what_they_need() {
        let sample = package_entry();
        let arch = plan([&sample], &platform(Some(PackageManager::Arch)), &none).unwrap();
        assert_eq!(arch.steps[0].display(), "sudo pacman -S --needed sample");
        assert!(arch.privileged);
        assert_eq!(arch.risk, Risk::Medium);
        assert_eq!(arch.requirements, ["pacman", "sudo"]);
        let debian = plan([&sample], &platform(Some(PackageManager::Debian)), &none).unwrap();
        assert_eq!(debian.steps[0].display(), "sudo apt-get install sample-deb");
        assert!(matches!(
            plan([&sample], &platform(Some(PackageManager::Fedora)), &none),
            Err(EngineError::UnsupportedEntry(_))
        ));
        assert!(matches!(
            plan([&sample], &platform(None), &none),
            Err(EngineError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn aur_and_flatpak_routes_and_the_other_families() {
        let mut entry = package_entry();
        entry.action = Action::Package {
            binary: Some("helixforge-nonexistent-binary-12345".into()),
            packages: PackageNames {
                suse: Some("sample-suse".into()),
                ..Default::default()
            },
            aur: Some("sample-bin".into()),
            flatpak: Some("org.example.Sample".into()),
            conflicts: Vec::new(),
            removable: false,
        };
        let suse = plan([&entry], &platform(Some(PackageManager::Suse)), &none).unwrap();
        assert_eq!(
            suse.steps[0].display(),
            "sudo zypper install sample-suse  (else Flathub)"
        );
        assert_eq!(suse.requirements, ["sudo", "zypper"]);
        assert_eq!(
            suse.steps[0].fallbacks(),
            [format!(
                "if that fails, Flathub: sudo flatpak remote-add --if-not-exists flathub {FLATHUB} && sudo flatpak install flathub org.example.Sample"
            )]
        );
        let arch = plan([&entry], &platform(Some(PackageManager::Arch)), &none).unwrap();
        let shown = arch.steps[0].display();
        assert!(
            shown == "paru -S --needed sample-bin  (else Flathub)"
                || shown == "yay -S --needed sample-bin  (else Flathub)",
            "{shown}"
        );
        let fedora = plan([&entry], &platform(Some(PackageManager::Fedora)), &none).unwrap();
        assert_eq!(
            fedora.steps[0].display(),
            format!(
                "sudo flatpak remote-add --if-not-exists flathub {FLATHUB} && sudo flatpak install flathub org.example.Sample"
            )
        );
        assert_eq!(fedora.requirements, ["flatpak", "sudo"]);
        assert!(fedora.steps[0].fallbacks().is_empty());
        let unknown = plan([&entry], &platform(None), &none).unwrap();
        assert!(unknown.steps[0].display().contains("flatpak install"));
        assert_eq!(aur_helper(&|name| name == "yay"), Some("yay"));
        assert_eq!(aur_helper(&|_| true), Some("paru"));
        assert_eq!(aur_helper(&|_| false), None);
    }

    #[test]
    fn installed_state_comes_from_packages_and_conflicts_before_commands() {
        let arch = platform(Some(PackageManager::Arch));
        let package = |name: &str, binary: Option<&str>, conflicts: &[&str]| {
            entry(
                name,
                "I",
                Action::Package {
                    binary: binary.map(Into::into),
                    packages: PackageNames {
                        arch: Some(name.into()),
                        ..Default::default()
                    },
                    aur: None,
                    flatpak: Some(format!("org.example.{name}")),
                    conflicts: conflicts.iter().map(|name| name.to_string()).collect(),
                    removable: true,
                },
            )
        };
        let staging = package("wine-staging", None, &["wine"]);
        let wine = package("wine", Some("wine"), &["wine-staging"]);
        // Plain WINE installed: wine on the PATH, pacman lists wine.
        let plain = Machine {
            holds: &|probe| probe == &Probe::package(PackageManager::Arch, "wine").unwrap(),
            on_path: &|binary| binary == "wine",
        };
        assert_eq!(
            package_state(&staging, &arch, plain),
            Some(PackageState::Absent {
                replaces: vec!["wine".into()]
            })
        );
        let planned = plan_with([&staging], &arch, &none, plain).unwrap();
        assert_eq!(
            planned.steps.len(),
            1,
            "staging is planned, not taken as installed"
        );
        assert!(planned.notes.is_empty());
        // The preflight says what it replaces.
        let facts = preflight::Facts {
            machine: plain,
            present: &|_| true,
            free_bytes: &|_| Some(u64::MAX),
            gpus: &Vec::new,
            repositories: &|| None,
            sizes: &|_, _| None,
        };
        let lookup = |id: &str| (id == "wine-staging").then_some(&staging);
        assert_eq!(
            preflight::check(&planned, &arch, &lookup, &facts),
            [preflight::Finding {
                severity: preflight::Severity::Warning,
                message: "WINE-STAGING replaces wine, which is installed; the package manager asks before removing it".into(),
            }]
        );
        assert_eq!(
            package_state(&wine, &arch, plain),
            Some(PackageState::Installed("pacman lists wine".into()))
        );
        // Staging installed: its wine command does not make plain WINE installed.
        let staged = Machine {
            holds: &|probe| probe == &Probe::package(PackageManager::Arch, "wine-staging").unwrap(),
            on_path: &|binary| binary == "wine",
        };
        assert_eq!(
            package_state(&wine, &arch, staged),
            Some(PackageState::Absent {
                replaces: vec!["wine-staging".into()]
            })
        );
        // From Nix or a download: no package listed, the command on the PATH.
        let elsewhere = Machine {
            holds: &|_| false,
            on_path: &|binary| binary == "wine",
        };
        assert_eq!(
            package_state(&wine, &arch, elsewhere),
            Some(PackageState::OnPath("wine is on the PATH".into()))
        );
        let noted = plan_with([&wine], &arch, &none, elsewhere).unwrap();
        assert!(noted.steps.is_empty());
        assert_eq!(
            noted.notes,
            ["WINE: already installed (wine is on the PATH)"]
        );
        // Without a command, nothing on the PATH counts.
        assert_eq!(
            package_state(&staging, &arch, elsewhere),
            Some(PackageState::Absent { replaces: vec![] })
        );
        // Its Flatpak counts on any family.
        let flatpak = Machine {
            holds: &|probe| matches!(probe, Probe::Flatpak(_)),
            on_path: &|_| false,
        };
        assert_eq!(
            package_state(&staging, &platform(Some(PackageManager::Fedora)), flatpak),
            Some(PackageState::Installed(
                "org.example.wine-staging is installed from Flathub".into()
            ))
        );
        let script = entry(
            "script",
            "",
            Action::Command {
                line: "true".into(),
            },
        );
        assert_eq!(package_state(&script, &arch, plain), None);
    }

    #[test]
    fn removal_follows_how_the_package_is_installed() {
        let mut present = package_entry();
        present.action = Action::Package {
            binary: Some("sh".into()),
            packages: PackageNames {
                debian: Some("dash".into()),
                ..Default::default()
            },
            aur: Some("sh-bin".into()),
            flatpak: None,
            conflicts: Vec::new(),
            removable: true,
        };
        // Every package listed, and sh on the PATH.
        let listed = Machine {
            holds: &|_| true,
            on_path: &|binary| binary == "sh",
        };
        let remove = |entry: &Entry, manager, machine| {
            plan_removal_with([entry], &platform(Some(manager)), machine).unwrap()
        };
        let debian = remove(&present, PackageManager::Debian, listed);
        assert_eq!(debian.steps[0].display(), "sudo apt-get remove dash");
        assert_eq!(debian.steps[0].name(), "Remove SAMPLE");
        assert!(debian.privileged);
        assert_eq!(debian.risk, Risk::Medium);
        let arch = remove(&present, PackageManager::Arch, listed);
        assert_eq!(arch.steps[0].display(), "sudo pacman -R sh-bin");
        // Nothing listed: the command alone is someone else's install.
        let unlisted = Machine {
            holds: &|_| false,
            on_path: &|binary| binary == "sh",
        };
        let other = remove(&present, PackageManager::Arch, unlisted);
        assert!(other.steps.is_empty());
        assert!(other.notes[0].contains("not by a package"));
        let fedora = remove(&present, PackageManager::Fedora, listed);
        assert!(fedora.steps.is_empty(), "Fedora has no package for it");
        let nothing = Machine {
            holds: &|_| false,
            on_path: &|_| false,
        };
        let absent = remove(&package_entry(), PackageManager::Arch, nothing);
        assert!(absent.steps.is_empty());
        assert!(absent.notes[0].contains("not installed"));
        // The Flatpak is found and removed as such.
        let mut flatpak = package_entry();
        if let Action::Package { flatpak: id, .. } = &mut flatpak.action {
            *id = Some("org.example.Sample".into());
        }
        let only_flatpak = Machine {
            holds: &|probe| matches!(probe, Probe::Flatpak(_)),
            on_path: &|_| false,
        };
        let removed = remove(&flatpak, PackageManager::Arch, only_flatpak);
        assert_eq!(
            removed.steps[0].display(),
            "sudo flatpak uninstall org.example.Sample"
        );
        let mut fixed = package_entry();
        if let Action::Package { removable, .. } = &mut fixed.action {
            *removable = false;
        }
        assert!(matches!(
            plan_removal([&fixed], &platform(Some(PackageManager::Arch))),
            Err(EngineError::NotRemovable(_))
        ));
        let script = entry(
            "script",
            "",
            Action::Command {
                line: "true".into(),
            },
        );
        assert!(matches!(
            plan_removal([&script], &platform(Some(PackageManager::Arch))),
            Err(EngineError::NotRemovable(_))
        ));
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("app/org.example.Sample")).unwrap();
        assert!(flatpak_installed_in(
            &[root.path().to_path_buf()],
            "org.example.Sample"
        ));
        assert!(!flatpak_installed_in(
            &[root.path().to_path_buf()],
            "org.example.Other"
        ));
    }

    #[test]
    fn installed_packages_become_notes_and_profiles_expand() {
        let installed = entry(
            "shell",
            "I",
            Action::Package {
                binary: Some("sh".into()),
                packages: PackageNames {
                    arch: Some("sh".into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
                conflicts: Vec::new(),
                removable: true,
            },
        );
        let report = entry(
            "report",
            "",
            Action::Native {
                action: NativeAction::SystemReport,
            },
        );
        let profile = entry(
            "basics",
            "",
            Action::Profile {
                entries: vec!["shell".into(), "report".into()],
            },
        );
        let members = [installed.clone(), report.clone()];
        let lookup = |id: &str| members.iter().find(|entry| entry.id == id);
        let planned = plan([&profile], &platform(Some(PackageManager::Arch)), &lookup).unwrap();
        assert_eq!(planned.steps.len(), 1);
        assert_eq!(planned.steps[0].id(), "report");
        assert_eq!(planned.notes.len(), 1);
        assert!(!planned.privileged);
        assert_eq!(planned.risk, Risk::Low);
        let broken = entry(
            "broken",
            "",
            Action::Profile {
                entries: vec!["ghost".into()],
            },
        );
        assert!(matches!(
            super::plan([&broken], &platform(Some(PackageManager::Arch)), &lookup),
            Err(EngineError::UnknownMember(_, _))
        ));
    }

    #[test]
    fn the_shell_program_runs_steps_in_order_and_stops_on_failure() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("it's.sh");
        std::fs::write(
            &script,
            "#!/bin/sh\nprintf 'lib=%s\\n' \"$HELIXFORGE_LIB\"\npwd\n",
        )
        .unwrap();
        let plan = Plan {
            steps: vec![
                Step::Script {
                    id: "s".into(),
                    name: "S".into(),
                    file: script,
                    program: "/bin/sh".into(),
                    args: vec!["-e".into()],
                    interactive: true,
                },
                Step::Command {
                    id: "fail".into(),
                    name: "Fail".into(),
                    line: "echo before; exit 7".into(),
                },
                Step::Command {
                    id: "never".into(),
                    name: "Never".into(),
                    line: "echo never".into(),
                },
            ],
            ..Default::default()
        };
        let program = shell_program(
            &plan,
            Path::new("/tmp/lib dir"),
            Path::new("/usr/bin/helixforge"),
        );
        let output = Command::new("sh").arg("-c").arg(&program).output().unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(7));
        assert!(text.contains("==> S"));
        assert!(text.contains("lib=/tmp/lib dir"));
        assert!(text.contains(&directory.path().to_string_lossy().to_string()));
        assert!(text.contains("before"));
        assert!(!text.contains("never"));
    }

    #[test]
    fn native_steps_reinvoke_helixforge() {
        let plan = Plan {
            steps: vec![Step::Native {
                id: "report".into(),
                name: "Report".into(),
                action: NativeAction::SystemReport,
            }],
            ..Default::default()
        };
        let program = shell_program(&plan, Path::new("/lib"), Path::new("/opt/helix forge"));
        assert!(program.contains("'/opt/helix forge' 'native' 'system-report'"));
    }

    #[test]
    fn outcomes_tell_failure_from_interruption() {
        assert_eq!(
            Outcome::from_status(&Command::new("true").status().unwrap()),
            Outcome::Success
        );
        assert_eq!(
            Outcome::from_status(&Command::new("sh").args(["-c", "exit 3"]).status().unwrap()),
            Outcome::Failed { exit_code: 3 }
        );
        assert_eq!(
            Outcome::from_status(
                &Command::new("sh")
                    .args(["-c", "kill -TERM $$"])
                    .status()
                    .unwrap()
            ),
            Outcome::Interrupted
        );
        assert_eq!(
            Outcome::from_status(
                &Command::new("sh")
                    .args(["-c", "exit 130"])
                    .status()
                    .unwrap()
            ),
            Outcome::Interrupted
        );
    }

    #[test]
    fn execution_goes_through_the_runner_and_is_recorded() {
        let state = tempfile::tempdir().unwrap();
        let plan = Plan {
            steps: vec![Step::Command {
                id: "hello".into(),
                name: "Hello".into(),
                line: "echo hello".into(),
            }],
            ..Default::default()
        };
        let mut runner = MockRunner {
            outcome: Some(Outcome::Failed { exit_code: 4 }),
            ..Default::default()
        };
        let outcome = execute_in(
            &plan,
            Path::new("/lib"),
            Path::new("/bin/helixforge"),
            &mut runner,
            Some(state.path()),
        )
        .unwrap();
        assert_eq!(outcome.outcome, Outcome::Failed { exit_code: 4 });
        assert!(outcome.checks.is_empty());
        assert_eq!(runner.programs.len(), 1);
        assert!(runner.programs[0].contains("sh -e -c 'echo hello'"));
        let records = history::read_in(state.path(), 1).unwrap();
        assert_eq!(records[0].entries, ["hello"]);
        assert!(!records[0].success);
        assert_eq!(records[0].result, "exit 4");
    }

    #[test]
    fn missing_requirements_stop_a_run_before_it_starts() {
        let plan = Plan {
            requirements: vec!["helixforge-no-such-tool".into()],
            ..Default::default()
        };
        let mut runner = MockRunner::default();
        assert!(matches!(
            execute(
                &plan,
                Path::new("/lib"),
                Path::new("/bin/helixforge"),
                &mut runner
            ),
            Err(EngineError::MissingExecutable(_))
        ));
        assert!(runner.programs.is_empty());
    }

    #[test]
    fn home_manager_managed_paths_block_an_entry() {
        let home = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(home.path().join(".config")).unwrap();
        std::os::unix::fs::symlink("/nix/store/abc-nvim", home.path().join(".config/nvim"))
            .unwrap();
        std::fs::create_dir_all(home.path().join(".config/ghostty")).unwrap();
        assert_eq!(
            managed_path_in(home.path(), &["~/.config/nvim/init.lua".to_string()]),
            Some(home.path().join(".config/nvim"))
        );
        assert_eq!(
            managed_path_in(home.path(), &["~/.config/ghostty/config".to_string()]),
            None
        );
        // Planning reads HOME; an unmanaged absolute path outside it is fine.
        let mut free = entry(
            "free",
            "FM",
            Action::Command {
                line: "true".into(),
            },
        );
        free.writes = vec![
            home.path()
                .join(".config/ghostty")
                .to_string_lossy()
                .into_owned(),
        ];
        let planned = plan([&free], &platform(None), &none).unwrap();
        assert_eq!(planned.steps.len(), 1);
        assert!(planned.blocked.is_empty());
        let mut managed = entry(
            "nvim",
            "FM",
            Action::Command {
                line: "true".into(),
            },
        );
        managed.writes = vec![
            home.path()
                .join(".config/nvim")
                .to_string_lossy()
                .into_owned(),
        ];
        let planned = plan([&managed], &platform(None), &none).unwrap();
        assert!(planned.steps.is_empty());
        assert!(planned.blocked[0].contains("Home Manager"));
    }

    /// A package step whose sources are harmless commands: `bin` is a
    /// temporary directory put first on the PATH, where sources create or
    /// delete the executable `hf-sample`.
    fn fake_package(bin: &Path, sources: Vec<(&str, &str, &[&str])>, removal: bool) -> Plan {
        let sample = bin.join("hf-sample");
        let sample = quote(&sample.to_string_lossy());
        Plan {
            steps: vec![Step::Package {
                id: "sample".into(),
                name: "Sample".into(),
                binary: Some("hf-sample".into()),
                sources: sources
                    .into_iter()
                    .map(|(label, line, requires)| Source {
                        label: label.into(),
                        commands: vec![vec![
                            "sh".into(),
                            "-c".into(),
                            line.replace("SAMPLE", &sample),
                        ]],
                        requires: requires.iter().map(|tool| tool.to_string()).collect(),
                        probe: None,
                    })
                    .collect(),
                removal,
            }],
            ..Default::default()
        }
    }

    fn run_in_shell(plan: &Plan, bin: &Path) -> (Option<i32>, String) {
        let path = format!(
            "{}:{}",
            bin.display(),
            std::env::var("PATH").unwrap_or_default()
        );
        let output = Command::new("sh")
            .arg("-c")
            .arg(shell_program(plan, Path::new("/lib"), Path::new("/bin/hf")))
            .env("PATH", path)
            .output()
            .unwrap();
        (
            output.status.code(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        )
    }

    const CREATE: &str = "printf '#!/bin/sh\\n' > SAMPLE && chmod +x SAMPLE";

    #[test]
    fn a_package_falls_back_to_the_next_source_that_works() {
        let bin = tempfile::tempdir().unwrap();
        let plan = fake_package(
            bin.path(),
            vec![
                ("first", "exit 3", &["sh"]),
                ("second", CREATE, &["helixforge-no-such-tool"]),
                ("third", CREATE, &["sh"]),
                ("fourth", "echo never", &["sh"]),
            ],
            false,
        );
        let (code, text) = run_in_shell(&plan, bin.path());
        assert_eq!(code, Some(0), "{text}");
        assert!(text.contains("HelixForge: first failed (exit 3)"), "{text}");
        assert!(
            text.contains("HelixForge: skipping second: it needs helixforge-no-such-tool"),
            "{text}"
        );
        assert!(text.contains("HelixForge: trying third"), "{text}");
        assert!(!text.contains("never"), "{text}");
        assert!(bin.path().join("hf-sample").exists());
    }

    #[test]
    fn a_source_that_installs_nothing_is_not_trusted() {
        let bin = tempfile::tempdir().unwrap();
        let alone = fake_package(bin.path(), vec![("quiet", "true", &["sh"])], false);
        let (code, text) = run_in_shell(&alone, bin.path());
        assert_eq!(code, Some(1), "{text}");
        assert!(
            text.contains("HelixForge: quiet finished, but Sample is not installed"),
            "{text}"
        );
        assert!(
            text.contains("HelixForge: Sample was not installed"),
            "{text}"
        );
        let rescued = fake_package(
            bin.path(),
            vec![("quiet", "true", &["sh"]), ("real", CREATE, &["sh"])],
            false,
        );
        let (code, text) = run_in_shell(&rescued, bin.path());
        assert_eq!(code, Some(0), "{text}");
        assert!(text.contains("HelixForge: trying real"), "{text}");
    }

    #[test]
    fn an_interrupted_source_stops_the_run_without_falling_back() {
        let bin = tempfile::tempdir().unwrap();
        let plan = fake_package(
            bin.path(),
            vec![("first", "exit 130", &["sh"]), ("second", CREATE, &["sh"])],
            false,
        );
        let (code, text) = run_in_shell(&plan, bin.path());
        assert_eq!(code, Some(130), "{text}");
        assert!(!text.contains("trying second"), "{text}");
        assert!(!bin.path().join("hf-sample").exists());
    }

    #[test]
    fn a_removal_fails_when_the_package_is_still_there() {
        let bin = tempfile::tempdir().unwrap();
        std::fs::write(bin.path().join("hf-sample"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(
            bin.path().join("hf-sample"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let kept = fake_package(bin.path(), vec![("keeper", "true", &["sh"])], true);
        let (code, text) = run_in_shell(&kept, bin.path());
        assert_eq!(code, Some(1), "{text}");
        assert!(
            text.contains("HelixForge: Sample is still installed"),
            "{text}"
        );
        let removed = fake_package(bin.path(), vec![("remover", "rm SAMPLE", &["sh"])], true);
        let (code, text) = run_in_shell(&removed, bin.path());
        assert_eq!(code, Some(0), "{text}");
        assert!(text.contains("HelixForge: Sample is removed"), "{text}");
        let failing = fake_package(bin.path(), vec![("remover", "exit 5", &["sh"])], true);
        assert_eq!(run_in_shell(&failing, bin.path()).0, Some(5));
    }

    #[test]
    fn probes_ask_the_package_manager_and_flatpak() {
        let pacman = Probe::package(PackageManager::Arch, "ripgrep").unwrap();
        assert_eq!(pacman.shell(), "'pacman' '-Q' 'ripgrep' >/dev/null 2>&1");
        assert_eq!(pacman.describe(), "pacman lists ripgrep");
        let rpm = Probe::package(PackageManager::Suse, "lynx").unwrap();
        assert_eq!(rpm.shell(), "'rpm' '-q' 'lynx' >/dev/null 2>&1");
        let dpkg = Probe::package(PackageManager::Debian, "lynx").unwrap();
        assert_eq!(
            dpkg.shell(),
            "[ \"$(dpkg-query -W -f='${db:Status-Status}' 'lynx' 2>/dev/null)\" = installed ]"
        );
        assert_eq!(Probe::package(PackageManager::Solus, "lynx"), None);
        let home = tempfile::tempdir().unwrap();
        let flatpak = Probe::Flatpak("org.example.Sample".into());
        let test = |probe: &Probe| {
            Command::new("sh")
                .arg("-c")
                .arg(probe.shell())
                .env("HOME", home.path())
                .status()
                .unwrap()
                .success()
        };
        assert!(!test(&flatpak));
        std::fs::create_dir_all(
            home.path()
                .join(".local/share/flatpak/app/org.example.Sample"),
        )
        .unwrap();
        assert!(test(&flatpak));
    }

    #[test]
    fn verification_checks_installs_and_removals() {
        let source = |probe: Option<Probe>| Source {
            label: "pacman".into(),
            commands: Vec::new(),
            requires: Vec::new(),
            probe,
        };
        let listed = Probe::package(PackageManager::Arch, "meld").unwrap();
        let probed =
            |name: &str, binary: &str, probe: Option<Probe>, removal: bool| Step::Package {
                id: name.to_lowercase(),
                name: name.into(),
                binary: Some(binary.into()),
                sources: vec![source(probe)],
                removal,
            };
        let step = |name: &str, binary: &str, removal: bool| {
            probed(name, binary, Some(listed.clone()), removal)
        };
        let plan = Plan {
            steps: vec![
                // Without a probe (Solus) the command is the only answer.
                probed("Ripgrep", "rg", None, false),
                step("Meld", "meld", false),
                Step::Command {
                    id: "c".into(),
                    name: "C".into(),
                    line: "true".into(),
                },
            ],
            ..Default::default()
        };
        let on_path = |binary: &str| binary == "rg";
        let checks = verify_with(&plan, &|_| true, &on_path);
        assert_eq!(checks.len(), 2);
        assert_eq!(checks[0].detail, "rg is on the PATH");
        assert_eq!(checks[1].detail, "pacman lists meld");
        assert!(checks.iter().all(|check| check.passed));
        let checks = verify_with(&plan, &|_| false, &on_path);
        assert!(checks[0].passed);
        assert!(!checks[1].passed);
        assert_eq!(
            checks[1].detail,
            "not found after the run: pacman (meld) does not list it"
        );
        // A command from another install is no proof where the package
        // manager can be asked.
        let elsewhere = verify_with(&plan, &|_| false, &|_| true);
        assert!(!elsewhere[1].passed);
        let finished = Finished {
            outcome: Outcome::Success,
            checks,
        };
        assert!(!finished.succeeded());
        assert_eq!(
            finished.describe(),
            "exit 0, not verified: Meld (not found after the run: pacman (meld) does not list it)"
        );
        let removal = Plan {
            steps: vec![step("Remove Meld", "meld", true)],
            ..Default::default()
        };
        let gone = verify_with(&removal, &|_| false, &|_| false);
        assert!(gone[0].passed);
        assert_eq!(gone[0].detail, "removed");
        let other = verify_with(&removal, &|_| false, &|_| true);
        assert!(other[0].passed);
        assert!(other[0].detail.contains("from another install"));
        let still = verify_with(&removal, &|_| true, &|_| false);
        assert!(!still[0].passed);
        assert_eq!(still[0].detail, "still installed: pacman lists meld");
        let passed = Finished {
            outcome: Outcome::Success,
            checks: gone,
        };
        assert!(passed.verified());
        assert_eq!(passed.describe(), "exit 0, verified");
        let plain = Finished {
            outcome: Outcome::Failed { exit_code: 2 },
            checks: Vec::new(),
        };
        assert!(!plain.verified());
        assert_eq!(plain.describe(), "exit 2");
    }

    #[test]
    fn a_missing_first_source_does_not_block_a_package_with_another() {
        let sources = |first: &str, second: &str| {
            vec![
                Source {
                    label: "first".into(),
                    commands: Vec::new(),
                    requires: vec![first.into()],
                    probe: None,
                },
                Source {
                    label: "second".into(),
                    commands: Vec::new(),
                    requires: vec![second.into()],
                    probe: None,
                },
            ]
        };
        let plan = |sources| Plan {
            steps: vec![Step::Package {
                id: "p".into(),
                name: "P".into(),
                binary: Some("p".into()),
                sources,
                removal: false,
            }],
            requirements: vec!["helixforge-no-such-tool".into()],
            ..Default::default()
        };
        assert!(check_requirements(&plan(sources("helixforge-no-such-tool", "sh"))).is_ok());
        assert!(matches!(
            check_requirements(&plan(sources(
                "helixforge-no-such-tool",
                "helixforge-no-such-tool-2"
            ))),
            Err(EngineError::MissingExecutable(_))
        ));
    }

    #[test]
    fn quoting_keeps_metacharacters_literal() {
        let value = "spaces ' $(false) ; \" end";
        let output = Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote(value)))
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(output.stdout).unwrap(), value);
    }
}
