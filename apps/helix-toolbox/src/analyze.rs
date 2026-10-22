//! `helix-toolbox performance` and `recovery`: the performance and boot
//! analyzer and recovery readiness on the command line
//! (helix_toolbox_engine::performance and recovery). Both only read; the
//! snapshot check is a plan of its own that reads as root.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{
    backups, doctor, drift, history,
    performance::{self, Boot, Comparison, shown, shown_change},
    recovery, storage,
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;

#[derive(Subcommand)]
pub enum PerformanceCommand {
    /// This boot, the machine now, and this boot beside the one before
    /// (the default).
    Show,
    /// The boots recorded so far, newest first, numbered for compare.
    Boots,
    /// One recorded boot beside another, with the runs in between.
    Compare {
        /// The earlier boot: its number in `performance boots`, or the start
        /// of its id.
        before: String,
        /// The later boot; this one when left out.
        after: Option<String>,
    },
}

fn state() -> Option<std::path::PathBuf> {
    history::state_dir()
}

fn boot_line(boot: &Boot) -> String {
    format!(
        "{}, kernel {}: {}",
        crate::when(boot.booted),
        if boot.kernel.is_empty() {
            "unknown"
        } else {
            &boot.kernel
        },
        shown(boot.phases.total)
    )
}

fn print_comparison(compared: &Comparison, against: &str) {
    println!(
        "{against} ({}): {}",
        crate::when(compared.before.booted),
        shown_change(compared.total())
    );
    for change in compared.phases.iter().skip(1) {
        if let (Some(after), Some(difference)) = (change.after, change.difference())
            && difference.unsigned_abs() >= 100
        {
            println!(
                "  {:<38} {:>10}  ({})",
                change.name,
                shown(after),
                shown_change(difference)
            );
        }
    }
    for change in &compared.units {
        let now = change.after.map(shown).unwrap_or_else(|| "not now".into());
        let how = match change.difference() {
            Some(difference) => shown_change(difference),
            None if change.before.is_none() => "new".into(),
            None => format!("was {}", change.before.map(shown).unwrap_or_default()),
        };
        println!("  {:<38} {:>10}  ({how})", change.name, now);
    }
    if compared.before.kernel != compared.after.kernel && !compared.before.kernel.is_empty() {
        println!(
            "  The kernel changed: {} to {}",
            compared.before.kernel, compared.after.kernel
        );
    }
    if compared.runs.is_empty() {
        println!("  No Helix Toolbox runs in between.");
    } else {
        println!("  Helix Toolbox ran in between:");
        for run in &compared.runs {
            println!(
                "    {}  {}{}",
                crate::when(run.at),
                run.names.join(", "),
                if run.success { "" } else { " (failed)" }
            );
        }
    }
}

pub fn performance(command: Option<PerformanceCommand>, as_json: bool) -> Result<()> {
    let history = history::read(1000).unwrap_or_default();
    let found = performance::gather(&doctor::Here, state().as_deref(), &history);
    match command.unwrap_or(PerformanceCommand::Show) {
        PerformanceCommand::Show => {
            if as_json {
                crate::json::print(
                    "performance",
                    serde_json::to_value(&found).unwrap_or_default(),
                );
                return Ok(());
            }
            match (&found.boot, &found.no_boot) {
                (Some(boot), _) => {
                    println!("Boot of {}", boot_line(boot));
                    println!(
                        "  {}",
                        boot.phases
                            .parts()
                            .iter()
                            .map(|(name, took)| format!("{name} {}", shown(*took)))
                            .collect::<Vec<_>>()
                            .join(" · ")
                    );
                    println!();
                    match &found.previous {
                        Some(compared) => print_comparison(compared, "Against the boot before"),
                        None => println!(
                            "No earlier boot is recorded yet; after the next boot, this one is compared with it."
                        ),
                    }
                    let waited: Vec<_> = boot
                        .chain
                        .iter()
                        .filter(|link| link.took.is_some())
                        .collect();
                    if !waited.is_empty() {
                        println!("\nThe login screen waited for:");
                        for link in waited {
                            println!(
                                "  {:<44} {:>10}{}",
                                link.name,
                                shown(link.took.unwrap_or(0)),
                                link.at
                                    .map(|at| format!("  (up {} into the services)", shown(at)))
                                    .unwrap_or_default()
                            );
                        }
                    }
                    if !boot.slowest.is_empty() {
                        println!("\nSlowest services since the boot:");
                        for timed in &boot.slowest {
                            println!("  {:>10}  {}", shown(timed.took), timed.name);
                        }
                    }
                }
                (None, why) => println!("{}", why.as_deref().unwrap_or("No record of the boot.")),
            }
            let now = &found.now;
            println!("\nNow:");
            if let Some([one, five, fifteen]) = now.load {
                println!(
                    "  Load {one:.2} {five:.2} {fifteen:.2} (1, 5 and 15 minutes) on {} processors",
                    now.cpus
                );
            }
            if now.memory_total > 0 {
                println!(
                    "  Memory: {} free of {}",
                    storage::shown(now.memory_available),
                    storage::shown(now.memory_total)
                );
            }
            if now.swaps.is_empty() {
                println!("  Swap: none");
            } else {
                println!(
                    "  Swap: {} used of {} ({})",
                    storage::shown(now.swap_used()),
                    storage::shown(now.swap_total()),
                    now.swaps
                        .iter()
                        .map(|swap| if swap.zram {
                            "zram".to_string()
                        } else {
                            swap.name.clone()
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            let pressure = [
                ("processor", now.pressure.cpu),
                ("memory", now.pressure.memory),
                ("disk", now.pressure.io),
            ]
            .into_iter()
            .filter_map(|(what, waited)| Some(format!("{what} {:.0}%", waited?)))
            .collect::<Vec<_>>();
            if !pressure.is_empty() {
                println!("  Waiting in the last minute: {}", pressure.join(", "));
            }
            match (&now.governor, &now.power_profile) {
                (Some(governor), Some(profile)) => {
                    println!("  Governor {governor}, power profile {profile}")
                }
                (Some(governor), None) => println!("  Governor {governor}"),
                (None, Some(profile)) => println!("  Power profile {profile}"),
                (None, None) => {}
            }
            if !now.heaviest.is_empty() {
                println!(
                    "  Most memory: {}",
                    now.heaviest
                        .iter()
                        .take(5)
                        .map(|process| format!(
                            "{} {}",
                            helix_toolbox_core::printable(&process.name),
                            storage::shown(process.memory)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            if !found.hints.is_empty() {
                println!("\nWorth knowing:");
                for hint in &found.hints {
                    println!("  {}\n    {}", hint.title, hint.detail);
                    if let Some(command) = &hint.command {
                        println!("    {command}");
                    }
                    if let Some(entry) = &hint.entry {
                        println!("    helix-toolbox run {entry}");
                    }
                }
            }
            Ok(())
        }
        PerformanceCommand::Boots => {
            let boots = state()
                .map(|state| performance::load(&performance::boots_path(&state)))
                .unwrap_or_default();
            if as_json {
                crate::json::print("boots", json!({ "boots": boots }));
                return Ok(());
            }
            if boots.is_empty() {
                println!("No boots are recorded yet; helix-toolbox performance records this one.");
            }
            let current = found.boot.as_ref().map(|boot| boot.id.as_str());
            for (index, boot) in boots.iter().enumerate() {
                println!(
                    "{:>3}  {}{}",
                    index + 1,
                    boot_line(boot),
                    if Some(boot.id.as_str()) == current {
                        "  (this boot)"
                    } else {
                        ""
                    }
                );
            }
            Ok(())
        }
        PerformanceCommand::Compare { before, after } => {
            let boots = state()
                .map(|state| performance::load(&performance::boots_path(&state)))
                .unwrap_or_default();
            let pick = |wanted: &str| {
                performance::find(&boots, wanted).with_context(|| {
                    format!("no recorded boot {wanted}; see helix-toolbox performance boots")
                })
            };
            let earlier = pick(&before)?;
            let later = match &after {
                Some(after) => pick(after)?,
                None => found
                    .boot
                    .as_ref()
                    .and_then(|boot| boots.iter().find(|other| other.id == boot.id))
                    .context("this boot has no record to compare; name the later boot")?,
            };
            if earlier.id == later.id {
                bail!("that is one boot twice; name an earlier one");
            }
            let (earlier, later) = if earlier.booted <= later.booted {
                (earlier, later)
            } else {
                (later, earlier)
            };
            let compared = performance::compare(earlier, later, &history);
            if as_json {
                crate::json::print(
                    "comparison",
                    serde_json::to_value(&compared).unwrap_or_default(),
                );
                return Ok(());
            }
            println!("Boot of {}", boot_line(later));
            print_comparison(&compared, "Against the boot of");
            Ok(())
        }
    }
}

pub fn recovery(
    check_snapshots: bool,
    dry_run: bool,
    yes: bool,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    if check_snapshots {
        let Some(tool) = recovery::snapshot_tool(&doctor::Here) else {
            bail!(
                "neither Snapper (with a root configuration) nor Timeshift is set up, so there are no snapshots to look at"
            );
        };
        return crate::confirm_and_run(
            catalog,
            platform,
            &recovery::plan_check(tool),
            dry_run,
            yes,
            "CHECK",
        );
    }
    let found_backups = backups::discover(&backups::Roots::here());
    let baselines = drift::dir()
        .map(|directory| drift::load_all(&directory).0)
        .unwrap_or_default();
    let found = recovery::examine(
        &doctor::Here,
        &recovery::Known {
            manager: platform.manager,
            backups: &found_backups,
            baselines: &baselines,
            now: history::now(),
        },
    );
    if as_json {
        let mut value = crate::json::doctor(&found);
        value["snapshots"] = json!(recovery::snapshot_tool(&doctor::Here).map(|tool| tool.label()));
        crate::json::print("recovery", value);
        return Ok(());
    }
    for check in &found {
        println!(
            "{:<8} {}: {}",
            check.health.label(),
            check.title,
            check.summary
        );
        for line in &check.evidence {
            println!("           {line}");
        }
        if check.health != doctor::Health::Good || check.id == "snapshots" {
            if let Some(advice) = &check.advice {
                println!("           What to do: {advice}");
            }
            if let Some(entry) = &check.entry {
                println!("           The {entry} entry sets it up: helix-toolbox run {entry}");
            }
        }
    }
    Ok(())
}
