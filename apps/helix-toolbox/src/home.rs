//! `helix-toolbox home-manager`: Home Manager on the command line
//! (helix_toolbox_engine::home_manager). Showing only reads; building,
//! switching, going back to a generation and pulling the configuration's
//! repository are plans, confirmed like any other.

use anyhow::{Result, bail};
use clap::Subcommand;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::printable;
use helix_toolbox_engine::{
    doctor,
    home_manager::{self, Config, Status},
    repositories::{self, Change},
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::json;

#[derive(Subcommand)]
pub enum HomeManagerCommand {
    /// The configuration, the active generation, its repository and the
    /// files Home Manager links (the default).
    Show,
    /// The generations Home Manager built, newest first.
    Generations,
    /// Build the configuration without activating it; confirmed by typing
    /// BUILD.
    Build {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Build and activate the configuration; confirmed by typing SWITCH.
    Switch {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Go back to an earlier generation through its own activate script;
    /// confirmed by typing ACTIVATE.
    Activate {
        /// The generation's number, as `home-manager generations` lists it.
        generation: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Pull the configuration's Git repository, fast-forward only;
    /// confirmed by typing GIT.
    Pull {
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
}

/// The status as JSON, for `home-manager --json` and the window.
pub fn json(status: &Status) -> serde_json::Value {
    json!({
        "installed": status.installed,
        "version": status.version,
        "config": status.config.as_ref().map(|config| json!({
            "folder": config.folder,
            "flake": config.flake,
            "profile": config.profile,
            "origin": config.origin,
            "reference": config.flake_reference(),
        })),
        "problem": status.problem,
        "generations": status.generations,
        "repository": status.repository,
        "links": status.links,
    })
}

fn ready(status: &Status) -> Result<&Config> {
    if !status.installed {
        bail!("home-manager is not installed here");
    }
    match (&status.config, &status.problem) {
        (Some(config), _) => Ok(config),
        (None, Some(problem)) => bail!("{problem}"),
        (None, None) => bail!(
            "no Home Manager configuration was found: name its folder as flake under [home_manager] in settings.toml"
        ),
    }
}

pub fn home_manager(
    command: Option<HomeManagerCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let status = home_manager::status(&doctor::Here, &repositories::Here);
    let run = |planned, dry_run, yes, word| {
        crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, word)
    };
    match command.unwrap_or(HomeManagerCommand::Show) {
        HomeManagerCommand::Show => {
            if as_json {
                crate::json::print("home-manager", json(&status));
                return Ok(());
            }
            print(&status);
            Ok(())
        }
        HomeManagerCommand::Generations => {
            if as_json {
                crate::json::print(
                    "home-manager-generations",
                    json!({ "generations": status.generations }),
                );
                return Ok(());
            }
            if status.generations.is_empty() {
                println!("Home Manager lists no generations here.");
            }
            for generation in &status.generations {
                println!(
                    "{:>5}  {}  {}{}",
                    generation.id,
                    generation.built,
                    generation.path.display(),
                    if generation.current { "  (active)" } else { "" }
                );
            }
            Ok(())
        }
        HomeManagerCommand::Build { dry_run, yes } => run(
            home_manager::plan_build(ready(&status)?),
            dry_run,
            yes,
            "BUILD",
        ),
        HomeManagerCommand::Switch { dry_run, yes } => run(
            home_manager::plan_switch(ready(&status)?),
            dry_run,
            yes,
            "SWITCH",
        ),
        HomeManagerCommand::Activate {
            generation,
            dry_run,
            yes,
        } => {
            let found = home_manager::find_generation(&status.generations, &generation)
                .map_err(anyhow::Error::msg)?;
            let planned =
                home_manager::plan_activate(&doctor::Here, &found).map_err(anyhow::Error::msg)?;
            run(planned, dry_run, yes, "ACTIVATE")
        }
        HomeManagerCommand::Pull { dry_run, yes } => {
            ready(&status)?;
            let Some(repository) = &status.repository else {
                bail!("the configuration is not in a Git repository");
            };
            let details = repositories::details(&repositories::Here, &repository.path)
                .map_err(anyhow::Error::msg)?;
            let planned = repositories::plan(&repositories::Here, &details, &Change::Pull)
                .map_err(anyhow::Error::msg)?;
            run(planned, dry_run, yes, "GIT")
        }
    }
}

fn print(status: &Status) {
    match (&status.version, status.installed) {
        (Some(version), _) => println!("Home Manager {version}"),
        (None, true) => println!("Home Manager"),
        (None, false) => println!("home-manager is not installed here."),
    }
    match (&status.config, &status.problem) {
        (Some(config), _) => println!(
            "Configuration: {} ({}, from {})",
            config.flake_reference().unwrap_or_else(|| config
                .folder
                .join("home.nix")
                .display()
                .to_string()),
            if config.flake { "a flake" } else { "home.nix" },
            config.origin.label()
        ),
        (None, Some(problem)) => println!("Configuration: {}", printable(problem)),
        (None, None) => println!(
            "Configuration: none found; name its folder as flake under [home_manager] in settings.toml"
        ),
    }
    if let Some(current) = status.current() {
        println!("Active: generation {}, built {}", current.id, current.built);
    }
    if status.generations.len() > 1 {
        println!(
            "{} generations to go back to: helix-toolbox home-manager generations",
            status.generations.len() - 1
        );
    }
    if let Some(repository) = &status.repository {
        match &repository.problem {
            Some(problem) => println!(
                "Repository {}: {}",
                repository.path.display(),
                printable(problem)
            ),
            None => println!(
                "Repository {}: {}, {} changed{}",
                repository.path.display(),
                repository.branch.as_deref().unwrap_or("detached"),
                repository.changed,
                match (repository.ahead, repository.behind) {
                    (Some(ahead), Some(behind)) if ahead + behind > 0 =>
                        format!(", {ahead} ahead and {behind} behind its upstream"),
                    (Some(_), Some(_)) => ", up to date with its upstream".to_string(),
                    _ => String::new(),
                }
            ),
        }
    }
    let managed: Vec<&str> = status
        .links
        .iter()
        .filter(|link| link.managed)
        .map(|link| link.name.as_str())
        .collect();
    if !managed.is_empty() {
        println!(
            "Home Manager links: {}; change them in its configuration",
            managed.join(", ")
        );
    }
}
