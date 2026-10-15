//! Shared installation planning and execution. No shell interpolation.

use helixforge_core::{PackageManager, Tool, valid_package_name};
use helixforge_platform::{LinuxInfo, binary_exists};
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("this distribution is not yet supported for installations")]
    UnsupportedPlatform,
    #[error("{0} has no package mapping for this distro family")]
    UnsupportedTool(String),
    #[error("unsafe or unsupported catalog entry: {0}")]
    InvalidPackage(String),
    #[error("required executable not found: {0}")]
    MissingExecutable(String),
    #[error("failed to execute {0}: {1}")]
    Execute(String, #[source] std::io::Error),
    #[error("{0} exited unsuccessfully: {1}")]
    NonZeroExit(String, ExitStatus),
    #[error("could not record execution history: {0}")]
    Record(#[from] std::io::Error),
    #[error("could not encode execution history: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("HOME is unset and XDG_STATE_HOME is unavailable")]
    MissingStateDirectory,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

impl CommandSpec {
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Plan {
    pub tool_id: String,
    pub tool_name: String,
    pub package: String,
    pub manager: PackageManager,
    pub command: CommandSpec,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum PlanResult {
    AlreadyDetected { binary: String },
    Ready(Plan),
}

pub fn plan_install(tool: &Tool, platform: &LinuxInfo) -> Result<PlanResult, EngineError> {
    if tool.install.kind != "package" || !valid_package_name(&tool.detect.binary) {
        return Err(EngineError::InvalidPackage(tool.id.clone()));
    }
    let manager = platform.manager.ok_or(EngineError::UnsupportedPlatform)?;
    let package = tool
        .install
        .packages
        .for_manager(manager)
        .ok_or_else(|| EngineError::UnsupportedTool(tool.id.clone()))?;
    if !valid_package_name(package) {
        return Err(EngineError::InvalidPackage(package.into()));
    }
    if binary_exists(&tool.detect.binary) {
        return Ok(PlanResult::AlreadyDetected {
            binary: tool.detect.binary.clone(),
        });
    }
    // No sh -c, no eval, no scripted privilege escalation.
    let mut args = match manager {
        PackageManager::Arch => vec!["pacman".into(), "-S".into(), "--needed".into()],
        PackageManager::Debian => vec!["apt-get".into(), "install".into()],
        PackageManager::Fedora => vec!["dnf".into(), "install".into()],
    };
    args.push(package.to_string());
    Ok(PlanResult::Ready(Plan {
        tool_id: tool.id.clone(),
        tool_name: tool.name.clone(),
        package: package.into(),
        manager,
        command: CommandSpec {
            program: "sudo".into(),
            args,
        },
    }))
}

pub trait Runner {
    fn run(&self, command: &CommandSpec) -> Result<ExitStatus, std::io::Error>;
}

pub struct ProcessRunner;

impl Runner for ProcessRunner {
    fn run(&self, command: &CommandSpec) -> Result<ExitStatus, std::io::Error> {
        Command::new(&command.program).args(&command.args).status() // Inherits terminal stdin/stdout/stderr for sudo and package prompts.
    }
}

pub fn execute_with<R: Runner>(plan: &Plan, runner: &R) -> Result<(), EngineError> {
    if !binary_exists(plan.manager.executable()) {
        return Err(EngineError::MissingExecutable(
            plan.manager.executable().to_owned(),
        ));
    }
    if !binary_exists("sudo") {
        return Err(EngineError::MissingExecutable("sudo".to_owned()));
    }
    let result = runner
        .run(&plan.command)
        .map_err(|e| EngineError::Execute(plan.command.program.clone(), e));
    let (success, outcome) = match &result {
        Ok(status) => (status.success(), format!("exit: {status}")),
        Err(e) => (false, e.to_string()),
    };
    // Always try to record the attempt, even when the subprocess fails.
    if let Err(e) = record_attempt(plan, success, &outcome) {
        eprintln!("warning: could not write HELIXFORGE history: {e}");
    }
    let status = result?;
    if !status.success() {
        return Err(EngineError::NonZeroExit(
            plan.command.program.clone(),
            status,
        ));
    }
    Ok(())
}

pub fn execute(plan: &Plan) -> Result<(), EngineError> {
    execute_with(plan, &ProcessRunner)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryRecord {
    pub unix_timestamp: u64,
    pub tool_id: String,
    pub command: String,
    pub success: bool,
    pub result: String,
}

pub fn history_file() -> Result<PathBuf, EngineError> {
    if let Some(dir) = env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(dir);
        if path.is_absolute() {
            return Ok(path.join("helixforge/history.jsonl"));
        }
    }
    let home = env::var_os("HOME").ok_or(EngineError::MissingStateDirectory)?;
    Ok(PathBuf::from(home).join(".local/state/helixforge/history.jsonl"))
}

fn record_attempt(plan: &Plan, success: bool, result: &str) -> Result<(), EngineError> {
    let path = history_file()?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let item = HistoryRecord {
        unix_timestamp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        tool_id: plan.tool_id.clone(),
        command: plan.command.display(),
        success,
        result: result.to_owned(),
    };
    let mut f = OpenOptions::new().append(true).create(true).open(path)?;
    serde_json::to_writer(&mut f, &item)?;
    writeln!(f)?;
    Ok(())
}

pub fn read_history(limit: usize) -> Result<Vec<HistoryRecord>, EngineError> {
    let path = history_file()?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(path)?;
    let mut records: Vec<HistoryRecord> = data
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    records.reverse();
    records.truncate(limit);
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;
    use helixforge_core::{Detect, Install, PackageNames};

    fn sample_tool() -> Tool {
        Tool {
            id: "sample".into(),
            name: "Sample".into(),
            description: "Test".into(),
            category: "Test".into(),
            detect: Detect {
                binary: "helixforge-nonexistent-binary-12345".into(),
            },
            install: Install {
                kind: "package".into(),
                packages: PackageNames {
                    arch: Some("sample".into()),
                    debian: Some("sample-deb".into()),
                    fedora: Some("sample-rpm".into()),
                },
            },
        }
    }

    fn platform(manager: PackageManager) -> LinuxInfo {
        LinuxInfo {
            id: "test".into(),
            pretty_name: "Test Linux".into(),
            id_like: vec![],
            manager: Some(manager),
        }
    }

    #[test]
    fn arch_plan_is_structured() {
        let plan = plan_install(&sample_tool(), &platform(PackageManager::Arch)).unwrap();
        let PlanResult::Ready(plan) = plan else {
            panic!("expected ready");
        };
        assert_eq!(plan.command.program, "sudo");
        assert_eq!(plan.command.args, ["pacman", "-S", "--needed", "sample"]);
    }

    #[test]
    fn apt_and_dnf_use_native_managers() {
        for (manager, expected) in [
            (
                PackageManager::Debian,
                vec!["apt-get", "install", "sample-deb"],
            ),
            (PackageManager::Fedora, vec!["dnf", "install", "sample-rpm"]),
        ] {
            let PlanResult::Ready(plan) = plan_install(&sample_tool(), &platform(manager)).unwrap()
            else {
                panic!("expected ready");
            };
            assert_eq!(plan.command.args, expected);
        }
    }

    #[test]
    fn untrusted_package_flags_are_rejected() {
        let mut tool = sample_tool();
        tool.install.packages.arch = Some("--overwrite".into());
        assert!(matches!(
            plan_install(&tool, &platform(PackageManager::Arch)),
            Err(EngineError::InvalidPackage(_))
        ));
    }

    #[test]
    fn unsupported_os_cannot_plan_install() {
        let mut info = platform(PackageManager::Arch);
        info.manager = None;
        assert!(matches!(
            plan_install(&sample_tool(), &info),
            Err(EngineError::UnsupportedPlatform)
        ));
    }
}
