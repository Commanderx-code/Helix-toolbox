//! HelixForge: build, configure and optimise a Linux machine from a terminal
//! menu, or from the command line.

mod docgen;
mod logging;
mod tui;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item, NativeAction, printable};
use helixforge_engine::{
    InheritedRunner, Machine, PackageState, Plan, backups, doctor, execute, history,
    inventory::{Inventory, Status, which},
    package_state, plan, plan_removal, preflight, run_native,
};
use helixforge_platform::LinuxInfo;
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
    /// Show whether package entries are installed, where from, at which
    /// version, and whether an update is waiting (from the repository
    /// metadata already on this machine; nothing is downloaded).
    Status {
        /// The entries to show; every package entry on this machine when none.
        ids: Vec<String>,
    },
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
        /// After a failed step, go on with the steps that do not need it.
        #[arg(long)]
        keep_going: bool,
        /// Take a Snapper or Timeshift snapshot first when the run changes
        /// the system (privileged files, the kernel, disks).
        #[arg(long)]
        snapshot: bool,
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
    /// Update installed package entries, after confirmation: through their
    /// package manager or Flatpak; on Arch, the whole system.
    Update {
        /// The entries to update; every package entry with an update when none.
        ids: Vec<String>,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
        /// After a failed step, go on with the steps that do not need it.
        #[arg(long)]
        keep_going: bool,
    },
    /// List configuration backups HelixForge's entries have left, newest
    /// first, and whether snapshots are taken before system changes.
    Backups,
    /// Put a backup back; the current file is kept as a backup first.
    Restore {
        /// The backup's path, as `helixforge backups` lists it.
        backup: PathBuf,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
    /// Check the machine's health: network, DNS, services, graphics drivers,
    /// package database, home folder permissions, boot images, disk space.
    /// Read-only; a repair runs only with --repair, after confirmation.
    Doctor {
        /// Run the repair offered for this check (its id, as listed).
        #[arg(long, value_name = "ID")]
        repair: Option<String>,
        /// With --repair: show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// With --repair: skip the confirmation phrase.
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
                    conflicts,
                    removable,
                } => {
                    let state = match package_state(entry, &platform, Machine::real()) {
                        Some(PackageState::Installed(found)) => format!("installed: {found}"),
                        Some(PackageState::OnPath(found)) => {
                            format!("installed some other way: {found}")
                        }
                        Some(PackageState::Absent { replaces }) if !replaces.is_empty() => {
                            format!("not installed; would replace {}", replaces.join(", "))
                        }
                        _ => "not installed".to_string(),
                    };
                    println!("Kind: package ({state})");
                    if let Some(binary) = binary {
                        println!("  Command: {binary}");
                    }
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
                    if !conflicts.is_empty() {
                        println!("  Replaces: {}", conflicts.join(", "));
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
        Command::Status { ids } => {
            let catalog = load(ids.is_empty())?;
            let entries: Vec<&Entry> = if ids.is_empty() {
                catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| entry.identity().is_some())
                    .collect()
            } else {
                ids.iter()
                    .map(|id| find(&catalog, id))
                    .collect::<Result<_>>()?
            };
            let inventory = Inventory::gather(&platform);
            for entry in entries {
                match inventory.status(entry, &platform, &which) {
                    Some(status) => println!(
                        "{:<34} {:<14} {}",
                        entry.name,
                        status.label(),
                        status.detail()
                    ),
                    None => println!("{:<34} {:<14} not a package entry", entry.name, "-"),
                }
            }
            Ok(())
        }
        Command::Plan { ids } => {
            let catalog = load(false)?;
            let planned = plan_ids(&catalog, &platform, &ids)?;
            print_plan(&catalog, &platform, &planned);
            Ok(())
        }
        Command::Run {
            ids,
            dry_run,
            yes,
            keep_going,
            snapshot,
        } => {
            let catalog = load(false)?;
            let mut planned = plan_ids(&catalog, &platform, &ids)?;
            planned.keep_going = keep_going;
            if snapshot {
                add_snapshot(&catalog, &mut planned);
            }
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "RUN")
        }
        Command::Remove { ids, dry_run, yes } => {
            let catalog = load(false)?;
            let mut entries = Vec::new();
            for id in &ids {
                entries.push(find(&catalog, id)?);
            }
            let planned = plan_removal(entries, &platform)?;
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "REMOVE")
        }
        Command::Update {
            ids,
            dry_run,
            yes,
            keep_going,
        } => {
            let catalog = load(ids.is_empty())?;
            let inventory = Inventory::gather(&platform);
            let entries: Vec<&Entry> = if ids.is_empty() {
                catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| {
                        matches!(
                            inventory.status(entry, &platform, &which),
                            Some(Status::Update { .. })
                        )
                    })
                    .collect()
            } else {
                ids.iter()
                    .map(|id| find(&catalog, id))
                    .collect::<Result<_>>()?
            };
            if entries.is_empty() {
                println!(
                    "No package entry has an update in the repository metadata on this machine."
                );
                return Ok(());
            }
            let mut planned = inventory.plan_update(entries, &platform, &which);
            planned.keep_going = keep_going;
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "UPDATE")
        }
        Command::Doctor {
            repair,
            dry_run,
            yes,
        } => {
            let found = doctor::examine(&doctor::Here, platform.manager);
            let Some(id) = repair else {
                for diagnosis in &found {
                    println!(
                        "{:<8} {}: {}  [{}]",
                        diagnosis.health.label(),
                        diagnosis.title,
                        diagnosis.summary,
                        diagnosis.id
                    );
                    if diagnosis.health != doctor::Health::Good {
                        for line in &diagnosis.evidence {
                            println!("           {line}");
                        }
                        if let Some(advice) = &diagnosis.advice {
                            println!("           What to do: {advice}");
                        }
                        if let Some(fix) = &diagnosis.repair {
                            println!(
                                "           Repair: {} (helixforge doctor --repair {})",
                                fix.label, diagnosis.id
                            );
                        }
                        if let Some(entry) = &diagnosis.entry {
                            println!("           Entry: helixforge run {entry}");
                        }
                    }
                }
                return Ok(());
            };
            let diagnosis = found
                .iter()
                .find(|diagnosis| diagnosis.id == id)
                .with_context(|| format!("no check named '{id}'; see helixforge doctor"))?;
            let fix = diagnosis
                .repair
                .as_ref()
                .with_context(|| format!("{} has no repair HelixForge can run", diagnosis.title))?;
            let catalog = load(false)?;
            confirm_and_run(
                &catalog,
                &platform,
                &fix.plan(&diagnosis.id),
                dry_run,
                yes,
                "REPAIR",
            )
        }
        Command::Backups => {
            match backups::Snapshots::here() {
                Some(tool) => println!(
                    "Snapshots: {} is set up; run --snapshot and the menu take one before system changes. Roll back with {}. HelixForge does not roll back packages itself.\n",
                    tool.label(),
                    tool.how_to_restore()
                ),
                None => println!(
                    "Snapshots: neither Snapper (with a root configuration) nor Timeshift is set up, so none are taken.\n"
                ),
            }
            let found = backups::discover(&backups::Roots::here());
            if found.is_empty() {
                println!("No backups found.");
            }
            for backup in found {
                println!(
                    "{}  {:<6}  {}\n    {}",
                    backup.taken.as_deref().unwrap_or("unknown time       "),
                    if backup.system { "system" } else { "user" },
                    printable(&backup.original.display().to_string()),
                    printable(&backup.backup.display().to_string())
                );
            }
            Ok(())
        }
        Command::Restore {
            backup,
            dry_run,
            yes,
        } => {
            let catalog = load(false)?;
            // Only a backup HelixForge finds can be restored, never any path.
            let found = backups::discover(&backups::Roots::here())
                .into_iter()
                .find(|known| known.backup == backup)
                .with_context(|| {
                    format!(
                        "{} is not a backup HelixForge knows; see helixforge backups",
                        backup.display()
                    )
                })?;
            let planned = backups::plan_restore(&found);
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "RESTORE")
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

/// Put a snapshot step first when the plan changes the system and the
/// machine can take one; otherwise say why not.
fn add_snapshot(catalog: &Catalog, planned: &mut Plan) {
    let flags = |id: &str| catalog.find(id).map(|entry| entry.flags.clone());
    if !backups::worth_a_snapshot(planned, &flags) {
        println!("No snapshot: this run does not change system files, the kernel or disks.");
        return;
    }
    match backups::Snapshots::here() {
        Some(tool) => backups::with_snapshot(planned, tool),
        None => println!("No snapshot: neither Snapper nor Timeshift is set up here."),
    }
}

/// Show a plan, ask for the confirmation word unless told not to, and run it.
fn confirm_and_run(
    catalog: &Catalog,
    platform: &LinuxInfo,
    planned: &Plan,
    dry_run: bool,
    yes: bool,
    word: &str,
) -> Result<()> {
    let findings = print_plan(catalog, platform, planned);
    if planned.is_empty() {
        println!("Nothing to run.");
        return Ok(());
    }
    if dry_run {
        println!("Dry run: nothing was changed.");
        return Ok(());
    }
    if preflight::blocked(&findings) {
        bail!("not run: fix what is blocked above first; nothing was changed");
    }
    if !yes && confirm(word)?.trim() != word {
        bail!("cancelled; nothing was changed");
    }
    let this = std::env::current_exe().context("cannot find the HelixForge executable")?;
    let finished = execute(planned, &catalog.lib_dir(), &this, &mut InheritedRunner)?;
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
    let unfinished = finished.unfinished();
    if !unfinished.is_empty() {
        println!();
        for step in &finished.steps {
            println!("{}: {}", step.name, step.state.describe());
        }
        // What to run again: the entries whose steps did not finish.
        let retry: Vec<&str> = unfinished
            .iter()
            .map(|step| step.id.as_str())
            .filter(|id| catalog.find(id).is_some())
            .collect();
        if !retry.is_empty() {
            println!(
                "To try the unfinished ones again: helixforge run {}",
                retry.join(" ")
            );
        }
    }
    if !finished.outcome.succeeded() {
        bail!("the run stopped: {}", finished.describe());
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

/// Print a plan and what the checks before a run found; returns the findings.
fn print_plan(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) -> Vec<preflight::Finding> {
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
    let findings = preflight::check(
        planned,
        platform,
        &|id| catalog.find(id),
        &preflight::Facts::real(),
    );
    for finding in &findings {
        println!("{}", finding.line());
    }
    findings
}
