//! `helix-toolbox repos`: the Git repositories dashboard and project
//! workspaces on the command line (helix_toolbox_engine::repositories and
//! workspaces). Listing, showing and diffs only read; every change is a
//! plan, confirmed like any other.

use anyhow::{Context, Result, anyhow, bail};
use clap::{Args, Subcommand};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::printable;
use helix_toolbox_engine::{
    repositories::{self, Change, Details, Here, Summary},
    settings,
    workspaces::{self, Shell, Task, Workspaces},
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;
use std::path::PathBuf;

#[derive(Args, Clone, Copy, Default)]
pub struct Confirm {
    /// Show the plan and stop.
    #[arg(long)]
    dry_run: bool,
    /// Skip the confirmation phrase.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand)]
pub enum ReposCommand {
    /// The repositories found, with their branch, changes and upstream
    /// (the default).
    List,
    /// One repository: its files, branches, stashes, recent commits and
    /// tasks. <repo> is its name, or its path when names repeat.
    Show { repo: String },
    /// What changed: the whole repository, or one file; --staged for what
    /// is staged.
    Diff {
        repo: String,
        file: Option<String>,
        #[arg(long)]
        staged: bool,
    },
    /// Stage changed files, or every change with --all.
    Stage {
        repo: String,
        files: Vec<String>,
        #[arg(long)]
        all: bool,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Take files out of what is staged; their changes stay.
    Unstage {
        repo: String,
        #[arg(required = true)]
        files: Vec<String>,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Commit what is staged.
    Commit {
        repo: String,
        #[arg(short, long)]
        message: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Switch to a branch, or create one and switch to it with --create;
    /// the repository must have no changes.
    Branch {
        repo: String,
        name: String,
        #[arg(long)]
        create: bool,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Put the changes aside in a stash.
    Stash {
        repo: String,
        #[arg(short, long, default_value = "")]
        message: String,
        /// Untracked files too.
        #[arg(long)]
        untracked: bool,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Apply a stash (stash@{n}, or its commit), keeping it.
    StashApply {
        repo: String,
        stash: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Fetch every remote, pruning branches gone there.
    Fetch {
        repo: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Bring in the upstream's commits, fast-forward only.
    Pull {
        repo: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Push the branch to its upstream, never forcing.
    Push {
        repo: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Push the branch to a remote and track it there.
    Publish {
        repo: String,
        #[arg(default_value = "origin")]
        remote: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// The repository's tasks: yours, and those its manifests name.
    Tasks { repo: String },
    /// Run one of its tasks in its folder.
    Task {
        repo: String,
        task: String,
        #[command(flatten)]
        confirm: Confirm,
    },
    /// Keep a task of your own for the repository (replacing one of the
    /// same id).
    TaskAdd {
        repo: String,
        id: String,
        command: String,
        #[arg(long)]
        name: Option<String>,
        /// Run it with fish or bash instead of as it is.
        #[arg(long, value_enum, default_value = "direct")]
        shell: ShellChoice,
    },
    /// Remove a task of your own.
    TaskRemove { repo: String, id: String },
    /// Mark the repository a favorite, listed first, or not with --off.
    Favorite {
        repo: String,
        #[arg(long)]
        off: bool,
    },
    /// Put the repository in a group, or out of one with "".
    Group { repo: String, group: String },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum ShellChoice {
    Direct,
    Fish,
    Bash,
}

/// The repositories found, by the places in the settings.
pub fn found() -> (repositories::Places, Vec<PathBuf>) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let places = repositories::places(&settings::path().unwrap_or_default(), &home);
    let repos = repositories::discover(&places);
    (places, repos)
}

fn kept() -> Result<(PathBuf, Workspaces)> {
    let path = workspaces::path().context("cannot find the configuration folder")?;
    let kept = workspaces::load(&path).map_err(|error| anyhow!(error))?;
    Ok((path, kept))
}

/// Favorites first, then by name.
pub fn ordered(mut summaries: Vec<Summary>, kept: &Workspaces) -> Vec<Summary> {
    summaries.sort_by_key(|summary| !kept.favorite(&summary.path));
    summaries
}

/// One summary as JSON, with what the user keeps about it.
pub fn summary_json(summary: &Summary, kept: &Workspaces) -> serde_json::Value {
    let mut value = json!(summary);
    let project = kept.project(&summary.path);
    value["favorite"] = json!(project.is_some_and(|project| project.favorite));
    value["group"] = json!(
        project
            .map(|project| project.group.clone())
            .unwrap_or_default()
    );
    value
}

/// How a repository stands against its upstream, in a few words.
pub fn sync_words(summary: &Summary) -> String {
    match (&summary.upstream, summary.ahead, summary.behind) {
        (None, ..) => "no upstream".to_string(),
        (Some(_), Some(0), Some(0)) => "up to date".to_string(),
        (Some(_), ahead, behind) => {
            let mut parts = Vec::new();
            if let Some(ahead) = ahead.filter(|count| *count > 0) {
                parts.push(format!("{ahead} ahead"));
            }
            if let Some(behind) = behind.filter(|count| *count > 0) {
                parts.push(format!("{behind} behind"));
            }
            parts.join(", ")
        }
    }
}

pub fn repos(
    command: Option<ReposCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let (places, repos) = found();
    let pick = |wanted: &str| -> Result<PathBuf> {
        repositories::find(&repos, wanted)
            .cloned()
            .map_err(|error| anyhow!(error))
    };
    let read = |wanted: &str| -> Result<Details> {
        repositories::details(&Here, &pick(wanted)?).map_err(|error| anyhow!(error))
    };
    let change = |wanted: &str, change: Change, confirm: Confirm| -> Result<()> {
        let found = read(wanted)?;
        let planned = repositories::plan(&Here, &found, &change).map_err(|error| anyhow!(error))?;
        crate::confirm_and_run(
            catalog,
            platform,
            &planned,
            confirm.dry_run,
            confirm.yes,
            "GIT",
        )
    };
    match command.unwrap_or(ReposCommand::List) {
        ReposCommand::List => {
            let (_, kept) = kept()?;
            let summaries = ordered(repositories::summaries(&Here, &repos), &kept);
            if as_json {
                crate::json::print(
                    "repositories",
                    json!({
                        "roots": places.roots,
                        "depth": places.depth,
                        "origin": places.origin,
                        "repositories": summaries.iter().map(|summary| summary_json(summary, &kept)).collect::<Vec<_>>(),
                    }),
                );
                return Ok(());
            }
            if summaries.is_empty() {
                println!(
                    "No repositories under {} (depth {}). Set [repositories] roots in {} to look elsewhere.",
                    shown_roots(&places.roots),
                    places.depth,
                    settings::path().unwrap_or_default().display()
                );
                return Ok(());
            }
            for summary in &summaries {
                println!(
                    "{}{:<24} {:<18} {:<11} {:<18} {}",
                    if kept.favorite(&summary.path) {
                        "* "
                    } else {
                        "  "
                    },
                    printable(&summary.name),
                    printable(summary.branch.as_deref().unwrap_or("(detached)")),
                    match summary.changed {
                        0 => "clean".to_string(),
                        count => format!("{count} changed"),
                    },
                    sync_words(summary),
                    summary
                        .problem
                        .as_deref()
                        .or(summary.last.as_deref())
                        .map(printable)
                        .unwrap_or_default()
                );
            }
            println!();
            println!(
                "{} repositories under {} (depth {}, from the {}).",
                summaries.len(),
                shown_roots(&places.roots),
                places.depth,
                places.origin
            );
            Ok(())
        }
        ReposCommand::Show { repo } => {
            let found = read(&repo)?;
            let (_, kept) = kept()?;
            let tasks = workspaces::tasks(&found.summary.path, &kept);
            if as_json {
                let mut value = json!(found);
                value["summary"] = summary_json(&found.summary, &kept);
                value["tasks"] = json!(tasks);
                crate::json::print("repository", value);
                return Ok(());
            }
            print_details(&found, &tasks);
            Ok(())
        }
        ReposCommand::Diff { repo, file, staged } => {
            let path = pick(&repo)?;
            let found = repositories::details(&Here, &path).map_err(|error| anyhow!(error))?;
            let shown = repositories::diff(&Here, &path, &found, file.as_deref(), staged)
                .map_err(|error| anyhow!(error))?;
            if as_json {
                crate::json::print("diff", json!(shown));
                return Ok(());
            }
            print!("{}", printable_lines(&shown.text));
            if shown.truncated {
                println!("(cut short: it is too long to show here)");
            }
            Ok(())
        }
        ReposCommand::Stage {
            repo,
            files,
            all,
            confirm,
        } => {
            let wanted = if all {
                if !files.is_empty() {
                    bail!("give files or --all, not both");
                }
                Change::StageAll
            } else {
                if files.is_empty() {
                    bail!("give the files to stage, or --all");
                }
                Change::Stage(files)
            };
            change(&repo, wanted, confirm)
        }
        ReposCommand::Unstage {
            repo,
            files,
            confirm,
        } => change(&repo, Change::Unstage(files), confirm),
        ReposCommand::Commit {
            repo,
            message,
            confirm,
        } => change(&repo, Change::Commit(message), confirm),
        ReposCommand::Branch {
            repo,
            name,
            create,
            confirm,
        } => change(
            &repo,
            if create {
                Change::CreateBranch(name)
            } else {
                Change::SwitchBranch(name)
            },
            confirm,
        ),
        ReposCommand::Stash {
            repo,
            message,
            untracked,
            confirm,
        } => change(&repo, Change::Stash { message, untracked }, confirm),
        ReposCommand::StashApply {
            repo,
            stash,
            confirm,
        } => change(&repo, Change::ApplyStash(stash), confirm),
        ReposCommand::Fetch { repo, confirm } => change(&repo, Change::Fetch, confirm),
        ReposCommand::Pull { repo, confirm } => change(&repo, Change::Pull, confirm),
        ReposCommand::Push { repo, confirm } => change(&repo, Change::Push, confirm),
        ReposCommand::Publish {
            repo,
            remote,
            confirm,
        } => change(&repo, Change::Publish(remote), confirm),
        ReposCommand::Tasks { repo } => {
            let path = pick(&repo)?;
            let (_, kept) = kept()?;
            let tasks = workspaces::tasks(&path, &kept);
            if as_json {
                crate::json::print("tasks", json!({"repository": path, "tasks": tasks}));
                return Ok(());
            }
            if tasks.is_empty() {
                println!(
                    "{} names no tasks Helix Toolbox can find; add your own with helix-toolbox repos task-add.",
                    path.display()
                );
            }
            print_tasks(&tasks);
            Ok(())
        }
        ReposCommand::Task {
            repo,
            task,
            confirm,
        } => {
            let path = pick(&repo)?;
            let (_, kept) = kept()?;
            let tasks = workspaces::tasks(&path, &kept);
            let Some(chosen) = tasks.iter().find(|other| other.id == task) else {
                bail!(
                    "{} has no task {task}; see helix-toolbox repos tasks {repo}",
                    path.display()
                );
            };
            let planned = workspaces::plan_task(&path, chosen).map_err(|error| anyhow!(error))?;
            crate::confirm_and_run(
                catalog,
                platform,
                &planned,
                confirm.dry_run,
                confirm.yes,
                "TASK",
            )
        }
        ReposCommand::TaskAdd {
            repo,
            id,
            command,
            name,
            shell,
        } => {
            let path = pick(&repo)?;
            let (file, mut kept) = kept()?;
            kept.add_task(
                &path,
                Task {
                    name: name.unwrap_or_else(|| id.clone()),
                    id: id.clone(),
                    command,
                    shell: match shell {
                        ShellChoice::Direct => Shell::Direct,
                        ShellChoice::Fish => Shell::Fish,
                        ShellChoice::Bash => Shell::Bash,
                    },
                    detected: false,
                },
            )
            .map_err(|error| anyhow!(error))?;
            workspaces::save(&file, &kept).map_err(|error| anyhow!(error))?;
            println!(
                "The task {id} is kept for {} in {}.",
                path.display(),
                file.display()
            );
            Ok(())
        }
        ReposCommand::TaskRemove { repo, id } => {
            let path = pick(&repo)?;
            let (file, mut kept) = kept()?;
            kept.remove_task(&path, &id)
                .map_err(|error| anyhow!(error))?;
            workspaces::save(&file, &kept).map_err(|error| anyhow!(error))?;
            println!("The task {id} is removed.");
            Ok(())
        }
        ReposCommand::Favorite { repo, off } => {
            let path = pick(&repo)?;
            let (file, mut kept) = kept()?;
            kept.set_favorite(&path, !off);
            workspaces::save(&file, &kept).map_err(|error| anyhow!(error))?;
            println!(
                "{} is {} a favorite.",
                path.display(),
                if off { "no longer" } else { "now" }
            );
            Ok(())
        }
        ReposCommand::Group { repo, group } => {
            let path = pick(&repo)?;
            let (file, mut kept) = kept()?;
            kept.set_group(&path, &group)
                .map_err(|error| anyhow!(error))?;
            workspaces::save(&file, &kept).map_err(|error| anyhow!(error))?;
            println!("{} is in the group {group:?}.", path.display());
            Ok(())
        }
    }
}

fn shown_roots(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        return "no folders".to_string();
    }
    roots
        .iter()
        .map(|root| root.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Text from a repository, made safe for the terminal line by line.
fn printable_lines(text: &str) -> String {
    text.lines()
        .map(|line| format!("{}\n", printable(line)))
        .collect()
}

fn print_details(found: &Details, tasks: &[Task]) {
    let summary = &found.summary;
    println!(
        "{} on {}: {}",
        printable(&summary.name),
        printable(summary.branch.as_deref().unwrap_or("a detached HEAD")),
        sync_words(summary)
    );
    println!("  {}", summary.path.display());
    if let Some(origin) = &summary.origin {
        println!("  origin {}", printable(origin));
    }
    if let Some(busy) = &found.busy {
        println!("  In the middle of {busy}.");
    }
    println!();
    if found.files.is_empty() {
        println!("No changes.");
    } else {
        println!(
            "Changes ({}, {} staged):",
            found.files.len(),
            found.staged()
        );
        for file in &found.files {
            println!(
                "  {} {:<12} {}{}",
                if file.is_staged() { "S" } else { " " },
                file.state(),
                printable(&file.path),
                file.original
                    .as_deref()
                    .map(|original| format!(" (was {})", printable(original)))
                    .unwrap_or_default()
            );
        }
    }
    if !found.branches.is_empty() {
        println!();
        println!("Branches: {}", printable(&found.branches.join(", ")));
    }
    if !found.stashes.is_empty() {
        println!();
        println!("Stashes:");
        for stash in &found.stashes {
            println!("  {} {}", stash.name, printable(&stash.subject));
        }
    }
    if !found.log.is_empty() {
        println!();
        println!("Recent commits:");
        for commit in &found.log {
            println!(
                "  {} {:<10} {}",
                commit.hash,
                commit
                    .at
                    .map(helix_toolbox_engine::date)
                    .unwrap_or_default(),
                printable(&commit.subject)
            );
        }
    }
    if !tasks.is_empty() {
        println!();
        println!("Tasks:");
        print_tasks(tasks);
    }
}

fn print_tasks(tasks: &[Task]) {
    for task in tasks {
        println!(
            "  {:<16} {:<28} {}{}",
            printable(&task.id),
            printable(&task.name),
            match task.shell {
                Shell::Direct => "",
                Shell::Fish => "fish: ",
                Shell::Bash => "bash: ",
            },
            printable(&task.command)
        );
    }
}
