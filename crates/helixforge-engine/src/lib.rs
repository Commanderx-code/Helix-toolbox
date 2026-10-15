//! Turns entries into steps, runs them, and records what happened.
//!
//! A package entry becomes one native package-manager command. A script entry
//! runs from its own directory with its interpreter. Either way the steps are
//! joined into one POSIX shell program that stops at the first failure, which
//! runs in a pseudo-terminal inside the interface or in the caller's terminal
//! from the command line. The shell only ever sees quoted literals from the
//! validated catalog, never free text.

pub mod history;
pub mod pty;

use helixforge_core::{Action, Entry, PackageManager, valid_package_name};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::{
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};
use thiserror::Error;

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
    #[error("required executable not found: {0}")]
    MissingExecutable(String),
    #[error("cannot start the shell: {0}")]
    Spawn(#[source] std::io::Error),
}

/// One thing to run, with everything needed to run it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Step {
    Package {
        id: String,
        name: String,
        manager: PackageManager,
        package: String,
    },
    Script {
        id: String,
        name: String,
        file: PathBuf,
        program: String,
        args: Vec<String>,
    },
    Command {
        id: String,
        name: String,
        line: String,
    },
}

impl Step {
    pub fn id(&self) -> &str {
        match self {
            Step::Package { id, .. } | Step::Script { id, .. } | Step::Command { id, .. } => id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Step::Package { name, .. } | Step::Script { name, .. } | Step::Command { name, .. } => {
                name
            }
        }
    }

    /// The command line as the shell will run it, for previews and plans.
    pub fn display(&self) -> String {
        match self {
            Step::Package {
                manager, package, ..
            } => package_command(*manager, package).join(" "),
            Step::Script {
                file,
                program,
                args,
                ..
            } => {
                let mut words = vec![program.clone()];
                words.extend(args.iter().cloned());
                words.push(file.to_string_lossy().into_owned());
                format!(
                    "cd {} && {}",
                    file.parent().unwrap_or(Path::new(".")).display(),
                    words.join(" ")
                )
            }
            Step::Command { line, .. } => line.clone(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Planned {
    Ready(Step),
    /// A package entry whose executable is already on the PATH.
    AlreadyInstalled {
        binary: String,
    },
}

/// Work out what running an entry means on this machine.
pub fn plan(entry: &Entry, platform: &LinuxInfo) -> Result<Planned, EngineError> {
    match &entry.action {
        Action::Package { binary, packages } => {
            let manager = platform.manager.ok_or(EngineError::UnsupportedPlatform)?;
            let package = packages
                .for_manager(manager)
                .ok_or_else(|| EngineError::UnsupportedEntry(entry.id.clone()))?;
            if !valid_package_name(package) || !valid_package_name(binary) {
                return Err(EngineError::Invalid(entry.id.clone()));
            }
            if binary_exists(binary) {
                return Ok(Planned::AlreadyInstalled {
                    binary: binary.clone(),
                });
            }
            Ok(Planned::Ready(Step::Package {
                id: entry.id.clone(),
                name: entry.name.clone(),
                manager,
                package: package.to_string(),
            }))
        }
        Action::Script { file } => {
            let (program, args) = helixforge_catalog::interpreter(file);
            Ok(Planned::Ready(Step::Script {
                id: entry.id.clone(),
                name: entry.name.clone(),
                file: file.clone(),
                program,
                args,
            }))
        }
        Action::Command { line } => Ok(Planned::Ready(Step::Command {
            id: entry.id.clone(),
            name: entry.name.clone(),
            line: line.clone(),
        })),
    }
}

fn package_command(manager: PackageManager, package: &str) -> Vec<String> {
    let mut words: Vec<String> = match manager {
        PackageManager::Arch => vec![
            "sudo".into(),
            "pacman".into(),
            "-S".into(),
            "--needed".into(),
        ],
        PackageManager::Debian => vec!["sudo".into(), "apt-get".into(), "install".into()],
        PackageManager::Fedora => vec!["sudo".into(), "dnf".into(), "install".into()],
    };
    words.push(package.to_string());
    words
}

/// Quote a value for the POSIX shell.
pub fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The shell program that runs the steps in order and stops at the first
/// failure. `lib_dir` is exported as `HELIXFORGE_LIB` for scripts.
pub fn shell_program(steps: &[Step], lib_dir: &Path) -> String {
    let mut text = format!(
        "{LIB_VARIABLE}={}\nexport {LIB_VARIABLE}\n",
        quote(&lib_dir.to_string_lossy())
    );
    for step in steps {
        let body = match step {
            Step::Package {
                manager, package, ..
            } => package_command(*manager, package)
                .iter()
                .map(|word| quote(word))
                .collect::<Vec<_>>()
                .join(" "),
            Step::Script {
                file,
                program,
                args,
                ..
            } => {
                let directory = file.parent().unwrap_or(Path::new("."));
                let mut words = vec![quote(program)];
                words.extend(args.iter().map(|arg| quote(arg)));
                words.push(quote(&file.to_string_lossy()));
                format!(
                    "cd {} || exit 1\n{}",
                    quote(&directory.to_string_lossy()),
                    words.join(" ")
                )
            }
            Step::Command { line, .. } => format!("sh -e -c {}", quote(line)),
        };
        text.push_str(&format!(
            "(\n{body}\n)\nrc=$?\n[ \"$rc\" -eq 0 ] || exit \"$rc\"\n"
        ));
    }
    text
}

/// Check that what the steps need is installed before anything starts.
pub fn check_requirements(steps: &[Step]) -> Result<(), EngineError> {
    for step in steps {
        if let Step::Package { manager, .. } = step {
            if !binary_exists(manager.executable()) {
                return Err(EngineError::MissingExecutable(
                    manager.executable().to_string(),
                ));
            }
            if !binary_exists("sudo") {
                return Err(EngineError::MissingExecutable("sudo".to_string()));
            }
        }
    }
    Ok(())
}

/// Run the steps in the caller's terminal, inheriting stdin and stdout, for
/// the command line.
pub fn run_inherited(steps: &[Step], lib_dir: &Path) -> Result<ExitStatus, EngineError> {
    check_requirements(steps)?;
    Command::new("sh")
        .arg("-c")
        .arg(shell_program(steps, lib_dir))
        .env(LIB_VARIABLE, lib_dir)
        .status()
        .map_err(EngineError::Spawn)
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

    fn package_entry() -> Entry {
        Entry {
            id: "sample".into(),
            name: "Sample".into(),
            description: String::new(),
            flags: "I".into(),
            multi_select: true,
            action: Action::Package {
                binary: "helixforge-nonexistent-binary-12345".into(),
                packages: PackageNames {
                    arch: Some("sample".into()),
                    debian: Some("sample-deb".into()),
                    fedora: None,
                },
            },
        }
    }

    #[test]
    fn package_plans_use_the_native_manager() {
        let Planned::Ready(step) =
            plan(&package_entry(), &platform(Some(PackageManager::Arch))).unwrap()
        else {
            panic!("expected a step");
        };
        assert_eq!(step.display(), "sudo pacman -S --needed sample");
        let Planned::Ready(step) =
            plan(&package_entry(), &platform(Some(PackageManager::Debian))).unwrap()
        else {
            panic!("expected a step");
        };
        assert_eq!(step.display(), "sudo apt-get install sample-deb");
        assert!(matches!(
            plan(&package_entry(), &platform(Some(PackageManager::Fedora))),
            Err(EngineError::UnsupportedEntry(_))
        ));
        assert!(matches!(
            plan(&package_entry(), &platform(None)),
            Err(EngineError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn installed_packages_are_reported_not_planned() {
        let mut entry = package_entry();
        entry.action = Action::Package {
            binary: "sh".into(),
            packages: PackageNames {
                arch: Some("sh".into()),
                ..Default::default()
            },
        };
        assert!(matches!(
            plan(&entry, &platform(Some(PackageManager::Arch))).unwrap(),
            Planned::AlreadyInstalled { .. }
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
        let steps = vec![
            Step::Script {
                id: "s".into(),
                name: "S".into(),
                file: script,
                program: "/bin/sh".into(),
                args: vec!["-e".into()],
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
        ];
        let program = shell_program(&steps, Path::new("/tmp/lib dir"));
        let output = Command::new("sh").arg("-c").arg(&program).output().unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(7));
        assert!(text.contains("lib=/tmp/lib dir"));
        assert!(text.contains(&directory.path().to_string_lossy().to_string()));
        assert!(text.contains("before"));
        assert!(!text.contains("never"));
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
