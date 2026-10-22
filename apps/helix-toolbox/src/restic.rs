//! `helix-toolbox restic`: Restic backups of your own files on the command
//! line (helix_toolbox_engine::restic). Showing, browsing and a file's
//! history only read; checking, backing up, restoring and recovery tests
//! are plans, confirmed like any other.

use anyhow::{Result, bail};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::printable;
use helix_toolbox_engine::{
    history, notifications,
    restic::{self, Config, Every, Found, Here},
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum ResticCommand {
    /// The repository, how restic finds its password, and the snapshots
    /// (the default).
    Show,
    /// What a folder of a snapshot holds.
    Files {
        /// The snapshot's id, or latest.
        snapshot: String,
        /// The folder, a full path; the snapshot's first folder when left
        /// out.
        folder: Option<String>,
    },
    /// Every version of the files a name or path matches, across all
    /// snapshots, with which changed.
    History {
        /// A file name or path, with * wildcards, such as notes.txt or
        /// ~/Documents/*.odt.
        pattern: String,
        #[arg(long)]
        ignore_case: bool,
    },
    /// Check the repository's structure; confirmed by typing CHECK.
    Check {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Back up the folders `[restic]` in settings.toml names (the home
    /// folder unless set); confirmed by typing BACKUP.
    Backup {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Restore a snapshot, or one path of it, into a new folder of your
    /// home folder; nothing is overwritten. Confirmed by typing RESTORE.
    Restore {
        snapshot: String,
        /// The new folder to restore into.
        target: PathBuf,
        /// Only this path of the snapshot, a full path.
        #[arg(long)]
        include: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Record a file's SHA-256 as a baseline to test restores against; with
    /// no file, list the baselines.
    Baseline { file: Option<PathBuf> },
    /// Restore a baseline's file from a snapshot into a temporary folder
    /// and compare its SHA-256; confirmed by typing TEST.
    Test {
        /// The baseline: its number in `restic baseline`, or its id.
        baseline: String,
        /// The snapshot; the latest when left out.
        snapshot: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Back up on a timer of your systemd user session, or stop.
    Timer {
        switch: Switch,
        /// Every week instead of every day.
        #[arg(long)]
        weekly: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum Switch {
    On,
    Off,
}

fn home() -> Result<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow::anyhow!("there is no home folder"))
}

fn state() -> Result<PathBuf> {
    history::state_dir().ok_or_else(|| anyhow::anyhow!("cannot find the state folder"))
}

/// The repository set up, or why there is none.
fn ready() -> Result<Config> {
    match restic::here() {
        Found::Ready(config) => Ok(config),
        Found::Problem(why) => bail!("{why}"),
        Found::Missing => bail!(
            "no restic repository is set up: add [restic] with repository and password_file to {}",
            helix_toolbox_engine::settings::path()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "settings.toml".into())
        ),
    }
}

/// A path typed in: `~/` for the home folder, relative to here otherwise.
fn typed(path: &Path) -> Result<PathBuf> {
    let home = home()?;
    Ok(match path.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) if path.is_absolute() => path.to_path_buf(),
        Err(_) => std::env::current_dir()?.join(path),
    })
}

/// What the overview shows, as JSON: also the window's.
pub fn overview(
    found: &Found,
    snapshots: Option<&Result<Vec<restic::Snapshot>, String>>,
) -> serde_json::Value {
    let timer = restic::units_here().and_then(|units| restic::timer(&units));
    let (config, problem) = match found {
        Found::Ready(config) => (Some(config), None),
        Found::Problem(why) => (None, Some(why.clone())),
        Found::Missing => (None, None),
    };
    json!({
        "configured": config.is_some(),
        "repository": config.map(|config| &config.repository),
        "origin": config.map(|config| config.origin),
        "password": config.map(|config| config.password.describe()),
        "paths": config.map(|config| &config.paths).cloned().unwrap_or_default(),
        "excludes": config.map(|config| &config.excludes).cloned().unwrap_or_default(),
        "problem": problem,
        "snapshots": snapshots.and_then(|read| read.as_ref().ok()).cloned().unwrap_or_default(),
        "snapshots_problem": snapshots.and_then(|read| read.as_ref().err()),
        "timer": timer,
    })
}

pub fn restic(
    command: Option<ResticCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    match command.unwrap_or(ResticCommand::Show) {
        ResticCommand::Show => show(as_json),
        ResticCommand::Files { snapshot, folder } => {
            let config = ready()?;
            let folder = match folder {
                Some(folder) => folder,
                None if snapshot == "latest" => config
                    .paths
                    .first()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "/".into()),
                None => restic::snapshots(&Here, &config)
                    .map_err(anyhow::Error::msg)?
                    .iter()
                    .find(|found| found.id.starts_with(&snapshot))
                    .and_then(|found| found.paths.first().cloned())
                    .unwrap_or_else(|| "/".into()),
            };
            let nodes =
                restic::files(&Here, &config, &snapshot, &folder).map_err(anyhow::Error::msg)?;
            if as_json {
                crate::json::print(
                    "restic-files",
                    json!({"snapshot": snapshot, "folder": folder, "nodes": nodes}),
                );
                return Ok(());
            }
            println!(
                "{} in snapshot {}:",
                printable(&folder),
                printable(&snapshot)
            );
            for node in &nodes {
                if node.is_dir() {
                    println!("  {}/", printable(&node.name));
                } else {
                    println!(
                        "  {:<48} {}",
                        printable(&node.name),
                        node.size.map(size).unwrap_or_default()
                    );
                }
            }
            if nodes.is_empty() {
                println!("  (nothing)");
            }
            Ok(())
        }
        ResticCommand::History {
            pattern,
            ignore_case,
        } => {
            let config = ready()?;
            let found = restic::history(&Here, &config, &home()?, &pattern, ignore_case)
                .map_err(anyhow::Error::msg)?;
            if as_json {
                crate::json::print(
                    "restic-history",
                    json!({"pattern": pattern, "files": found}),
                );
                return Ok(());
            }
            if found.is_empty() {
                println!("No snapshot has a file matching {}.", printable(&pattern));
            }
            for file in &found {
                println!("{}", printable(&file.path));
                for version in &file.versions {
                    println!(
                        "  {}  {}  {:<10} {}",
                        version.short_id,
                        version
                            .time
                            .replace('T', " ")
                            .get(..16)
                            .unwrap_or(&version.time),
                        version.size.map(size).unwrap_or_default(),
                        match version.change {
                            restic::Change::First => "first",
                            restic::Change::Changed => "changed",
                            restic::Change::Same => "same",
                        }
                    );
                }
                if file.missing_from > 0 {
                    println!(
                        "  Not in the {} newer snapshot(s) of the same folder: deleted or moved since.",
                        file.missing_from
                    );
                }
            }
            if !found.is_empty() {
                println!(
                    "helix-toolbox restic restore <snapshot> <new folder> --include <path> brings one back."
                );
            }
            Ok(())
        }
        ResticCommand::Check { dry_run, yes } => crate::confirm_and_run(
            catalog,
            platform,
            &restic::plan_check(&ready()?),
            dry_run,
            yes,
            "CHECK",
        ),
        ResticCommand::Backup { dry_run, yes } => crate::confirm_and_run(
            catalog,
            platform,
            &restic::plan_backup(&ready()?),
            dry_run,
            yes,
            "BACKUP",
        ),
        ResticCommand::Restore {
            snapshot,
            target,
            include,
            dry_run,
            yes,
        } => {
            let config = ready()?;
            if !restic::restores_safely(&Here) {
                bail!(
                    "this restic cannot restore without overwriting and check what it wrote; restic 0.17 or later can"
                );
            }
            let target =
                restic::check_target(&home()?, &typed(&target)?).map_err(anyhow::Error::msg)?;
            let include = include
                .map(|path| typed(Path::new(&path)).map(|path| path.display().to_string()))
                .transpose()?;
            let planned = restic::plan_restore(&config, &snapshot, &target, include.as_deref())
                .map_err(anyhow::Error::msg)?;
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "RESTORE")
        }
        ResticCommand::Baseline { file: Some(file) } => {
            let recorded = restic::record(&state()?, &home()?, &typed(&file)?, history::now())
                .map_err(anyhow::Error::msg)?;
            if as_json {
                crate::json::print("restic-baseline", json!(recorded));
                return Ok(());
            }
            println!("Recorded {}: SHA-256 {}.", recorded.path, recorded.sha256);
            println!(
                "Back it up, then helix-toolbox restic test 1 restores it from the latest snapshot and compares."
            );
            Ok(())
        }
        ResticCommand::Baseline { file: None } => {
            let all = restic::baselines(&state()?);
            if as_json {
                crate::json::print("restic-baselines", json!({"baselines": all}));
                return Ok(());
            }
            if all.is_empty() {
                println!("No baselines yet: helix-toolbox restic baseline <file> records one.");
            }
            for (place, baseline) in all.iter().enumerate() {
                println!(
                    "{:>2}. {}  {}  {}",
                    place + 1,
                    crate::when(baseline.recorded),
                    &baseline.sha256[..12.min(baseline.sha256.len())],
                    printable(&baseline.path)
                );
            }
            Ok(())
        }
        ResticCommand::Test {
            baseline,
            snapshot,
            dry_run,
            yes,
        } => {
            let config = ready()?;
            let all = restic::baselines(&state()?);
            let Some(chosen) = restic::find_baseline(&all, &baseline) else {
                bail!("no baseline {baseline}: helix-toolbox restic baseline lists them");
            };
            let planned =
                restic::plan_test(&config, chosen, snapshot.as_deref().unwrap_or("latest"))
                    .map_err(anyhow::Error::msg)?;
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "TEST")
        }
        ResticCommand::Timer { switch, weekly } => {
            let units = restic::units_here()
                .ok_or_else(|| anyhow::anyhow!("cannot find your systemd folder"))?;
            match switch {
                Switch::On => {
                    let config = ready()?;
                    let program = std::env::current_exe()?;
                    let every = if weekly { Every::Week } else { Every::Day };
                    let started = restic::timer_on(
                        &units,
                        &program,
                        &config,
                        every,
                        &notifications::systemctl,
                    )
                    .map_err(anyhow::Error::msg)?;
                    println!(
                        "The backup runs {}{}.",
                        every.label(),
                        match started {
                            notifications::Started::Now => "",
                            notifications::Started::AtLogin => ", from the next login",
                        }
                    );
                }
                Switch::Off => {
                    restic::timer_off(&units, &notifications::systemctl)
                        .map_err(anyhow::Error::msg)?;
                    println!("The timed backup is off.");
                }
            }
            Ok(())
        }
    }
}

fn size(bytes: u64) -> String {
    helix_toolbox_engine::storage::shown(bytes)
}

fn show(as_json: bool) -> Result<()> {
    let found = restic::here();
    let snapshots = match &found {
        Found::Ready(config) => Some(restic::snapshots(&Here, config)),
        _ => None,
    };
    if as_json {
        crate::json::print("restic", overview(&found, snapshots.as_ref()));
        return Ok(());
    }
    let config = match &found {
        Found::Ready(config) => config,
        Found::Problem(why) => bail!("{why}"),
        Found::Missing => {
            println!("No restic repository is set up. Add to settings.toml:");
            println!();
            println!("[restic]");
            println!("repository = \"/path/to/the/repository\"");
            println!("password_file = \"~/.config/restic/password\"");
            println!("# paths = [\"~/Documents\"]  (the home folder when left out)");
            println!("# excludes = [\"~/.cache\"]");
            println!();
            println!("RESTIC_REPOSITORY and RESTIC_PASSWORD_FILE are read too.");
            return Ok(());
        }
    };
    println!(
        "Repository: {} (from {})",
        printable(&config.repository),
        config.origin.label()
    );
    println!("Password: from {}", config.password.describe());
    println!(
        "Backs up: {}{}",
        config
            .paths
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        if config.excludes.is_empty() {
            String::new()
        } else {
            format!(", leaving out {}", config.excludes.join(", "))
        }
    );
    println!(
        "Timer: {}",
        restic::units_here()
            .and_then(|units| restic::timer(&units))
            .map(Every::label)
            .unwrap_or("off (helix-toolbox restic timer on)")
    );
    match snapshots.unwrap_or_else(|| Ok(Vec::new())) {
        Ok(listed) => {
            println!();
            println!("Snapshots ({}):", listed.len());
            for snapshot in &listed {
                println!(
                    "  {}  {}  {:<16} {}",
                    snapshot.short_id,
                    snapshot.when(),
                    printable(&snapshot.hostname),
                    printable(&snapshot.paths.join(", "))
                );
            }
            Ok(())
        }
        Err(why) => bail!("restic could not read the repository: {why}"),
    }
}
