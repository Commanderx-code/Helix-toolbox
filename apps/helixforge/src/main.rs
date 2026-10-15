//! HelixForge: build, configure and optimise a Linux machine from a terminal
//! menu, or from the command line.

mod docgen;
mod tui;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item};
use helixforge_engine::{Planned, Step, history, plan, run_inherited};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::{
    io::{self, IsTerminal, Write},
    path::PathBuf,
};
use tui::theme::Theme;

#[derive(Parser)]
#[command(
    name = "helixforge",
    version,
    about = "HelixForge: build, configure and optimise"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Open the terminal interface (the default).
    Tui(TuiArgs),
    /// Show the distribution and package manager HelixForge detects.
    System,
    /// List the catalog as tabs, groups and entries.
    List {
        /// Include entries hidden on this machine.
        #[arg(short = 'u', long)]
        all: bool,
    },
    /// Show one entry.
    Info { id: String },
    /// Show what running entries would do, without running them.
    Plan { ids: Vec<String> },
    /// Run entries in this terminal, after confirmation.
    Run {
        ids: Vec<String>,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
    /// Show recent runs.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Write the catalog walkthrough as Markdown.
    Docgen { file: PathBuf },
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct TuiArgs {
    /// Colour theme: default, or compatible for terminals with few colours.
    #[arg(short, long, value_enum, default_value_t = Theme::Default)]
    pub theme: Theme,
    /// Skip the confirmation before running an entry.
    #[arg(short = 'y', long)]
    pub skip_confirmation: bool,
    /// Show every entry, ignoring the checks that hide unsupported ones.
    #[arg(short = 'u', long)]
    pub override_validation: bool,
    /// Draw even when the terminal is smaller than the recommended size.
    #[arg(short = 's', long)]
    pub size_bypass: bool,
    /// Do not warn when running as root.
    #[arg(short = 'r', long)]
    pub bypass_root: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let platform = LinuxInfo::detect().context("HelixForge needs Linux with /etc/os-release")?;
    match cli.command.unwrap_or(Command::Tui(TuiArgs::default())) {
        Command::Tui(args) => {
            let catalog =
                Catalog::builtin(!args.override_validation).context("cannot load the catalog")?;
            tui::run(catalog, platform, args)
        }
        Command::System => {
            println!(
                "Distribution:    {} ({})",
                platform.pretty_name, platform.id
            );
            println!(
                "Package manager: {}",
                platform
                    .manager
                    .map(|manager| manager.label())
                    .unwrap_or("none supported for package entries")
            );
            if let Some(manager) = platform.manager {
                println!(
                    "Manager command: {}",
                    if binary_exists(manager.executable()) {
                        "present"
                    } else {
                        "missing"
                    }
                );
            }
            println!(
                "sudo:            {}",
                if binary_exists("sudo") {
                    "present"
                } else {
                    "missing"
                }
            );
            Ok(())
        }
        Command::List { all } => {
            let catalog = Catalog::builtin(!all).context("cannot load the catalog")?;
            for tab in &catalog.tabs {
                println!("{}", tab.name);
                list_items(&tab.items, 1);
            }
            Ok(())
        }
        Command::Info { id } => {
            let catalog = Catalog::builtin(false)?;
            let entry = find(&catalog, &id)?;
            println!("{} ({})", entry.name, entry.id);
            if !entry.description.is_empty() {
                println!("{}", entry.description);
            }
            if !entry.flags.is_empty() {
                println!("Flags: {}", entry.flags);
            }
            match &entry.action {
                Action::Package { binary, packages } => {
                    println!(
                        "Kind: package, provides {binary} ({})",
                        if binary_exists(binary) {
                            "installed"
                        } else {
                            "not installed"
                        }
                    );
                    for (family, name) in [
                        ("arch", &packages.arch),
                        ("debian", &packages.debian),
                        ("fedora", &packages.fedora),
                    ] {
                        if let Some(name) = name {
                            println!("  {family}: {name}");
                        }
                    }
                }
                Action::Script { file } => println!("Kind: script, {}", file.display()),
                Action::Command { line } => println!("Kind: command, {line}"),
            }
            Ok(())
        }
        Command::Plan { ids } => {
            let catalog = Catalog::builtin(false)?;
            let (steps, notes) = steps_for(&catalog, &platform, &ids)?;
            print_plan(&steps, &notes);
            Ok(())
        }
        Command::Run { ids, dry_run, yes } => {
            let catalog = Catalog::builtin(false)?;
            let (steps, notes) = steps_for(&catalog, &platform, &ids)?;
            print_plan(&steps, &notes);
            if steps.is_empty() {
                println!("Nothing to run.");
                return Ok(());
            }
            if dry_run {
                println!("Dry run: nothing was changed.");
                return Ok(());
            }
            if !yes {
                if !io::stdin().is_terminal() {
                    bail!("confirmation needs a terminal; use --dry-run or --yes");
                }
                print!("Type RUN to continue: ");
                io::stdout().flush()?;
                let mut answer = String::new();
                io::stdin().read_line(&mut answer)?;
                if answer.trim() != "RUN" {
                    bail!("cancelled; nothing was changed");
                }
            }
            let status = run_inherited(&steps, &catalog.lib_dir())?;
            let names: Vec<(String, String)> = steps
                .iter()
                .map(|step| (step.id().to_string(), step.name().to_string()))
                .collect();
            let result = format!("exit {}", status.code().unwrap_or(-1));
            if let Err(error) = history::record(&names, status.success(), &result, None) {
                eprintln!("warning: the run was not recorded: {error}");
            }
            if !status.success() {
                bail!("the run stopped with {result}");
            }
            Ok(())
        }
        Command::History { limit } => {
            let records = history::read(limit)?;
            if records.is_empty() {
                println!("No runs recorded yet.");
            }
            for record in records {
                println!(
                    "{} | {} | {} | {}{}",
                    record.unix_timestamp,
                    if record.success { "ok    " } else { "failed" },
                    record.names.join(", "),
                    record.result,
                    record
                        .log
                        .map(|log| format!(" | log {}", log.display()))
                        .unwrap_or_default()
                );
            }
            Ok(())
        }
        Command::Docgen { file } => {
            let catalog = Catalog::builtin(false)?;
            std::fs::write(&file, docgen::walkthrough(&catalog.tabs))
                .with_context(|| format!("cannot write {}", file.display()))?;
            Ok(())
        }
    }
}

fn list_items(items: &[Item], depth: usize) {
    let indent = "  ".repeat(depth);
    for item in items {
        match item {
            Item::Group(group) => {
                println!("{indent}{}/", group.name);
                list_items(&group.items, depth + 1);
            }
            Item::Entry(entry) => {
                let flags = if entry.flags.is_empty() {
                    String::new()
                } else {
                    format!("  [{}]", entry.flags)
                };
                println!("{indent}{:<32} {}{flags}", entry.id, entry.name);
            }
        }
    }
}

fn find<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Entry> {
    catalog
        .find(id)
        .with_context(|| format!("no entry named '{id}'; see helixforge list"))
}

fn steps_for(
    catalog: &Catalog,
    platform: &LinuxInfo,
    ids: &[String],
) -> Result<(Vec<Step>, Vec<String>)> {
    let mut steps = Vec::new();
    let mut notes = Vec::new();
    for id in ids {
        let entry = find(catalog, id)?;
        match plan(entry, platform)? {
            Planned::Ready(step) => steps.push(step),
            Planned::AlreadyInstalled { binary } => notes.push(format!(
                "{}: already installed ({binary} is on the PATH)",
                entry.name
            )),
        }
    }
    Ok((steps, notes))
}

fn print_plan(steps: &[Step], notes: &[String]) {
    for note in notes {
        println!("{note}");
    }
    for (index, step) in steps.iter().enumerate() {
        println!("{}. {}\n   {}", index + 1, step.name(), step.display());
    }
}
