//! `helix-toolbox services`, `startup` and `storage`: the service and
//! startup manager and the storage and cleanup center on the command line
//! (helix_toolbox_engine::services and storage). Listing reads only; every
//! change is a plan, shown and confirmed first.

use anyhow::{Result, bail};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{
    doctor,
    services::{self, Verb},
    storage,
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;

#[derive(Subcommand)]
pub enum ServicesCommand {
    /// List the machine's and your own units: services, timers and sockets
    /// with whether each is enabled and running (the default).
    List {
        /// Only units whose name or description has this.
        filter: Option<String>,
        /// Only your own session's units.
        #[arg(long)]
        user: bool,
        /// Only units that failed.
        #[arg(long)]
        failed: bool,
    },
    /// A unit in full: its state, what needs it, and what stopping or
    /// disabling it costs.
    Show {
        unit: String,
        #[arg(long)]
        user: bool,
    },
    /// Start a unit now.
    Start(Change),
    /// Stop a unit now.
    Stop(Change),
    /// Restart a unit now.
    Restart(Change),
    /// Start a unit at every boot (or, with --user, login), from the next.
    Enable(Change),
    /// Stop starting a unit at boot or login, from the next.
    Disable(Change),
}

#[derive(clap::Args)]
pub struct Change {
    unit: String,
    /// Your own session's unit, not the machine's.
    #[arg(long)]
    user: bool,
    /// Show the plan and stop.
    #[arg(long)]
    dry_run: bool,
    /// Skip the confirmation phrase, for deliberate automation.
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand)]
pub enum StartupCommand {
    /// The applications the desktop starts at login (the default).
    List,
    /// Start an application at login again.
    On(Switch),
    /// Do not start an application at login, for you only.
    Off(Switch),
}

#[derive(clap::Args)]
pub struct Switch {
    /// Its autostart file's name, such as org.kde.discover.notifier.desktop.
    file: String,
    #[arg(long)]
    dry_run: bool,
    #[arg(long)]
    yes: bool,
}

#[derive(Subcommand)]
pub enum StorageCommand {
    /// Where the space goes and what can be cleaned (the default).
    Show,
    /// Clean one item: package-cache, unused-packages, flatpak-unused,
    /// journal, trash, thumbnails or aur-builds.
    Clean {
        item: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
}

fn desktop() -> Option<String> {
    std::env::var("XDG_CURRENT_DESKTOP").ok()
}

fn ssh() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some()
}

fn state(unit: &services::Unit) -> String {
    format!(
        "{}, {}",
        unit.enablement.as_deref().unwrap_or("no unit file"),
        match (&unit.active, &unit.sub) {
            (Some(active), Some(sub)) if active != sub => format!("{active} ({sub})"),
            (Some(active), _) => active.clone(),
            _ => "not loaded".to_string(),
        }
    )
}

pub fn services(
    command: Option<ServicesCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let found = services::gather(&doctor::Here, desktop().as_deref());
    let command = command.unwrap_or(ServicesCommand::List {
        filter: None,
        user: false,
        failed: false,
    });
    let change = |change: Change, verb: Verb| -> Result<()> {
        if !found
            .units
            .iter()
            .any(|unit| unit.name == change.unit && unit.user == change.user)
        {
            bail!(
                "no {}unit named {}; see helix-toolbox services list",
                if change.user { "user " } else { "" },
                change.unit
            );
        }
        let needed = services::needed_by(&doctor::Here, &change.unit, change.user);
        let planned = services::plan(&change.unit, change.user, verb, &needed, ssh())
            .map_err(anyhow::Error::msg)?;
        crate::confirm_and_run(
            catalog,
            platform,
            &planned,
            change.dry_run,
            change.yes,
            "YES",
        )
    };
    match command {
        ServicesCommand::List {
            filter,
            user,
            failed,
        } => {
            if !found.systemd {
                bail!("systemd does not run this machine, so there are no units to list");
            }
            let lowered = filter.as_deref().map(str::to_lowercase);
            let shown: Vec<&services::Unit> = found
                .units
                .iter()
                .filter(|unit| !user || unit.user)
                .filter(|unit| !failed || unit.failed())
                .filter(|unit| {
                    lowered.as_ref().is_none_or(|filter| {
                        unit.name.to_lowercase().contains(filter)
                            || unit.description.to_lowercase().contains(filter)
                    })
                })
                .collect();
            if as_json {
                crate::json::print(
                    "services",
                    json!({ "units": shown, "session": found.session }),
                );
                return Ok(());
            }
            for unit in &shown {
                println!(
                    "{}{:<44} {:<32} {}",
                    if unit.failed() { "! " } else { "  " },
                    format!("{}{}", if unit.user { "(you) " } else { "" }, unit.name),
                    state(unit),
                    unit.description
                );
            }
            println!(
                "{} units{}. helix-toolbox services show <unit> says what needs one.",
                shown.len(),
                if found.session {
                    ""
                } else {
                    "; your own session's systemd did not answer"
                }
            );
            Ok(())
        }
        ServicesCommand::Show { unit, user } => {
            let Some(found) = found
                .units
                .iter()
                .find(|other| other.name == unit && other.user == user)
            else {
                bail!("no unit named {unit}; see helix-toolbox services list");
            };
            let needed = services::needed_by(&doctor::Here, &found.name, found.user);
            let cost = services::consequence(&found.name, found.user, ssh());
            if as_json {
                crate::json::print(
                    "service",
                    json!({ "unit": found, "needed_by": needed, "consequence": cost }),
                );
                return Ok(());
            }
            println!(
                "{}{}",
                found.name,
                if found.user { " (your session)" } else { "" }
            );
            if !found.description.is_empty() {
                println!("{}", found.description);
            }
            println!("State: {}", state(found));
            if !needed.is_empty() {
                println!("Needed by: {}", needed.join(", "));
            }
            if let Some(cost) = cost {
                println!("Before stopping or disabling it: {cost}");
            }
            if !found.switchable() {
                println!(
                    "It is not enabled or disabled itself: another unit or a request starts it."
                );
            }
            Ok(())
        }
        ServicesCommand::Start(it) => change(it, Verb::Start),
        ServicesCommand::Stop(it) => change(it, Verb::Stop),
        ServicesCommand::Restart(it) => change(it, Verb::Restart),
        ServicesCommand::Enable(it) => change(it, Verb::Enable),
        ServicesCommand::Disable(it) => change(it, Verb::Disable),
    }
}

pub fn startup(
    command: Option<StartupCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let dirs = services::AutostartDirs::here(&doctor::Here);
    let found = services::autostart_in(&doctor::Here, &dirs, desktop().as_deref());
    match command.unwrap_or(StartupCommand::List) {
        StartupCommand::List => {
            if as_json {
                crate::json::print("startup", json!({ "applications": found }));
                return Ok(());
            }
            for item in &found {
                println!(
                    "{} {:<36} {:<44} {}",
                    if !item.here {
                        "-"
                    } else if item.enabled {
                        "+"
                    } else {
                        " "
                    },
                    item.name,
                    item.file,
                    item.exec
                );
            }
            println!(
                "+ starts at login, - not in this desktop. helix-toolbox startup off <file> stops one, for you only."
            );
            Ok(())
        }
        StartupCommand::On(switch) | StartupCommand::Off(switch) if as_json && !switch.dry_run => {
            bail!("--json shows plans only: add --dry-run; nothing was changed")
        }
        command @ (StartupCommand::On(_) | StartupCommand::Off(_)) => {
            let (on, switch) = match command {
                StartupCommand::On(switch) => (true, switch),
                StartupCommand::Off(switch) => (false, switch),
                StartupCommand::List => unreachable!(),
            };
            let Some(item) = found.iter().find(|item| item.file == switch.file) else {
                bail!(
                    "no autostart file named {}; see helix-toolbox startup",
                    switch.file
                );
            };
            let planned = services::plan_autostart(item, on, &dirs, &doctor::Here)
                .map_err(anyhow::Error::msg)?;
            crate::confirm_and_run(
                catalog,
                platform,
                &planned,
                switch.dry_run,
                switch.yes,
                "YES",
            )
        }
    }
}

pub fn storage(
    command: Option<StorageCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let found = storage::gather(&doctor::Here, platform.manager);
    match command.unwrap_or(StorageCommand::Show) {
        StorageCommand::Show => {
            if as_json {
                crate::json::print("storage", serde_json::to_value(&found).unwrap_or_default());
                return Ok(());
            }
            for filesystem in &found.filesystems {
                println!(
                    "{}: {} free of {}",
                    filesystem.path,
                    storage::shown(filesystem.free),
                    storage::shown(filesystem.total)
                );
            }
            println!();
            for item in &found.items {
                let size = item.size.map(storage::shown).unwrap_or_else(|| "?".into());
                match &item.not_offered {
                    Some(why) => {
                        println!("  {:<16} {:<26} {:>9}  ({why})", item.id, item.title, size)
                    }
                    None => println!(
                        "  {:<16} {:<26} {:>9}  frees {}",
                        item.id,
                        item.title,
                        size,
                        item.frees
                            .map(storage::shown)
                            .unwrap_or_else(|| "what it finds".into())
                    ),
                }
            }
            println!(
                "\nhelix-toolbox storage clean <item> shows its plan, then asks.\n\nKept, never cleaned here:"
            );
            for kept in &found.kept {
                println!("  {}: {}", kept.title, kept.detail);
            }
            Ok(())
        }
        StorageCommand::Clean { item, dry_run, yes } => {
            let Some(found) = found.items.iter().find(|other| other.id == item) else {
                bail!("{item} is not something Helix Toolbox cleans; see helix-toolbox storage");
            };
            let planned = storage::plan(found, platform.manager, &doctor::Here)
                .map_err(anyhow::Error::msg)?;
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "CLEAN")
        }
    }
}
