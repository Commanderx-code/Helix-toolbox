//! HelixForge: build, configure and optimise a Linux machine from a terminal
//! menu, or from the command line.

mod docgen;
mod logging;
mod tui;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item, NativeAction};
use helixforge_engine::{InheritedRunner, Plan, execute, history, plan, plan_removal, run_native};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::{
    io::{self, BufRead, IsTerminal, Write},
    path::PathBuf,
};
use tui::theme::Theme;

/// The release version: what the release workflow builds with
/// (HELIXFORGE_VERSION, from scripts/release-version.py), or the workspace
/// version in a local build.
const VERSION: &str = match option_env!("HELIXFORGE_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Parser)]
#[command(
    name = "helixforge",
    version = VERSION,
    about = "HelixForge: build, configure and optimise"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Read the catalog and scripts from this directory instead of the copy
    /// built into the program; for tests with a fixture catalog.
    #[arg(long, global = true, hide = true, value_name = "DIR")]
    catalog_dir: Option<PathBuf>,
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
    /// Remove package entries that offer removal, after confirmation.
    Remove {
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
    /// Run a native operation; the engine calls this from a run.
    #[command(hide = true)]
    Native { action: String },
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct TuiArgs {
    /// Colour theme for this run: solid (dark panels, the default),
    /// transparent (over the terminal's background), or compatible (sixteen
    /// colours). Settings in the menu changes the remembered one.
    #[arg(short, long, value_enum)]
    pub theme: Option<Theme>,
    /// Draw plain text instead of Nerd Font icons for this run.
    #[arg(long)]
    pub plain: bool,
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
    logging::init();
    let platform = LinuxInfo::detect().context("HelixForge needs Linux with /etc/os-release")?;
    let source = cli.catalog_dir;
    let load = |validate: bool| -> Result<Catalog> {
        match &source {
            Some(directory) => Catalog::from_directory(directory, validate)
                .with_context(|| format!("cannot load the catalog in {}", directory.display())),
            None => Catalog::builtin(validate).context("cannot load the catalog"),
        }
    };
    match cli.command.unwrap_or(Command::Tui(TuiArgs::default())) {
        Command::Tui(args) => {
            // Keys are read from the controlling terminal, not standard
            // input, so `curl … | sh` still works.
            if !io::stdout().is_terminal() || std::fs::File::open("/dev/tty").is_err() {
                bail!(
                    "the menu needs a terminal; run helixforge from one, or see helixforge --help for the commands that work without it"
                );
            }
            tui::run(load(!args.override_validation)?, platform, args)
        }
        Command::System => {
            print!("{}", run_native(NativeAction::SystemReport, &platform));
            Ok(())
        }
        Command::List { all } => {
            let catalog = load(!all)?;
            for tab in &catalog.tabs {
                println!("{}", tab.name);
                list_items(&tab.items, 1);
            }
            Ok(())
        }
        Command::Info { id } => {
            let catalog = load(false)?;
            let entry = find(&catalog, &id)?;
            println!("{} ({})", entry.name, entry.id);
            if !entry.description.is_empty() {
                println!("{}", entry.description);
            }
            if !entry.flags.is_empty() {
                println!("Flags: {}", entry.flags);
            }
            match &entry.action {
                Action::Package {
                    binary,
                    packages,
                    aur,
                    flatpak,
                    removable,
                } => {
                    println!(
                        "Kind: package, provides {binary} ({})",
                        if binary_exists(binary) {
                            "installed"
                        } else {
                            "not installed"
                        }
                    );
                    for (family, name) in packages.all() {
                        if let Some(name) = name {
                            println!("  {family}: {name}");
                        }
                    }
                    if let Some(aur) = aur {
                        println!("  AUR (Arch, through paru or yay): {aur}");
                    }
                    if let Some(flatpak) = flatpak {
                        println!("  Flathub, for other families: {flatpak}");
                    }
                    if *removable {
                        println!("Removable: helixforge remove {}", entry.id);
                    }
                }
                Action::Script { file, interactive } => println!(
                    "Kind: script, {}{}",
                    file.display(),
                    if *interactive { " (interactive)" } else { "" }
                ),
                Action::Command { line } => println!("Kind: command, {line}"),
                Action::Nix { action } => println!("Kind: nix, {action:?}"),
                Action::Native { action } => println!("Kind: native, {}", action.id()),
                Action::Profile { entries } => println!("Kind: profile, {}", entries.join(", ")),
            }
            Ok(())
        }
        Command::Plan { ids } => {
            let catalog = load(false)?;
            let planned = plan_ids(&catalog, &platform, &ids)?;
            print_plan(&planned);
            Ok(())
        }
        Command::Run { ids, dry_run, yes } => {
            let catalog = load(false)?;
            let planned = plan_ids(&catalog, &platform, &ids)?;
            confirm_and_run(&catalog, &planned, dry_run, yes, "RUN")
        }
        Command::Remove { ids, dry_run, yes } => {
            let catalog = load(false)?;
            let mut entries = Vec::new();
            for id in &ids {
                entries.push(find(&catalog, id)?);
            }
            let planned = plan_removal(entries, &platform)?;
            confirm_and_run(&catalog, &planned, dry_run, yes, "REMOVE")
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
            let catalog = load(false)?;
            std::fs::write(&file, docgen::walkthrough(&catalog.tabs))
                .with_context(|| format!("cannot write {}", file.display()))?;
            Ok(())
        }
        Command::Native { action } => {
            let action = NativeAction::parse(&action)
                .with_context(|| format!("unknown native action '{action}'"))?;
            print!("{}", run_native(action, &platform));
            Ok(())
        }
    }
}

/// Show a plan, ask for the confirmation word unless told not to, and run it.
fn confirm_and_run(
    catalog: &Catalog,
    planned: &Plan,
    dry_run: bool,
    yes: bool,
    word: &str,
) -> Result<()> {
    print_plan(planned);
    if planned.is_empty() {
        println!("Nothing to run.");
        return Ok(());
    }
    if dry_run {
        println!("Dry run: nothing was changed.");
        return Ok(());
    }
    if !yes && confirm(word)?.trim() != word {
        bail!("cancelled; nothing was changed");
    }
    let this = std::env::current_exe().context("cannot find the HelixForge executable")?;
    let finished = execute(planned, &catalog.lib_dir(), &this, &mut InheritedRunner)?;
    if !finished.outcome.succeeded() {
        bail!("the run stopped: {}", finished.outcome.describe());
    }
    for check in &finished.checks {
        println!(
            "{} {}: {}",
            if check.passed {
                "Verified:"
            } else {
                "Not verified:"
            },
            check.name,
            check.detail
        );
    }
    if !finished.succeeded() {
        bail!("the run finished, but not everything it did could be verified");
    }
    Ok(())
}

/// Ask for the confirmation word. When standard input is not a terminal, as
/// with `curl … | sh -s -- run <entry>`, ask on the controlling terminal.
fn confirm(word: &str) -> Result<String> {
    let prompt = format!("Type {word} to continue: ");
    let mut answer = String::new();
    if io::stdin().is_terminal() {
        print!("{prompt}");
        io::stdout().flush()?;
        io::stdin().read_line(&mut answer)?;
        return Ok(answer);
    }
    let Ok(mut terminal) = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/tty")
    else {
        bail!("confirmation needs a terminal; use --dry-run or --yes");
    };
    terminal.write_all(prompt.as_bytes())?;
    terminal.flush()?;
    io::BufReader::new(terminal).read_line(&mut answer)?;
    Ok(answer)
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

fn plan_ids(catalog: &Catalog, platform: &LinuxInfo, ids: &[String]) -> Result<Plan> {
    let mut entries = Vec::new();
    for id in ids {
        entries.push(find(catalog, id)?);
    }
    Ok(plan(entries, platform, &|id| catalog.find(id))?)
}

fn print_plan(planned: &Plan) {
    for note in &planned.notes {
        println!("{note}");
    }
    for blocked in &planned.blocked {
        println!("Not run: {blocked}");
    }
    for (index, step) in planned.steps.iter().enumerate() {
        println!("{}. {}\n   {}", index + 1, step.name(), step.display());
        for fallback in step.fallbacks() {
            println!("   {fallback}");
        }
    }
    if !planned.is_empty() {
        println!(
            "Risk: {}{}",
            planned.risk.label(),
            if planned.privileged {
                "; asks for your password"
            } else {
                ""
            }
        );
    }
}
