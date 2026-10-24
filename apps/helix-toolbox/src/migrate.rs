//! `helix-toolbox migrate`: the migration hub on the command line
//! (helix_toolbox_engine::migration). Exporting and showing only read;
//! applying is a plan, confirmed like any other.

use anyhow::{Context, Result};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::printable;
use helix_toolbox_engine::{
    doctor, history,
    migration::{self, Kind, Standing, Translation},
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum MigrateCommand {
    /// Write what this machine has to a file to take to another machine or
    /// distribution: the catalog entries installed, the Flatpaks and the
    /// packages you installed that no entry covers. It only reads.
    Export {
        /// Where to write it; the current directory and a dated name when
        /// left out. With --json the migration is printed instead.
        file: Option<PathBuf>,
    },
    /// What a migration file means on this machine: what would be
    /// installed, what is here already, what cannot be, and the packages
    /// left to you.
    Show { file: PathBuf },
    /// Install what `migrate show` marks Install, as one plan; a failed
    /// install does not stop the others. Confirmed by typing MIGRATE.
    Apply {
        file: PathBuf,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase.
        #[arg(long)]
        yes: bool,
    },
}

/// A migration file read on this machine.
pub fn translate(path: &Path, catalog: &Catalog, platform: &LinuxInfo) -> Result<Translation> {
    Ok(migration::read_here(
        &doctor::Here,
        path,
        &catalog.entries(),
        platform,
    )?)
}

/// The translation as JSON, for `migrate show --json` and the window.
pub fn json(translation: &Translation) -> serde_json::Value {
    json!({
        "from": translation.migration.from,
        "family": translation.migration.family,
        "architecture": translation.migration.architecture,
        "made": translation.migration.made,
        "same_family": translation.same_family,
        "counts": {
            "install": translation.count(Standing::Install),
            "here": translation.count(Standing::Here),
            "unavailable": translation.count(Standing::Unavailable),
            "yours": translation.count(Standing::Yours),
        },
        "items": translation.items,
    })
}

pub fn migrate(
    command: MigrateCommand,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    match command {
        MigrateCommand::Export { file } => {
            let found =
                migration::gather(&doctor::Here, &catalog.entries(), platform, history::now());
            if as_json {
                crate::json::print("migrate-export", json!(found));
                return Ok(());
            }
            let path = file.unwrap_or_else(|| PathBuf::from(migration::file_name(&found)));
            migration::write(&path, &found)?;
            println!(
                "Written to {}: {} entries, {} Flatpaks and {} packages of yours from {}.",
                path.display(),
                found.entries.len(),
                found.flatpaks.len(),
                found.packages.len(),
                found.from
            );
            if migration::chosen(&doctor::Here, platform.manager).is_none() {
                println!(
                    "The packages you installed yourself cannot be told apart here, so only the entries and Flatpaks are in it."
                );
            }
            println!(
                "On the other machine: helix-toolbox migrate show {0}, then migrate apply {0}.",
                path.display()
            );
            Ok(())
        }
        MigrateCommand::Show { file } => {
            let translation = translate(&file, catalog, platform)?;
            if as_json {
                crate::json::print("migration", json(&translation));
                return Ok(());
            }
            print(&translation, platform);
            Ok(())
        }
        MigrateCommand::Apply { file, dry_run, yes } => {
            let translation = translate(&file, catalog, platform)?;
            let planned =
                migration::plan_here(&doctor::Here, &translation, &catalog.entries(), platform)
                    .with_context(|| format!("cannot plan {}", file.display()))?;
            let yours =
                translation.count(Standing::Yours) + translation.count(Standing::Unavailable);
            if !crate::JSON.load(std::sync::atomic::Ordering::Relaxed) && yours > 0 {
                println!(
                    "{yours} of what it names are not installed by this plan; helix-toolbox migrate show {} lists them.",
                    file.display()
                );
            }
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "MIGRATE")
        }
    }
}

fn print(translation: &Translation, platform: &LinuxInfo) {
    let migration = &translation.migration;
    println!(
        "From {} ({}), made {}, onto {}{}.",
        migration.from,
        migration.architecture,
        crate::when(migration.made),
        platform.pretty_name,
        if translation.same_family {
            ", the same family"
        } else {
            ""
        }
    );
    for standing in [
        Standing::Install,
        Standing::Here,
        Standing::Unavailable,
        Standing::Yours,
    ] {
        let items: Vec<_> = translation
            .items
            .iter()
            .filter(|item| item.standing == standing)
            .collect();
        if items.is_empty() {
            continue;
        }
        println!();
        println!(
            "{} ({}):",
            match standing {
                Standing::Install => "To install",
                Standing::Here => "Here already",
                Standing::Unavailable => "Cannot be installed here",
                Standing::Yours => "Left to you",
            },
            items.len()
        );
        for item in items {
            let kind = match item.kind {
                Kind::Entry => "entry",
                Kind::Flatpak => "Flatpak",
                Kind::Package => "package",
            };
            println!(
                "  {:<32} {:<8} {}",
                printable(&item.name),
                kind,
                printable(&item.detail)
            );
        }
    }
    if translation.count(Standing::Install) > 0 {
        println!();
        println!("helix-toolbox migrate apply <file> installs the first list, after you confirm.");
    }
}
