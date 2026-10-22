//! Project workspaces: the tasks a repository offers, found in its manifests
//! (`package.json` scripts, Cargo, a Makefile, a justfile) without running
//! anything, and the user's own, with favorites and groups, kept in
//! `$XDG_CONFIG_HOME/helix-toolbox/workspaces.toml`. Running a task is a
//! plan, reviewed first, run in the repository's folder.

use crate::{Plan, Step, quote};
use helix_toolbox_core::{Risk, printable};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// How a task's command is run.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Shell {
    /// Split into words and run as it is, with no shell.
    #[default]
    Direct,
    Fish,
    Bash,
}

/// One task of a project.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub shell: Shell,
    /// Found in a manifest, not saved by the user; never written to the file.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detected: bool,
}

/// What the user keeps about one repository.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub path: PathBuf,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, rename = "task", skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<Task>,
}

/// `workspaces.toml`.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Workspaces {
    #[serde(default, rename = "project")]
    pub projects: Vec<Project>,
}

impl Workspaces {
    pub fn project(&self, repo: &Path) -> Option<&Project> {
        self.projects.iter().find(|project| project.path == repo)
    }

    fn project_mut(&mut self, repo: &Path) -> &mut Project {
        if let Some(index) = self
            .projects
            .iter()
            .position(|project| project.path == repo)
        {
            return &mut self.projects[index];
        }
        self.projects.push(Project {
            path: repo.to_path_buf(),
            ..Project::default()
        });
        self.projects.last_mut().unwrap()
    }

    pub fn favorite(&self, repo: &Path) -> bool {
        self.project(repo).is_some_and(|project| project.favorite)
    }

    pub fn set_favorite(&mut self, repo: &Path, favorite: bool) {
        self.project_mut(repo).favorite = favorite;
        self.tidy();
    }

    pub fn set_group(&mut self, repo: &Path, group: &str) -> Result<(), String> {
        let group = group.trim();
        if group.chars().count() > 60 || group.chars().any(char::is_control) {
            return Err("a group name is up to 60 characters".to_string());
        }
        self.project_mut(repo).group = group.to_string();
        self.tidy();
        Ok(())
    }

    /// Add a task of the user's, or replace theirs of the same id.
    pub fn add_task(&mut self, repo: &Path, task: Task) -> Result<(), String> {
        check(&task)?;
        let project = self.project_mut(repo);
        project.tasks.retain(|other| other.id != task.id);
        if project.tasks.len() >= 30 {
            return Err("a project keeps up to 30 tasks".to_string());
        }
        project.tasks.push(Task {
            detected: false,
            ..task
        });
        Ok(())
    }

    pub fn remove_task(&mut self, repo: &Path, id: &str) -> Result<(), String> {
        let project = self.project_mut(repo);
        let before = project.tasks.len();
        project.tasks.retain(|task| task.id != id);
        if project.tasks.len() == before {
            return Err(format!("no task {id} of yours there"));
        }
        self.tidy();
        Ok(())
    }

    /// Projects with nothing kept are left out of the file.
    fn tidy(&mut self) {
        self.projects.retain(|project| {
            project.favorite || !project.group.is_empty() || !project.tasks.is_empty()
        });
    }
}

/// `$XDG_CONFIG_HOME/helix-toolbox/workspaces.toml`, beside `settings.toml`.
pub fn path() -> Option<PathBuf> {
    Some(crate::settings::path()?.with_file_name("workspaces.toml"))
}

/// The saved workspaces; none when the file is missing.
pub fn load(path: &Path) -> Result<Workspaces, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text)
            .map_err(|error| format!("{} cannot be read: {}", path.display(), error.message())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Workspaces::default()),
        Err(error) => Err(format!("{} cannot be read: {error}", path.display())),
    }
}

/// Write them beside the file first, then renamed.
pub fn save(path: &Path, workspaces: &Workspaces) -> Result<(), String> {
    let failed = |error: String| format!("cannot write {}: {error}", path.display());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| failed(error.to_string()))?;
    }
    let text = toml::to_string(workspaces).map_err(|error| failed(error.to_string()))?;
    let staging = path.with_extension("toml.new");
    std::fs::write(
        &staging,
        format!(
            "# Helix Toolbox project workspaces: favorites, groups and your own tasks.\n{text}"
        ),
    )
    .map_err(|error| failed(error.to_string()))?;
    std::fs::rename(&staging, path).map_err(|error| failed(error.to_string()))
}

/// A task's id, name and command as Helix Toolbox takes them.
fn check(task: &Task) -> Result<(), String> {
    if task.id.is_empty()
        || task.id.len() > 80
        || !task
            .id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err("a task's id is letters, digits, - and _, up to 80".to_string());
    }
    if task.name.trim().is_empty()
        || task.name.chars().count() > 80
        || task.name.chars().any(char::is_control)
    {
        return Err("a task's name is 1 to 80 characters".to_string());
    }
    if task.command.trim().is_empty() || task.command.len() > 4096 || task.command.contains('\0') {
        return Err("a task's command is 1 to 4096 bytes".to_string());
    }
    if task.shell == Shell::Direct {
        split(&task.command)?;
    }
    Ok(())
}

/// The tasks the repository's manifests name, read only: never run.
pub fn detect(repo: &Path) -> Vec<Task> {
    let mut found: Vec<Task> = Vec::new();
    let mut add = |id: String, name: String, command: String| {
        found.push(Task {
            id,
            name,
            command,
            shell: Shell::Direct,
            detected: true,
        });
    };
    let small = |file: &str| -> Option<String> {
        let path = repo.join(file);
        let meta = std::fs::metadata(&path).ok()?;
        (meta.is_file() && meta.len() <= 1 << 20).then(|| std::fs::read_to_string(path).ok())?
    };
    if let Some(text) = small("package.json")
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
        && let Some(scripts) = value.get("scripts").and_then(|scripts| scripts.as_object())
    {
        for script in ["build", "test", "dev", "lint", "check", "start"] {
            if scripts.get(script).is_some_and(|value| value.is_string()) {
                add(
                    format!("npm-{script}"),
                    format!("npm run {script}"),
                    format!("npm run {script}"),
                );
            }
        }
    }
    if repo.join("Cargo.toml").is_file() {
        for command in ["build", "test", "check", "run"] {
            add(
                format!("cargo-{command}"),
                format!("cargo {command}"),
                format!("cargo {command}"),
            );
        }
    }
    let makefile = ["GNUmakefile", "makefile", "Makefile"]
        .into_iter()
        .find_map(small);
    if let Some(text) = makefile {
        for target in ["all", "build", "test", "check", "lint"] {
            if recipe(&text, target, false) {
                add(
                    format!("make-{target}"),
                    format!("make {target}"),
                    format!("make {target}"),
                );
            }
        }
    }
    let justfile = ["justfile", "Justfile", ".justfile"]
        .into_iter()
        .find_map(small);
    if let Some(text) = justfile {
        for recipe_name in ["build", "test", "check", "lint", "run", "dev"] {
            if recipe(&text, recipe_name, true) {
                add(
                    format!("just-{recipe_name}"),
                    format!("just {recipe_name}"),
                    format!("just {recipe_name}"),
                );
            }
        }
    }
    found
}

/// Whether a Makefile (or a justfile, whose recipes take parameters before
/// the colon) has a rule for this target at the start of a line.
fn recipe(text: &str, target: &str, parameters: bool) -> bool {
    text.lines().any(|line| {
        let Some(rest) = line.strip_prefix(target) else {
            return false;
        };
        let rest = if parameters {
            rest.split_once(':')
                .filter(|(before, _)| before.is_empty() || before.starts_with(' '))
                .map(|(_, after)| after)
        } else {
            rest.trim_start_matches(' ').strip_prefix(':')
        };
        rest.is_some_and(|after| !after.starts_with('='))
    })
}

/// The repository's tasks: the user's first, then those found that the
/// user's do not replace.
pub fn tasks(repo: &Path, workspaces: &Workspaces) -> Vec<Task> {
    let mut all: Vec<Task> = workspaces
        .project(repo)
        .map(|project| project.tasks.clone())
        .unwrap_or_default();
    for task in detect(repo) {
        if !all.iter().any(|other| other.id == task.id) {
            all.push(task);
        }
    }
    all
}

/// A command split into words as a shell would: spaces separate them,
/// quotes keep them together, a backslash takes the next character as it is.
pub fn split(command: &str) -> Result<Vec<String>, String> {
    let mut words: Vec<String> = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut characters = command.chars();
    while let Some(character) = characters.next() {
        match character {
            ' ' | '\t' | '\n' => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('\'') => break,
                        Some(other) => word.push(other),
                        None => return Err("a quote is not closed".to_string()),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match characters.next() {
                        Some('"') => break,
                        Some('\\') => match characters.next() {
                            Some(other @ ('"' | '\\' | '$' | '`')) => word.push(other),
                            Some(other) => {
                                word.push('\\');
                                word.push(other);
                            }
                            None => return Err("a quote is not closed".to_string()),
                        },
                        Some(other) => word.push(other),
                        None => return Err("a quote is not closed".to_string()),
                    }
                }
            }
            '\\' => {
                started = true;
                match characters.next() {
                    Some(other) => word.push(other),
                    None => return Err("the command ends in a backslash".to_string()),
                }
            }
            other => {
                started = true;
                word.push(other);
            }
        }
    }
    if started {
        words.push(word);
    }
    if words.is_empty() {
        return Err("the command is empty".to_string());
    }
    Ok(words)
}

/// The plan that runs a task in the repository's folder.
pub fn plan_task(repo: &Path, task: &Task) -> Result<Plan, String> {
    check(task)?;
    let (line, requires) = match task.shell {
        Shell::Direct => {
            let words = split(&task.command)?;
            (
                format!(
                    "exec {}",
                    words
                        .iter()
                        .map(|word| quote(word))
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
                words[0].clone(),
            )
        }
        Shell::Fish => (
            format!("exec fish -c {}", quote(&task.command)),
            "fish".to_string(),
        ),
        Shell::Bash => (
            format!("exec bash -c {}", quote(&task.command)),
            "bash".to_string(),
        ),
    };
    let shown_repo = repo.display().to_string();
    let name = repo
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| shown_repo.clone());
    Ok(Plan {
        steps: vec![Step::Shell {
            id: format!("task-{}", task.id),
            name: printable(&format!("{} in {name}", task.name)),
            program: format!("cd -- {} || exit 1\n{line}", quote(&shown_repo)),
            display: printable(&format!(
                "cd {shown_repo} && {}{}",
                match task.shell {
                    Shell::Direct => "",
                    Shell::Fish => "fish -c ",
                    Shell::Bash => "bash -c ",
                },
                task.command
            )),
            requires: vec![requires.clone()],
            changes: Vec::new(),
        }],
        notes: vec![format!(
            "It runs the project's own {}: what it does is up to the project, and a script may run others before and after it.",
            if task.detected { "script" } else { "command" }
        )],
        requirements: vec![requires],
        privileged: false,
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tasks_are_found_in_the_manifests_without_running_anything() {
        let repo = tempfile::tempdir().unwrap();
        std::fs::write(
            repo.path().join("package.json"),
            r#"{"scripts": {"build": "vite build", "test": "vitest", "deploy": "rm -rf /", "dev": 1}}"#,
        )
        .unwrap();
        std::fs::write(repo.path().join("Cargo.toml"), "[package]\nname = \"x\"\n").unwrap();
        std::fs::write(
            repo.path().join("Makefile"),
            "CC := gcc\nall: build\nbuild:\n\tcc x.c\ncheck : all\ntest:=x\n",
        )
        .unwrap();
        std::fs::write(
            repo.path().join("justfile"),
            "lint target='.':\n  ruff\ntest:\n  pytest\nbuild := 'x'\n",
        )
        .unwrap();
        let ids: Vec<String> = detect(repo.path())
            .into_iter()
            .map(|task| task.id)
            .collect();
        assert_eq!(
            ids,
            [
                "npm-build",
                "npm-test",
                "cargo-build",
                "cargo-test",
                "cargo-check",
                "cargo-run",
                "make-all",
                "make-build",
                "make-check",
                "just-test",
                "just-lint"
            ]
        );
    }

    #[test]
    fn your_tasks_come_first_are_kept_and_run_in_the_folder() {
        let folder = tempfile::tempdir().unwrap();
        let repo = folder.path().join("project");
        std::fs::create_dir(&repo).unwrap();
        std::fs::write(repo.join("Cargo.toml"), "").unwrap();
        let file = folder.path().join("config/workspaces.toml");
        let mut kept = load(&file).unwrap();
        assert!(
            kept.add_task(
                &repo,
                Task {
                    id: "bad id".into(),
                    name: "x".into(),
                    command: "x".into(),
                    shell: Shell::Direct,
                    detected: false,
                },
            )
            .is_err()
        );
        kept.add_task(
            &repo,
            Task {
                id: "cargo-test".into(),
                name: "Tests, quietly".into(),
                command: "cargo test -q -- \"with space\"".into(),
                shell: Shell::Direct,
                detected: false,
            },
        )
        .unwrap();
        kept.set_favorite(&repo, true);
        save(&file, &kept).unwrap();
        let read = load(&file).unwrap();
        assert_eq!(read, kept);
        assert!(read.favorite(&repo));
        let all = tasks(&repo, &read);
        assert_eq!(all[0].name, "Tests, quietly");
        assert_eq!(all.iter().filter(|task| task.id == "cargo-test").count(), 1);
        let planned = plan_task(&repo, &all[0]).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!()
        };
        assert!(
            program.ends_with("exec 'cargo' 'test' '-q' '--' 'with space'"),
            "{program}"
        );
        assert_eq!(planned.requirements, ["cargo"]);
        // Nothing kept: the project leaves the file.
        let mut emptied = read.clone();
        emptied.remove_task(&repo, "cargo-test").unwrap();
        emptied.set_favorite(&repo, false);
        assert!(emptied.projects.is_empty());
    }

    #[test]
    fn commands_split_as_a_shell_would() {
        assert_eq!(split("npm run  dev").unwrap(), ["npm", "run", "dev"]);
        assert_eq!(
            split(r#"echo "a \"b\"" 'c d' e\ f"#).unwrap(),
            ["echo", "a \"b\"", "c d", "e f"]
        );
        assert_eq!(split("x ''").unwrap(), ["x", ""]);
        assert!(split("echo 'open").is_err());
        assert!(split("   ").is_err());
    }
}
