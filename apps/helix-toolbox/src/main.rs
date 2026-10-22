//! Helix Toolbox: build, configure and optimize a Linux machine from a terminal
//! menu, or from the command line.

mod analyze;
mod bundle;
mod configs;
mod docgen;
mod drift;
mod home;
mod inspect;
mod json;
mod logging;
mod manage;
mod migrate;
mod notify;
mod report;
mod repos;
mod restic;
mod selfupdate;
mod setup;
mod tui;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item, NativeAction, printable};
use helix_toolbox_engine::{
    InheritedRunner, Machine, PackageState, Plan, backups, doctor, execute, history,
    inventory::{Inventory, Status, which},
    package_state, plan, plan_removal, preflight, profiles, run_native, updates,
};
use helix_toolbox_platform::LinuxInfo;
use std::{
    io::{self, BufRead, IsTerminal, Write},
    path::PathBuf,
};
use tui::theme::Theme;

/// The release version: what the release workflow builds with
/// (HELIX_TOOLBOX_VERSION, from scripts/release-version.py), or the workspace
/// version in a local build.
const VERSION: &str = match option_env!("HELIX_TOOLBOX_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

#[derive(Parser)]
#[command(
    name = "helix-toolbox",
    version = VERSION,
    about = "Helix Toolbox: build, configure and optimize"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    /// Read the catalog and scripts from this directory instead of the copy
    /// built into the program; for tests with a fixture catalog.
    #[arg(long, global = true, hide = true, value_name = "DIR")]
    catalog_dir: Option<PathBuf>,
    /// Print the result as JSON, for other programs (see docs/JSON.md): for
    /// system, list, search, info, status, plan, history, updates, doctor,
    /// backups, drift check, drift list, reliability, trust, hardware, setup,
    /// desktop and profile list, and for --dry-run plans.
    #[arg(long, global = true)]
    json: bool,
    /// With plan, or a --dry-run: also show what the plan is expected to
    /// change on this machine: files side by side where their new contents
    /// can be worked out, packages, services, and what cannot be known in
    /// advance.
    #[arg(long, global = true)]
    effects: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Open the terminal interface (the default).
    Tui(TuiArgs),
    /// Show the distribution and package manager Helix Toolbox detects.
    System,
    /// List the catalog as tabs, groups and entries.
    List {
        /// Include entries hidden on this machine.
        #[arg(short = 'u', long)]
        all: bool,
    },
    /// Find entries by name, id or description, closest first; loose
    /// spellings match too.
    Search {
        #[arg(required = true)]
        words: Vec<String>,
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
    Plan {
        ids: Vec<String>,
        /// First download the pinned sources the entries read, such as
        /// dotfiles, into the folder the installers read them from, so
        /// --effects can compare the files they write with the ones here.
        /// Nothing else is changed.
        #[arg(long)]
        fetch_sources: bool,
    },
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
    /// Show pending updates across the package manager, the AUR helper,
    /// Flatpak and Nix, from what each tool already knows; with --apply,
    /// update them through those tools, after confirmation.
    Updates {
        /// The channels to update with --apply (pacman, aur, flatpak-system,
        /// home-manager, …); every channel with updates waiting when none.
        channels: Vec<String>,
        /// Also ask the network, through read-only commands (checkupdates,
        /// the AUR helper, Flatpak's remotes).
        #[arg(long)]
        online: bool,
        /// Update the channels.
        #[arg(long)]
        apply: bool,
        /// With --apply, show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
    /// List configuration backups Helix Toolbox's entries have left, newest
    /// first, and whether snapshots are taken before system changes.
    Backups,
    /// Put a backup back; the current file is kept as a backup first.
    Restore {
        /// The backup's path, as `helix-toolbox backups` lists it.
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
    /// HELIX Drift: save this machine as a baseline, and see how it, or
    /// another machine, differs from one or from a profile: missing
    /// applications, changed configuration files and settings. It only
    /// reads; what would restore the missing parts is printed as a run.
    Drift {
        #[command(subcommand)]
        command: Option<drift::DriftCommand>,
    },
    /// Offline installation bundles: download what entries need, with
    /// Helix Toolbox, into a folder for a machine of the same distribution
    /// without reliable internet; check a bundle; install from one.
    #[command(subcommand)]
    Bundle(bundle::BundleCommand),
    /// The setup wizard: what this machine is, the workstations it offers
    /// (developer, gaming, privacy, minimal, custom), and a named one's plan
    /// for this machine, to review and run with helix-toolbox run. It runs
    /// nothing itself.
    Setup {
        /// The workstation to plan.
        workstation: Option<String>,
    },
    /// The desktop: which one, its session and login screen, its appearance
    /// settings and who else sets them (Home Manager, Stylix), and the
    /// entries for it; save its look, or put a saved look back.
    Desktop {
        #[command(subcommand)]
        command: Option<setup::DesktopCommand>,
    },
    /// The machine's and your own systemd services, timers and sockets:
    /// list them, see what needs one, and start, stop, restart, enable or
    /// disable one after confirmation, told first what stopping it costs.
    Services {
        #[command(subcommand)]
        action: Option<manage::ServicesCommand>,
    },
    /// The applications the desktop starts at login (XDG autostart), and
    /// turning one off or on again for you.
    Startup {
        #[command(subcommand)]
        action: Option<manage::StartupCommand>,
    },
    /// Where the disk space goes and what can be cleaned, with what each
    /// frees; each cleanup after confirmation. Snapshots, bootable kernels
    /// and configuration backups are never cleaned.
    Storage {
        #[command(subcommand)]
        action: Option<manage::StorageCommand>,
    },
    /// How long the boot took and where the time went, how the machine is
    /// doing now, and this boot beside an earlier one with Helix Toolbox's
    /// runs in between. It only reads; each boot it sees is recorded, so
    /// later boots can be compared with it.
    Performance {
        #[command(subcommand)]
        action: Option<analyze::PerformanceCommand>,
    },
    /// Recovery readiness: snapshots and whether the boot menu can start
    /// one, a second kernel, configuration backups, a backup of your own
    /// files and a Drift baseline. It only reads; --check-snapshots lists
    /// the snapshots as root and looks inside the newest, changing nothing.
    Recovery {
        /// List the snapshots as root and check the newest holds a system;
        /// confirmed by typing CHECK.
        #[arg(long)]
        check_snapshots: bool,
        /// With --check-snapshots: show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// With --check-snapshots: skip the confirmation phrase.
        #[arg(long)]
        yes: bool,
    },
    /// Move to another machine or distribution: export what this one has to
    /// a file, then on the other one see what it means there and install
    /// it. Catalog entries and Flatpaks translate; the other packages you
    /// installed are listed for you when the families differ.
    Migrate {
        #[command(subcommand)]
        action: migrate::MigrateCommand,
    },
    /// Restic backups of your own files: the repository and its snapshots,
    /// the files in one, a file's history, and checks, backups, restores
    /// into a new folder and recovery tests, each a plan you confirm.
    Restic {
        #[command(subcommand)]
        action: Option<restic::ResticCommand>,
    },
    /// Home Manager: the configuration, its generations and which is
    /// active, its Git repository, and the files it links. Reading changes
    /// nothing; building, switching, going back to a generation and
    /// pulling the repository are plans, confirmed first.
    HomeManager {
        #[command(subcommand)]
        action: Option<home::HomeManagerCommand>,
    },
    /// The Git repositories under your project folders: their branch,
    /// changes and upstream, one repository's files, diffs, branches and
    /// stashes, and its tasks. Reading changes nothing; staging, commits,
    /// branches, stashes, syncing and tasks are plans, confirmed first.
    Repos {
        #[command(subcommand)]
        action: Option<repos::ReposCommand>,
    },
    /// Ghostty's and Fastfetch's configuration: which file is edited, its
    /// checks, a few common fields, your editor on a copy and the backups
    /// kept. Saving is a plan, confirmed first, that keeps the current file
    /// as a backup.
    Config {
        program: configs::Program,
        #[command(subcommand)]
        action: Option<configs::ConfigCommand>,
    },
    /// The hardware and firmware: fwupd's devices and the firmware updates it
    /// knows of, disks, temperatures, batteries, PCI and USB devices. It only
    /// reads; --smart and --firmware run a plan of their own, after
    /// confirmation.
    Hardware {
        /// Read this disk's SMART health (nvme0n1, sda), which needs root.
        #[arg(long, value_name = "DISK", conflicts_with = "firmware")]
        smart: Option<String>,
        /// Update this device's firmware through fwupd (its id, or its name
        /// as listed); confirmed by typing FIRMWARE.
        #[arg(long, value_name = "DEVICE")]
        firmware: Option<String>,
        /// With --smart or --firmware: show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
    /// Check that entries left the machine as they should, read-only: what
    /// each installs and its checks of files, settings and services. Checks
    /// that hold only after a reboot are made once the machine has started
    /// since the entry last ran. Exits 1 when one fails.
    Verify {
        #[arg(required = true)]
        ids: Vec<String>,
    },
    /// How far each entry has been tested: in distribution containers, with
    /// stand-in commands, on virtual or real machines, or not yet; and its
    /// last successful run on this machine.
    Reliability {
        /// The entries to show; every entry when none.
        ids: Vec<String>,
        /// Only the entries whose failure can stop a machine booting, logging
        /// in or showing a display, and whole-system changes.
        #[arg(long)]
        critical: bool,
    },
    /// Where an entry's installer comes from (repository, revision, checksum,
    /// license), what checks the packages it installs, what it downloads
    /// unchecked, the privileges it uses and how far it has been tested.
    Trust { id: String },
    /// Your workstation profiles: list, export, import or delete them. Run one
    /// like any entry: helix-toolbox run profile-<name>.
    #[command(subcommand)]
    Profile(ProfileCommand),
    /// Tell you on the desktop when updates are waiting, and if you choose
    /// when Helix Doctor finds a new problem or the machine drifts from a
    /// baseline: a systemd user timer checks every 6 hours and shows a
    /// notification when something is new. It never updates, repairs or
    /// puts back anything itself. Without a subcommand, says what is on.
    Notifications {
        #[command(subcommand)]
        action: Option<NotificationsCommand>,
    },
    /// Show recent runs.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Print what a bug report needs, to paste into an issue: version, how
    /// it was installed, the system, terminal, settings, recent runs and the
    /// health checks. It only reads, and shows the home directory as ~.
    Report {
        /// Leave out the health checks, which take a few seconds.
        #[arg(long, conflicts_with = "worked")]
        no_checks: bool,
        /// Report instead that this entry worked on this machine, for its
        /// Hardware Tested record: from its last successful run here, with a
        /// link that opens the issue filled in. Nothing is sent by itself.
        #[arg(long, value_name = "ENTRY")]
        worked: Option<String>,
        /// With --worked: what you checked, such as "the login screen came up
        /// after a reboot".
        #[arg(long, requires = "worked", default_value = "")]
        note: String,
    },
    /// Write the catalog walkthrough as Markdown.
    Docgen { file: PathBuf },
    /// Print shell completions: helix-toolbox completions fish >
    /// ~/.config/fish/completions/helix-toolbox.fish (the packages install them).
    Completions { shell: clap_complete::Shell },
    /// Write the manual pages into a directory; the packages are built with it.
    #[command(hide = true)]
    Manpages { directory: PathBuf },
    /// Download one of a release's files and check it against the release's
    /// SHA256SUMS; the Helix Toolbox update channel runs it.
    #[command(hide = true)]
    FetchRelease {
        version: String,
        asset: String,
        directory: PathBuf,
    },
    /// Run a native operation; the engine calls this from a run.
    #[command(hide = true)]
    Native { action: String },
}

#[derive(Subcommand)]
enum ProfileCommand {
    /// List your profiles and what each runs.
    List,
    /// Write a profile to a file to share or keep.
    Export {
        /// The profile's name, or its id (profile-<name>).
        name: String,
        /// Where to write it; the current directory and its name when left out.
        file: Option<PathBuf>,
    },
    /// Add a profile from a file, after checking every entry it names exists.
    Import { file: PathBuf },
    /// Delete one of your profiles.
    Delete { name: String },
}

#[derive(Subcommand)]
enum NotificationsCommand {
    /// Tell of updates waiting: writes the timer into your systemd user
    /// folder and starts it, if it is not there yet.
    On,
    /// Stop telling of anything: stops the timer and removes it.
    Off,
    /// Also tell of new problems Helix Doctor's health checks find (the
    /// network and DNS checks, which come and go, are left out), or not.
    Health { switch: Switch },
    /// Also tell of new ways this machine differs from a saved Drift
    /// baseline, or "off" to stop.
    Drift { baseline: String },
    /// Look now and tell of what is new; the timer runs this.
    Check,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum Switch {
    On,
    Off,
}

#[derive(clap::Args, Clone, Debug, Default)]
pub struct TuiArgs {
    /// Color theme for this run: solid (dark panels, the default),
    /// transparent (over the terminal's background), or compatible (sixteen
    /// colors). Settings in the menu changes the remembered one.
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
    /// Open on this screen, such as updates, doctor, drift, plan or an
    /// area's id (applications); on the first start the setup wizard opens
    /// over it.
    #[arg(long, value_name = "SCREEN")]
    pub screen: Option<String>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    // Before anything reads them: the folders named for HelixForge.
    for problem in helix_toolbox_engine::legacy::move_old_folders() {
        eprintln!("Helix Toolbox: {problem}");
    }
    logging::init();
    if !matches!(cli.command, None | Some(Command::Tui(_))) {
        // Output piped into head or similar ends quietly when the reader
        // stops, as other command-line tools do, instead of with an error.
        // SAFETY: the default action, set before any thread starts.
        let _ = unsafe {
            nix::sys::signal::signal(
                nix::sys::signal::Signal::SIGPIPE,
                nix::sys::signal::SigHandler::SigDfl,
            )
        };
    }
    let platform = LinuxInfo::detect().context("Helix Toolbox needs Linux with /etc/os-release")?;
    let source = cli.catalog_dir;
    let as_json = cli.json;
    JSON.store(as_json, std::sync::atomic::Ordering::Relaxed);
    EFFECTS.store(cli.effects, std::sync::atomic::Ordering::Relaxed);
    let load = |validate: bool| -> Result<Catalog> {
        let mut catalog = match &source {
            Some(directory) => Catalog::from_directory(directory, validate)
                .with_context(|| format!("cannot load the catalog in {}", directory.display()))?,
            None => Catalog::builtin(validate).context("cannot load the catalog")?,
        };
        // The user's own profiles join the Profiles area.
        if let Some(directory) = profiles::dir() {
            for problem in profiles::add_to(&mut catalog, &directory) {
                eprintln!("Profile skipped: {}", printable(&problem));
            }
        }
        Ok(catalog)
    };
    match cli.command.unwrap_or(Command::Tui(TuiArgs::default())) {
        Command::Tui(_)
        | Command::Docgen { .. }
        | Command::Native { .. }
        | Command::Completions { .. }
        | Command::Manpages { .. }
        | Command::FetchRelease { .. }
        | Command::Notifications { .. }
            if as_json =>
        {
            bail!(
                "--json is for system, list, search, info, status, plan, history, updates, doctor, backups, report, drift check, drift list, reliability, trust, hardware, setup, desktop, bundle verify and profile list, and for --dry-run plans"
            )
        }
        Command::Tui(args) => {
            // Keys are read from the controlling terminal, not standard
            // input, so `curl … | sh` still works.
            if !io::stdout().is_terminal() || std::fs::File::open("/dev/tty").is_err() {
                bail!(
                    "the menu needs a terminal; run helix-toolbox from one, or see helix-toolbox --help for the commands that work without it"
                );
            }
            tui::run(load(!args.override_validation)?, platform, args)
        }
        Command::System if as_json => {
            json::print("system", json::system(&platform));
            Ok(())
        }
        Command::System => {
            print!("{}", run_native(NativeAction::SystemReport, &platform));
            Ok(())
        }
        Command::List { all } => {
            let catalog = load(!all)?;
            if as_json {
                json::print("list", json::list(&catalog));
                return Ok(());
            }
            for tab in &catalog.tabs {
                println!("{}", tab.name);
                list_items(&tab.items, 1);
            }
            Ok(())
        }
        Command::Search { words, all } => {
            let catalog = load(!all)?;
            let query = words.join(" ").to_lowercase();
            let mut found: Vec<(u32, &Entry)> = catalog
                .entries()
                .into_iter()
                .filter_map(|entry| {
                    helix_toolbox_core::search::score(
                        &query,
                        &entry.name,
                        &entry.id,
                        &entry.description,
                    )
                    .map(|score| (score, entry))
                })
                .collect();
            found.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
            });
            if as_json {
                json::print("search", json::search(&query, &found));
                return Ok(());
            }
            if found.is_empty() {
                bail!("nothing matches \"{query}\"");
            }
            let width = found
                .iter()
                .map(|(_, entry)| entry.id.len())
                .max()
                .unwrap_or(0);
            for (_, entry) in found {
                let summary = entry.description.lines().next().unwrap_or_default();
                println!("{:width$}  {}  {summary}", entry.id, entry.name);
            }
            Ok(())
        }
        Command::Info { id } => {
            let catalog = load(false)?;
            let entry = find(&catalog, &id)?;
            if as_json {
                let status = entry
                    .identity()
                    .and_then(|_| Inventory::gather(&platform).status(entry, &platform, &which));
                json::print("info", json::info(entry, status.as_ref()));
                return Ok(());
            }
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
                        println!("Removable: helix-toolbox remove {}", entry.id);
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
            if as_json {
                let found: Vec<(&Entry, Option<Status>)> = entries
                    .into_iter()
                    .map(|entry| (entry, inventory.status(entry, &platform, &which)))
                    .collect();
                json::print("status", json::statuses(&found));
                return Ok(());
            }
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
        Command::Plan { ids, fetch_sources } => {
            let catalog = load(false)?;
            let planned = plan_ids(&catalog, &platform, &ids)?;
            if fetch_sources {
                let scripts = catalog.root().join("scripts");
                for name in helix_toolbox_engine::preview::sources_for(&planned) {
                    let folder = helix_toolbox_engine::preview::fetch_source(&scripts, name)
                        .map_err(|why| anyhow::anyhow!("cannot download {name}: {why}"))?;
                    eprintln!("Source: {name} in {}", folder.display());
                }
            }
            if as_json {
                print_plan_json(&catalog, &platform, &planned);
            } else {
                print_plan(&catalog, &platform, &planned);
            }
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
                if as_json {
                    return confirm_and_run(
                        &catalog,
                        &platform,
                        &Plan::default(),
                        dry_run,
                        yes,
                        "UPDATE",
                    );
                }
                println!(
                    "No package entry has an update in the repository metadata on this machine."
                );
                return Ok(());
            }
            let mut planned = inventory.plan_update(entries, &platform, &which);
            planned.keep_going = keep_going;
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "UPDATE")
        }
        Command::Updates {
            channels: wanted,
            online,
            apply,
            dry_run,
            yes,
        } => {
            let found = updates::overview(&doctor::Here, &platform, &this_places(), online);
            if !apply {
                if !wanted.is_empty() {
                    bail!("name channels only with --apply");
                }
                if as_json {
                    json::print("updates", json::updates(&found, online));
                    return Ok(());
                }
                if found.is_empty() {
                    println!(
                        "No package manager, Flatpak or Nix setup Helix Toolbox knows was found."
                    );
                }
                for channel in &found {
                    println!(
                        "{} [{}]: {}",
                        printable(&channel.name),
                        channel.id,
                        channel.summary()
                    );
                    println!("  {}", printable(&channel.note));
                    for pending in channel.pending.iter().flatten() {
                        let versions = match (&pending.from, &pending.to) {
                            (Some(from), Some(to)) => format!("{from} -> {to}"),
                            (None, Some(to)) => to.clone(),
                            _ => String::new(),
                        };
                        println!("    {} {}", printable(&pending.name), printable(&versions));
                    }
                    if channel.commands.is_empty() {
                        println!("  Update: nothing to run");
                    } else {
                        println!("  Update: {}", channel.command_line());
                    }
                    if let Some(caveat) = &channel.caveat {
                        println!("  Note: {}", printable(caveat));
                    }
                }
                return Ok(());
            }
            let chosen: Vec<&updates::Channel> = if wanted.is_empty() {
                found
                    .iter()
                    .filter(|channel| channel.count().is_some_and(|count| count > 0))
                    .filter(|channel| !channel.commands.is_empty())
                    .collect()
            } else {
                wanted
                    .iter()
                    .map(|id| {
                        found
                            .iter()
                            .find(|channel| channel.id == *id)
                            .ok_or_else(|| {
                                let known: Vec<&str> =
                                    found.iter().map(|channel| channel.id.as_str()).collect();
                                anyhow::anyhow!(
                                    "no update channel \"{}\" here; there are: {}",
                                    printable(id),
                                    known.join(", ")
                                )
                            })
                    })
                    .collect::<Result<_>>()?
            };
            if chosen.is_empty() && as_json {
                if !dry_run {
                    bail!("--json shows plans only: add --dry-run");
                }
                json::print("plan", json::plan(&Plan::default(), &[]));
                return Ok(());
            }
            if chosen.is_empty() {
                println!(
                    "Nothing is known to wait. Name a channel to update it anyway (Nix counts nothing before it updates), or check with --online."
                );
                return Ok(());
            }
            let catalog = load(false)?;
            confirm_and_run(
                &catalog,
                &platform,
                &updates::plan(&chosen),
                dry_run,
                yes,
                "UPDATE",
            )
        }
        Command::Doctor {
            repair,
            dry_run,
            yes,
        } => {
            let found = doctor::examine(&doctor::Here, platform.manager);
            if as_json && repair.is_none() {
                json::print("doctor", json::doctor(&found));
                return Ok(());
            }
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
                                "           Repair: {} (helix-toolbox doctor --repair {})",
                                fix.label, diagnosis.id
                            );
                        }
                        if let Some(entry) = &diagnosis.entry {
                            println!("           Entry: helix-toolbox run {entry}");
                        }
                    }
                }
                return Ok(());
            };
            let diagnosis = found
                .iter()
                .find(|diagnosis| diagnosis.id == id)
                .with_context(|| format!("no check named '{id}'; see helix-toolbox doctor"))?;
            let fix = diagnosis.repair.as_ref().with_context(|| {
                format!("{} has no repair Helix Toolbox can run", diagnosis.title)
            })?;
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
        Command::Backups if as_json => {
            json::print(
                "backups",
                json::backups(
                    backups::Snapshots::here(),
                    &backups::discover(&backups::Roots::here()),
                ),
            );
            Ok(())
        }
        Command::Backups => {
            match backups::Snapshots::here() {
                Some(tool) => println!(
                    "Snapshots: {} is set up; run --snapshot and the menu take one before system changes. Roll back with {}. Helix Toolbox does not roll back packages itself.\n",
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
            // Only a backup Helix Toolbox finds can be restored, never any path.
            let found = backups::discover(&backups::Roots::here())
                .into_iter()
                .find(|known| known.backup == backup)
                .with_context(|| {
                    format!(
                        "{} is not a backup Helix Toolbox knows; see helix-toolbox backups",
                        backup.display()
                    )
                })?;
            let planned = backups::plan_restore(&found);
            confirm_and_run(&catalog, &platform, &planned, dry_run, yes, "RESTORE")
        }
        Command::Drift { command } => drift::run(command, &load(false)?, &platform, as_json),
        Command::Bundle(command) => bundle::run(command, &load(false)?, &platform, as_json),
        Command::Setup { workstation } => {
            setup::run_setup(workstation, &load(true)?, &platform, as_json)
        }
        Command::Desktop { command } => {
            setup::run_desktop(command, &load(true)?, &platform, as_json)
        }
        Command::Services { action } => manage::services(action, &load(false)?, &platform, as_json),
        Command::Startup { action } => manage::startup(action, &load(false)?, &platform, as_json),
        Command::Storage { action } => manage::storage(action, &load(false)?, &platform, as_json),
        Command::Performance { action } => analyze::performance(action, as_json),
        Command::Recovery {
            check_snapshots,
            dry_run,
            yes,
        } => analyze::recovery(
            check_snapshots,
            dry_run,
            yes,
            &load(false)?,
            &platform,
            as_json,
        ),
        Command::Migrate { action } => migrate::migrate(action, &load(false)?, &platform, as_json),
        Command::Restic { action } => restic::restic(action, &load(false)?, &platform, as_json),
        Command::Repos { action } => repos::repos(action, &load(false)?, &platform, as_json),
        Command::Config { program, action } => {
            configs::config(program, action, &load(false)?, &platform, as_json)
        }
        Command::HomeManager { action } => {
            home::home_manager(action, &load(false)?, &platform, as_json)
        }
        Command::Hardware {
            smart,
            firmware,
            dry_run,
            yes,
        } => {
            let found = helix_toolbox_engine::hardware::gather(&doctor::Here);
            if let Some(disk) = smart {
                let planned =
                    helix_toolbox_engine::hardware::plan_smart(&disk).with_context(|| {
                        format!(
                            "'{}' is not a disk name such as nvme0n1 or sda",
                            printable(&disk)
                        )
                    })?;
                return confirm_and_run(&load(false)?, &platform, &planned, dry_run, yes, "CHECK");
            }
            if let Some(device) = firmware {
                let update = found
                    .firmware
                    .updates
                    .iter()
                    .find(|update| {
                        update.device_id == device || update.device.eq_ignore_ascii_case(&device)
                    })
                    .with_context(|| {
                        format!(
                            "fwupd knows no firmware update for '{}'; see helix-toolbox hardware",
                            printable(&device)
                        )
                    })?;
                let planned = helix_toolbox_engine::hardware::plan_firmware(update)
                    .context("fwupd gave that device an id Helix Toolbox does not accept")?;
                return confirm_and_run(
                    &load(false)?,
                    &platform,
                    &planned,
                    dry_run,
                    yes,
                    "FIRMWARE",
                );
            }
            if as_json {
                json::print("hardware", serde_json::to_value(&found).unwrap_or_default());
                return Ok(());
            }
            print!("{}", hardware_text(&found));
            Ok(())
        }
        Command::Verify { ids } => inspect::verify(&load(false)?, &platform, &ids, as_json),
        Command::Reliability { ids, critical } => {
            inspect::reliability(&load(false)?, &ids, critical, as_json)
        }
        Command::Trust { id } => inspect::trust(&load(false)?, &platform, &id, as_json),
        Command::Profile(command) => {
            let catalog = load(false)?;
            let directory = profiles::dir().context("cannot find the configuration folder")?;
            let (found, _) = profiles::load_all(&directory, &catalog);
            let named = |name: &str| {
                let wanted = name.strip_prefix(profiles::PREFIX).unwrap_or(name);
                let wanted = profiles::slug(wanted).unwrap_or_default();
                found
                    .iter()
                    .find(|(slug, _)| *slug == wanted)
                    .cloned()
                    .with_context(|| {
                        format!("no profile named '{name}'; see helix-toolbox profile list")
                    })
            };
            match command {
                ProfileCommand::List if as_json => json::print("profiles", json::profiles(&found)),
                _ if as_json => {
                    bail!("--json lists profiles only: helix-toolbox profile list --json")
                }
                ProfileCommand::List => {
                    if found.is_empty() {
                        println!(
                            "No profiles yet. In the menu's Install Plan, s saves what is queued as one; or import one with helix-toolbox profile import <file>."
                        );
                    }
                    for (slug, profile) in &found {
                        println!(
                            "{}{slug}  {}\n    {}",
                            profiles::PREFIX,
                            printable(&profile.name),
                            profile.entries.join(", ")
                        );
                    }
                }
                ProfileCommand::Export { name, file } => {
                    let (slug, profile) = named(&name)?;
                    let file = file.unwrap_or_else(|| PathBuf::from(format!("{slug}.toml")));
                    profiles::write(&file, &profile)?;
                    println!(
                        "Exported {} to {}",
                        printable(&profile.name),
                        file.display()
                    );
                }
                ProfileCommand::Import { file } => {
                    let profile = profiles::read(&file, &catalog)?;
                    let saved = profiles::save(&directory, &profile, &catalog)?;
                    println!(
                        "Imported {} ({} entries) as {}; run it with helix-toolbox run {}{}",
                        printable(&profile.name),
                        profile.entries.len(),
                        saved.display(),
                        profiles::PREFIX,
                        saved.file_stem().unwrap_or_default().to_string_lossy()
                    );
                }
                ProfileCommand::Delete { name } => {
                    let (slug, profile) = named(&name)?;
                    let path = directory.join(format!("{slug}.toml"));
                    std::fs::remove_file(&path)
                        .with_context(|| format!("cannot delete {}", path.display()))?;
                    println!("Deleted {}", printable(&profile.name));
                }
            }
            Ok(())
        }
        Command::History { limit } => {
            let records = history::read(limit)?;
            if as_json {
                json::print("history", json::history(&records));
                return Ok(());
            }
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
        Command::Notifications { action } => match action {
            None => notify::status(),
            Some(NotificationsCommand::On) => notify::change(|watch| watch.updates = true),
            Some(NotificationsCommand::Off) => notify::off(),
            Some(NotificationsCommand::Health { switch }) => {
                notify::change(|watch| watch.health = matches!(switch, Switch::On))
            }
            Some(NotificationsCommand::Drift { baseline }) => {
                let slug = match baseline.as_str() {
                    "off" => None,
                    name => Some(drift::saved(name)?.0),
                };
                notify::change(|watch| watch.drift = slug)
            }
            Some(NotificationsCommand::Check) => notify::check(&load(false)?, &platform),
        },
        Command::Report {
            worked: Some(id),
            note,
            ..
        } => {
            let found = report::worked(&load(false)?, &platform, &id, &note)?;
            if as_json {
                json::print(
                    "hardware_report",
                    serde_json::json!({
                        "entries": found.entries.iter().map(|(id, name)| serde_json::json!({
                            "id": id,
                            "name": name,
                        })).collect::<Vec<_>>(),
                        "machine": found.machine,
                        "system": found.machine.system(),
                        "version": found.version,
                        "date": found.date,
                        "records": found.claims,
                        "toml": found.records(),
                        "issue": found.issue_url(),
                    }),
                );
            } else {
                println!(
                    "{}\nOpen this to send it; GitHub asks you to sign in, and the form asks what you checked:\n\n{}",
                    found.text(),
                    found.issue_url()
                );
            }
            Ok(())
        }
        Command::Report { no_checks, .. } => {
            let found = report::gather(&platform, !no_checks);
            if as_json {
                json::print("report", found);
            } else {
                print!("{}", report::markdown(&found));
            }
            Ok(())
        }
        Command::FetchRelease {
            version,
            asset,
            directory,
        } => {
            selfupdate::fetch(&version, &asset, &directory, &selfupdate::base(), "=https")?;
            Ok(())
        }
        Command::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut <Cli as clap::CommandFactory>::command(),
                "helix-toolbox",
                &mut io::stdout(),
            );
            Ok(())
        }
        Command::Manpages { directory } => {
            std::fs::create_dir_all(&directory)
                .with_context(|| format!("cannot create {}", directory.display()))?;
            clap_mangen::generate_to(<Cli as clap::CommandFactory>::command(), &directory)
                .with_context(|| {
                    format!("cannot write the manual pages in {}", directory.display())
                })?;
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
        planned.notes.push(
            "No snapshot: this run does not change system files, the kernel or disks.".into(),
        );
        return;
    }
    match backups::Snapshots::here() {
        Some(tool) => backups::with_snapshot(planned, tool),
        None => planned
            .notes
            .push("No snapshot: neither Snapper nor Timeshift is set up here.".into()),
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
    if JSON.load(std::sync::atomic::Ordering::Relaxed) {
        if !dry_run {
            bail!("--json shows plans only: add --dry-run; nothing was changed");
        }
        print_plan_json(catalog, platform, planned);
        return Ok(());
    }
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
        bail!("canceled; nothing was changed");
    }
    let this = std::env::current_exe().context("cannot find the Helix Toolbox executable")?;
    let finished = execute(planned, &catalog.lib_dir(), &this, &mut InheritedRunner)?;
    for check in &finished.checks {
        println!("{}: {}: {}", check.label(), check.name, check.detail);
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
                "To try the unfinished ones again: helix-toolbox run {}",
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
        .with_context(|| format!("no entry named '{id}'; see helix-toolbox list"))
}

fn plan_ids(catalog: &Catalog, platform: &LinuxInfo, ids: &[String]) -> Result<Plan> {
    let mut entries = Vec::new();
    for id in ids {
        entries.push(find(catalog, id)?);
    }
    Ok(plan(entries, platform, &|id| catalog.find(id))?)
}

/// Where the update overview looks, with this Helix Toolbox for its own update.
pub fn this_places() -> updates::Places {
    let places = updates::Places::here();
    match std::env::current_exe() {
        Ok(exe) => places.with_this(VERSION, exe),
        Err(_) => places,
    }
}

/// A moment as a date and time in UTC, without a time library.
pub fn when(unix: u64) -> String {
    let days = unix / 86_400;
    let seconds = unix % 86_400;
    // Civil-from-days (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        seconds / 3600,
        (seconds % 3600) / 60
    )
}

/// The hardware center as text.
fn hardware_text(found: &helix_toolbox_engine::hardware::Hardware) -> String {
    use helix_toolbox_engine::hardware::size;
    let mut text = String::from("Firmware\n");
    if let Some(note) = &found.firmware.note {
        text.push_str(&format!("  {note}\n"));
    }
    if found.firmware.available {
        if found.firmware.updates.is_empty() {
            text.push_str("  No firmware updates in the metadata fwupd has (fwupdmgr refresh fetches newer).\n");
        }
        for update in &found.firmware.updates {
            text.push_str(&format!(
                "  Update: {} {} -> {}{}\n    helix-toolbox hardware --firmware {}\n",
                update.device,
                update.from.as_deref().unwrap_or("?"),
                update.to,
                update
                    .summary
                    .as_deref()
                    .map(|summary| format!(": {summary}"))
                    .unwrap_or_default(),
                update.device_id
            ));
        }
        let updatable = found
            .firmware
            .devices
            .iter()
            .filter(|device| device.updatable)
            .count();
        text.push_str(&format!(
            "  {} devices, {updatable} of them updatable through fwupd\n",
            found.firmware.devices.len()
        ));
    }
    text.push_str("\nDisks\n");
    for disk in &found.disks {
        text.push_str(&format!(
            "  {:<10} {:<5} {:<9} {}{}{}\n",
            disk.name,
            disk.kind,
            disk.bytes.map(size).unwrap_or_default(),
            disk.model.as_deref().unwrap_or("unknown model"),
            disk.firmware
                .as_deref()
                .map(|firmware| format!(", firmware {firmware}"))
                .unwrap_or_default(),
            disk.celsius
                .map(|celsius| format!(", {celsius:.0} °C"))
                .unwrap_or_default()
        ));
    }
    if !found.disks.is_empty() {
        text.push_str("  SMART health needs root: helix-toolbox hardware --smart <disk>\n");
    }
    text.push_str("\nTemperatures\n");
    for sensor in &found.sensors {
        text.push_str(&format!(
            "  {:<12} {:<24} {:>5.1} °C{}{}\n",
            sensor.chip,
            sensor.label,
            sensor.celsius,
            sensor
                .critical
                .map(|critical| format!("  (critical {critical:.0})"))
                .unwrap_or_default(),
            if sensor.hot() { "  HOT" } else { "" }
        ));
    }
    if !found.batteries.is_empty() {
        text.push_str("\nBatteries\n");
    }
    for battery in &found.batteries {
        text.push_str(&format!(
            "  {}: {}{}{}{}\n",
            battery.name,
            battery
                .capacity
                .map(|capacity| format!("{capacity}% charged"))
                .unwrap_or_else(|| "charge unknown".to_string()),
            battery
                .health
                .map(|health| format!(", holds {health}% of its design capacity"))
                .unwrap_or_default(),
            battery
                .cycles
                .map(|cycles| format!(", {cycles} cycles"))
                .unwrap_or_default(),
            battery
                .status
                .as_deref()
                .map(|status| format!(" ({status})"))
                .unwrap_or_default()
        ));
    }
    text.push_str("\nPCI devices\n");
    for device in &found.pci {
        text.push_str(&format!(
            "  {} {}{}\n",
            device.address,
            device.name,
            device
                .driver
                .as_deref()
                .map(|driver| format!("  [{driver}]"))
                .unwrap_or_default()
        ));
    }
    text.push_str("\nUSB devices\n");
    for device in &found.usb {
        text.push_str(&format!("  {} {}\n", device.address, device.name));
    }
    text
}

/// Whether --json was given, for the commands that confirm and run plans.
static JSON: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether --effects was given: plans show their change preview.
static EFFECTS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// What a plan is expected to change on this machine.
fn effects(
    catalog: &Catalog,
    platform: &LinuxInfo,
    planned: &Plan,
) -> Vec<helix_toolbox_engine::preview::StepPreview> {
    let scripts = catalog.root().join("scripts");
    let render =
        |arguments: &[String]| helix_toolbox_engine::preview::render_with(&scripts, arguments);
    helix_toolbox_engine::preview::preview(
        planned,
        &|id| catalog.find(id),
        platform,
        &helix_toolbox_engine::preview::Look {
            system: &doctor::Here,
            render: &render,
            pinned: &|name| helix_toolbox_engine::preview::pinned_source(&scripts, name),
        },
    )
}

/// A plan and what the checks before a run found, as JSON.
fn print_plan_json(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) {
    let findings = preflight::check(
        planned,
        platform,
        &|id| catalog.find(id),
        &preflight::Facts::real(),
    );
    let mut value = json::plan(planned, &findings);
    if EFFECTS.load(std::sync::atomic::Ordering::Relaxed) {
        value["effects"] = serde_json::to_value(effects(catalog, platform, planned))
            .unwrap_or(serde_json::Value::Null);
    }
    json::print("plan", value);
}

/// Print a plan and what the checks before a run found; returns the findings.
fn print_plan(catalog: &Catalog, platform: &LinuxInfo, planned: &Plan) -> Vec<preflight::Finding> {
    for warning in &planned.warnings {
        println!("Warning: {warning}");
    }
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
    if EFFECTS.load(std::sync::atomic::Ordering::Relaxed) && !planned.is_empty() {
        println!("\nWhat it changes (✓ known, ~ expected, ? not known in advance):");
        for step in effects(catalog, platform, planned) {
            println!("{}", step.name);
            for change in step.changes {
                println!("  {} {}", change.certainty.mark(), change.text);
                if let Some(diff) = change.diff.filter(|diff| diff.changed) {
                    for line in diff.unified().lines() {
                        println!("      {line}");
                    }
                }
            }
        }
        println!();
    }
    let evidence = helix_toolbox_engine::reliability::Evidence::load(catalog.root());
    for warning in helix_toolbox_engine::reliability::warnings(
        planned.steps.iter().map(|step| step.id()),
        &|id| catalog.find(id),
        &evidence,
    ) {
        println!("Validation: {warning}");
    }
    findings
}
