//! Turns entries into explicit plans, runs them, and records what happened.
//!
//! Every interface uses this crate the same way: `plan` an entry (or several)
//! into a [`Plan`] of ordered [`Step`]s with what they need, whether they are
//! privileged and how risky they are; show it; then run it through a
//! [`Runner`]. The steps become one POSIX shell program that stops at the
//! first failure. The shell only ever sees quoted literals from the validated
//! catalog, never free text. Native operations run back inside HelixForge
//! itself, so even they go through the same program and the same terminal.

pub mod history;
pub mod pty;

use helixforge_core::{
    Action, Entry, NativeAction, NixAction, PackageManager, Risk, valid_package_name,
};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
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
    /// Installs or removes a package: one or more commands run in order.
    Package {
        id: String,
        name: String,
        commands: Vec<Vec<String>>,
        requires: Vec<String>,
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
}

impl Step {
    pub fn id(&self) -> &str {
        match self {
            Step::Package { id, .. }
            | Step::Script { id, .. }
            | Step::Command { id, .. }
            | Step::Nix { id, .. }
            | Step::Native { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Step::Package { name, .. }
            | Step::Script { name, .. }
            | Step::Command { name, .. }
            | Step::Nix { name, .. }
            | Step::Native { name, .. } => name,
        }
    }

    /// The command line as the shell will run it, for previews and plans.
    pub fn display(&self) -> String {
        match self {
            Step::Package { commands, .. } => commands
                .iter()
                .map(|command| command.join(" "))
                .collect::<Vec<_>>()
                .join(" && "),
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
        }
    }

    /// The executables a step needs before it can start.
    fn requirements(&self) -> Vec<String> {
        match self {
            Step::Package { requires, .. } => requires.clone(),
            Step::Nix { action, .. } => match action {
                NixAction::ProfileInstall { .. } => vec!["nix".to_string()],
                NixAction::HomeManagerSwitch { .. } => vec!["home-manager".to_string()],
            },
            Step::Script { program, .. } => vec![program.clone()],
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

/// Plan one or more entries. Profiles expand into their members; a package
/// whose executable is already on the PATH becomes a note instead of a step.
pub fn plan<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
) -> Result<Plan, EngineError> {
    let mut result = Plan::default();
    for entry in entries {
        add_entry(entry, platform, lookup, &mut result, &mut Vec::new())?;
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
            if let Some(found) = installed(binary, flatpak.as_deref()) {
                into.notes
                    .push(format!("{}: already installed ({found})", entry.name));
                return Ok(());
            }
            let (commands, requires) = install_commands(
                entry,
                platform,
                packages,
                aur.as_deref(),
                flatpak.as_deref(),
            )?;
            Step::Package {
                id: entry.id.clone(),
                name: entry.name.clone(),
                commands,
                requires,
            }
        }
        Action::Script { file, interactive } => {
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
                add_entry(found, platform, lookup, into, trail)?;
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

fn words(base: Vec<&str>, extra: &[&str]) -> Vec<String> {
    base.into_iter()
        .chain(extra.iter().copied())
        .map(str::to_string)
        .collect()
}

/// Where an installed package was found: its executable on the PATH, or its
/// Flatpak in the system or user installation.
fn installed(binary: &str, flatpak: Option<&str>) -> Option<String> {
    if binary_exists(binary) {
        return Some(format!("{binary} is on the PATH"));
    }
    let id = flatpak?;
    let mut roots = vec![PathBuf::from("/var/lib/flatpak")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(home).join(".local/share/flatpak"));
    }
    flatpak_installed_in(&roots, id).then(|| format!("{id} is installed from Flathub"))
}

/// Whether a Flatpak application is installed under any of the roots.
pub fn flatpak_installed_in(roots: &[PathBuf], id: &str) -> bool {
    roots.iter().any(|root| root.join("app").join(id).is_dir())
}

/// The AUR helper to use, paru before yay.
pub fn aur_helper(exists: &dyn Fn(&str) -> bool) -> Option<&'static str> {
    ["paru", "yay"].into_iter().find(|helper| exists(helper))
}

/// The commands that install a package entry on this machine, and what they need.
fn install_commands(
    entry: &Entry,
    platform: &LinuxInfo,
    packages: &helixforge_core::PackageNames,
    aur: Option<&str>,
    flatpak: Option<&str>,
) -> Result<(Vec<Vec<String>>, Vec<String>), EngineError> {
    let check = |name: &str| {
        if valid_package_name(name) {
            Ok(())
        } else {
            Err(EngineError::Invalid(entry.id.clone()))
        }
    };
    if let Some(manager) = platform.manager {
        if let Some(package) = packages.for_manager(manager) {
            check(package)?;
            return Ok((
                vec![words(manager.install_command(), &[package])],
                vec![manager.executable().to_string(), "sudo".to_string()],
            ));
        }
        if manager == PackageManager::Arch
            && let Some(package) = aur
        {
            check(package)?;
            // A missing helper becomes a requirement the run reports before starting.
            let helper = aur_helper(&|name| binary_exists(name)).unwrap_or("paru");
            return Ok((
                vec![words(vec![helper, "-S", "--needed"], &[package])],
                vec![helper.to_string()],
            ));
        }
    }
    if let Some(id) = flatpak {
        if !helixforge_core::valid_flatpak_id(id) {
            return Err(EngineError::Invalid(entry.id.clone()));
        }
        return Ok((
            vec![
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
            vec!["flatpak".to_string(), "sudo".to_string()],
        ));
    }
    match platform.manager {
        Some(_) => Err(EngineError::UnsupportedEntry(entry.id.clone())),
        None => Err(EngineError::UnsupportedPlatform),
    }
}

/// Plan removing package entries. An entry is removed the way it is found
/// installed: its Flatpak, else the family's package, else on Arch the AUR
/// package with pacman. Entries that are not installed become notes.
pub fn plan_removal<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
) -> Result<Plan, EngineError> {
    let mut result = Plan::default();
    for entry in entries {
        let Action::Package {
            binary,
            packages,
            aur,
            flatpak,
            removable,
        } = &entry.action
        else {
            return Err(EngineError::NotRemovable(entry.id.clone()));
        };
        if !removable {
            return Err(EngineError::NotRemovable(entry.id.clone()));
        }
        let mut roots = vec![PathBuf::from("/var/lib/flatpak")];
        if let Some(home) = std::env::var_os("HOME") {
            roots.push(PathBuf::from(home).join(".local/share/flatpak"));
        }
        let (commands, requires) = if let Some(id) = flatpak
            .as_deref()
            .filter(|id| flatpak_installed_in(&roots, id))
        {
            (
                vec![words(vec!["sudo", "flatpak", "uninstall"], &[id])],
                vec!["flatpak".to_string(), "sudo".to_string()],
            )
        } else if !binary_exists(binary) {
            result
                .notes
                .push(format!("{}: not installed, nothing to remove", entry.name));
            continue;
        } else {
            let manager = platform.manager.ok_or(EngineError::UnsupportedPlatform)?;
            let package = packages.for_manager(manager).or(match manager {
                PackageManager::Arch => aur.as_deref(),
                _ => None,
            });
            let Some(package) = package else {
                result.notes.push(format!(
                    "{}: installed, but not by a package HelixForge knows; remove it the way it was installed",
                    entry.name
                ));
                continue;
            };
            if !valid_package_name(package) {
                return Err(EngineError::Invalid(entry.id.clone()));
            }
            (
                vec![words(manager.remove_command(), &[package])],
                vec![manager.executable().to_string(), "sudo".to_string()],
            )
        };
        result.privileged = true;
        result.risk = result.risk.max(Risk::Medium);
        result.requirements.extend(requires.iter().cloned());
        result.steps.push(Step::Package {
            id: entry.id.clone(),
            name: format!("Remove {}", entry.name),
            commands,
            requires,
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
            Step::Package { commands, .. } => commands
                .iter()
                .map(|command| quoted(command))
                .collect::<Vec<_>>()
                .join(" &&\n"),
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
        };
        text.push_str(&format!(
            "printf '\\n==> %s\\n' {}\n(\n{body}\n)\nrc=$?\n[ \"$rc\" -eq 0 ] || exit \"$rc\"\n",
            quote(step.name())
        ));
    }
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
pub fn check_requirements(plan: &Plan) -> Result<(), EngineError> {
    for requirement in &plan.requirements {
        let path = Path::new(requirement);
        let present = if path.is_absolute() {
            path.is_file()
        } else {
            binary_exists(requirement)
        };
        if !present {
            return Err(EngineError::MissingExecutable(requirement.clone()));
        }
    }
    Ok(())
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
) -> Result<Outcome, EngineError> {
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
) -> Result<Outcome, EngineError> {
    check_requirements(plan)?;
    info!(steps = ?plan.entries(), "run started");
    let outcome = runner.run(&shell_program(plan, lib_dir, this), lib_dir);
    info!(outcome = outcome.describe(), "run finished");
    if let Some(directory) = history_dir
        && let Err(error) = history::record_in(
            directory,
            &plan.entries(),
            outcome.succeeded(),
            &outcome.describe(),
            None,
        )
    {
        warn!(%error, "the run was not recorded");
        eprintln!(
            "HelixForge could not record this run in {}: {error}",
            directory.display()
        );
    }
    Ok(outcome)
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
        }
    }

    fn package_entry() -> Entry {
        entry(
            "sample",
            "I",
            Action::Package {
                binary: "helixforge-nonexistent-binary-12345".into(),
                packages: PackageNames {
                    arch: Some("sample".into()),
                    debian: Some("sample-deb".into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
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
            binary: "helixforge-nonexistent-binary-12345".into(),
            packages: PackageNames {
                suse: Some("sample-suse".into()),
                ..Default::default()
            },
            aur: Some("sample-bin".into()),
            flatpak: Some("org.example.Sample".into()),
            removable: false,
        };
        let suse = plan([&entry], &platform(Some(PackageManager::Suse)), &none).unwrap();
        assert_eq!(suse.steps[0].display(), "sudo zypper install sample-suse");
        let arch = plan([&entry], &platform(Some(PackageManager::Arch)), &none).unwrap();
        let shown = arch.steps[0].display();
        assert!(
            shown == "paru -S --needed sample-bin" || shown == "yay -S --needed sample-bin",
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
        let unknown = plan([&entry], &platform(None), &none).unwrap();
        assert!(unknown.steps[0].display().contains("flatpak install"));
        assert_eq!(aur_helper(&|name| name == "yay"), Some("yay"));
        assert_eq!(aur_helper(&|_| true), Some("paru"));
        assert_eq!(aur_helper(&|_| false), None);
    }

    #[test]
    fn removal_follows_how_the_package_is_installed() {
        let mut present = package_entry();
        present.action = Action::Package {
            binary: "sh".into(),
            packages: PackageNames {
                debian: Some("dash".into()),
                ..Default::default()
            },
            aur: Some("sh-bin".into()),
            flatpak: None,
            removable: true,
        };
        let debian = plan_removal([&present], &platform(Some(PackageManager::Debian))).unwrap();
        assert_eq!(debian.steps[0].display(), "sudo apt-get remove dash");
        assert_eq!(debian.steps[0].name(), "Remove SAMPLE");
        assert!(debian.privileged);
        assert_eq!(debian.risk, Risk::Medium);
        let arch = plan_removal([&present], &platform(Some(PackageManager::Arch))).unwrap();
        assert_eq!(arch.steps[0].display(), "sudo pacman -R sh-bin");
        let fedora = plan_removal([&present], &platform(Some(PackageManager::Fedora))).unwrap();
        assert!(fedora.steps.is_empty());
        assert!(fedora.notes[0].contains("not by a package"));
        let absent =
            plan_removal([&package_entry()], &platform(Some(PackageManager::Arch))).unwrap();
        assert!(absent.steps.is_empty());
        assert!(absent.notes[0].contains("not installed"));
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
                binary: "sh".into(),
                packages: PackageNames {
                    arch: Some("sh".into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
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
        assert_eq!(outcome, Outcome::Failed { exit_code: 4 });
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
