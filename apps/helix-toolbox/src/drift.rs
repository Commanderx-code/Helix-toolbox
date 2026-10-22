//! `helix-toolbox drift`: save this machine as a baseline, and compare it with
//! a baseline or a profile.

use crate::json;
use anyhow::{Context, Result, bail};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, printable};
pub use helix_toolbox_engine::drift::available;
use helix_toolbox_engine::{
    doctor,
    drift::{self, Baseline, Report},
    history,
    inventory::{Inventory, which},
    profiles,
};
use helix_toolbox_platform::LinuxInfo;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum DriftCommand {
    /// Compare this machine with a baseline (the newest saved one when none
    /// is named) or, with --profile, with a profile.
    Check {
        /// A saved baseline's name, or a baseline file.
        baseline: Option<String>,
        /// Compare with this profile instead: its id or its name.
        #[arg(long, conflicts_with = "baseline")]
        profile: Option<String>,
        /// Exit with status 1 when anything differs, for scripts.
        #[arg(long)]
        exit_code: bool,
    },
    /// Record this machine as a baseline: what is installed, the
    /// configuration files catalog entries write (by checksum), the login
    /// shell, display manager and repositories, and what Helix Toolbox ran.
    Save {
        name: String,
        /// Also record this file or folder of your own (repeatable).
        #[arg(long = "path", value_name = "PATH")]
        paths: Vec<PathBuf>,
    },
    /// List the saved baselines.
    List,
    /// Write a saved baseline to a file, to compare another machine with.
    Export {
        name: String,
        /// Where to write it; the current directory and its name when left out.
        file: Option<PathBuf>,
    },
    /// Add a baseline from a file, after checking it.
    Import { file: PathBuf },
    /// Delete a saved baseline.
    Delete { name: String },
}

/// What a capture or comparison reads from this machine.
struct Here {
    inventory: Inventory,
    home: PathBuf,
    history: Vec<history::Record>,
}

impl Here {
    fn gather(platform: &LinuxInfo) -> Self {
        Here {
            inventory: Inventory::gather(platform),
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            history: history::read(1000).unwrap_or_default(),
        }
    }

    fn sources<'a>(
        &'a self,
        catalog: &'a Catalog,
        platform: &'a LinuxInfo,
        settings: &'a dyn Fn() -> std::collections::BTreeMap<String, String>,
        render: &'a dyn Fn(&[String]) -> Result<String, String>,
    ) -> drift::Sources<'a> {
        drift::Sources {
            catalog,
            platform,
            inventory: &self.inventory,
            on_path: &which,
            system: &doctor::Here,
            home: &self.home,
            history: &self.history,
            settings,
            render,
        }
    }
}

fn settings() -> std::collections::BTreeMap<String, String> {
    drift::settings_here(&doctor::Here)
}

pub fn run(
    command: Option<DriftCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let directory = drift::dir().context("cannot find the configuration folder")?;
    let command = command.unwrap_or(DriftCommand::Check {
        baseline: None,
        profile: None,
        exit_code: false,
    });
    match command {
        DriftCommand::Check {
            baseline,
            profile,
            exit_code,
        } => {
            let here = Here::gather(platform);
            let scripts = catalog.root().join("scripts");
            let render = |arguments: &[String]| {
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
            };
            let sources = here.sources(catalog, platform, &settings, &render);
            let (report, against) = match profile {
                Some(name) => {
                    let (id, title, entries) = find_profile(catalog, &name)?;
                    let report = drift::compare_profile(&title, &entries, &sources);
                    (report, Against::Profile(id))
                }
                None => {
                    let baseline = match baseline {
                        Some(name) => load(&directory, &name)?,
                        None => drift::load_all(&directory)
                            .0
                            .into_iter()
                            .next()
                            .map(|(_, baseline)| baseline)
                            .context(
                                "no baseline saved yet: helix-toolbox drift save <name> records this machine as one",
                            )?,
                    };
                    let report = drift::compare(&baseline, &sources);
                    (report, Against::Baseline(baseline))
                }
            };
            let (restore, unavailable) = available(&report.restore(), catalog, platform);
            if as_json {
                json::print("drift", json::drift(&report, &against, &restore));
            } else {
                print_report(&report, &against, &restore, &unavailable, platform);
            }
            if exit_code && !report.differences.is_empty() {
                std::process::exit(1);
            }
            Ok(())
        }
        _ if as_json && !matches!(command, DriftCommand::List) => {
            bail!("--json is for drift check and drift list")
        }
        DriftCommand::List => {
            let (found, problems) = drift::load_all(&directory);
            if as_json {
                json::print("baselines", json::baselines(&found));
                return Ok(());
            }
            for problem in problems {
                eprintln!("Baseline skipped: {}", printable(&problem));
            }
            if found.is_empty() {
                println!(
                    "No baselines yet. helix-toolbox drift save <name> records this machine as one."
                );
            }
            for (slug, baseline) in &found {
                println!(
                    "{slug}  {}  {} on {}\n    {} entries, {} files, {} settings, {} runs",
                    printable(&baseline.name),
                    crate::when(baseline.taken),
                    printable(&baseline.system),
                    baseline.entries.len(),
                    baseline.files.len(),
                    baseline.settings.len(),
                    baseline.applied.len()
                );
            }
            Ok(())
        }
        DriftCommand::Save { name, paths } => {
            let here = Here::gather(platform);
            let scripts = catalog.root().join("scripts");
            let render = |arguments: &[String]| {
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
            };
            let sources = here.sources(catalog, platform, &settings, &render);
            let current = std::env::current_dir().unwrap_or_default();
            let extra: Vec<String> = paths
                .iter()
                .map(|path| current.join(path).display().to_string())
                .collect();
            let baseline = drift::capture(&name, history::now(), &sources, &extra);
            let file = drift::save(&directory, &baseline)?;
            println!(
                "Saved {}: {} entries, {} configuration paths, {} settings and {} runs, in {}.\nCompare later with: helix-toolbox drift check \"{}\"",
                printable(&baseline.name),
                baseline.entries.len(),
                baseline.files.len(),
                baseline.settings.len(),
                baseline.applied.len(),
                file.display(),
                printable(&baseline.name)
            );
            Ok(())
        }
        DriftCommand::Export { name, file } => {
            let (slug, baseline) = named(&directory, &name)?;
            let file = file.unwrap_or_else(|| PathBuf::from(format!("{slug}.toml")));
            drift::write(&file, &baseline)?;
            println!(
                "Exported {} to {}; on another machine: helix-toolbox drift check {}",
                printable(&baseline.name),
                file.display(),
                file.display()
            );
            Ok(())
        }
        DriftCommand::Import { file } => {
            let baseline = drift::read(&file)?;
            let saved = drift::save(&directory, &baseline)?;
            println!(
                "Imported {} as {}; compare with: helix-toolbox drift check \"{}\"",
                printable(&baseline.name),
                saved.display(),
                printable(&baseline.name)
            );
            Ok(())
        }
        DriftCommand::Delete { name } => {
            let (slug, baseline) = named(&directory, &name)?;
            let path = directory.join(format!("{slug}.toml"));
            std::fs::remove_file(&path)
                .with_context(|| format!("cannot delete {}", path.display()))?;
            println!("Deleted {}", printable(&baseline.name));
            Ok(())
        }
    }
}

/// What a report was compared with.
pub enum Against {
    Baseline(Baseline),
    /// A profile, by id.
    Profile(String),
}

fn named(directory: &Path, name: &str) -> Result<(String, Baseline)> {
    drift::find(directory, name)
        .with_context(|| format!("no baseline named '{name}'; see helix-toolbox drift list"))
}

/// A saved baseline by name: its file name and itself.
pub fn saved(name: &str) -> Result<(String, Baseline)> {
    let directory = drift::dir().context("cannot find the configuration folder")?;
    named(&directory, name)
}

/// Compare this machine with a baseline, as `drift check` does.
pub fn compare(catalog: &Catalog, platform: &LinuxInfo, baseline: &Baseline) -> Report {
    let here = Here::gather(platform);
    let scripts = catalog.root().join("scripts");
    let render =
        |arguments: &[String]| helix_toolbox_engine::preview::render_with(&scripts, arguments);
    drift::compare(
        baseline,
        &here.sources(catalog, platform, &settings, &render),
    )
}

/// A saved baseline by name, or a baseline file.
fn load(directory: &Path, name: &str) -> Result<Baseline> {
    let path = Path::new(name);
    if (name.contains('/') || name.ends_with(".toml")) && path.is_file() {
        return Ok(drift::read(path)?);
    }
    Ok(named(directory, name)?.1)
}

/// A profile entry by id, a user profile by name, or a built-in one by
/// name: its id, name and entries.
fn find_profile(catalog: &Catalog, name: &str) -> Result<(String, String, Vec<String>)> {
    let lowered = name.to_lowercase();
    let user = profiles::slug(name).map(|slug| format!("{}{slug}", profiles::PREFIX));
    catalog
        .entries()
        .into_iter()
        .find_map(|entry| match &entry.action {
            Action::Profile { entries }
                if entry.id == name
                    || Some(&entry.id) == user.as_ref()
                    || entry.name.to_lowercase() == lowered =>
            {
                Some((entry.id.clone(), entry.name.clone(), entries.clone()))
            }
            _ => None,
        })
        .with_context(|| format!("no profile named '{name}'; see helix-toolbox list"))
}

fn print_report(
    report: &Report,
    against: &Against,
    restore: &[String],
    unavailable: &[String],
    platform: &LinuxInfo,
) {
    match against {
        Against::Baseline(baseline) => println!(
            "Compared with the baseline {}, saved {} on {}.",
            printable(&baseline.name),
            crate::when(baseline.taken),
            printable(&baseline.system)
        ),
        Against::Profile(id) => println!("Compared with the profile {} ({id}).", report.against),
    }
    for unknown in &report.unknown {
        println!("Not in this catalog, so not compared: {unknown}");
    }
    if report.differences.is_empty() {
        println!(
            "No differences: {} things are as they were.",
            report.matched
        );
        return;
    }
    let width = report
        .differences
        .iter()
        .map(|difference| difference.subject.chars().count())
        .max()
        .unwrap_or(0)
        .min(40);
    for difference in &report.differences {
        println!(
            "{:<9} {:<width$}  {}",
            difference.kind.label(),
            difference.subject,
            difference.note
        );
    }
    let counts: Vec<String> = report
        .counts()
        .into_iter()
        .map(|(label, count)| format!("{count} {}", label.to_lowercase()))
        .collect();
    println!("\n{}; {} the same.", counts.join(", "), report.matched);
    if !restore.is_empty() {
        println!(
            "To put back what is missing (planned and confirmed first): helix-toolbox run {}",
            restore.join(" ")
        );
    }
    if !unavailable.is_empty() {
        println!(
            "Not available on {}: {}",
            printable(&platform.pretty_name),
            unavailable.join(", ")
        );
    }
}
