//! `helixforge setup` and `helixforge desktop`: the setup wizard's
//! workstations as a plan to review, and the desktop's appearance, its saved
//! looks and who else sets them.

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use helixforge_catalog::Catalog;
use helixforge_core::{Entry, printable};
use helixforge_engine::{
    desktop::{self, Desktop},
    doctor,
    setup::{self, Survey},
};
use helixforge_platform::LinuxInfo;
use std::path::Path;

/// This machine's desktop.
pub fn desktop_here() -> Desktop {
    desktop::detect(&doctor::Here, &|name| std::env::var(name).ok())
}

/// What the wizard learns about this machine.
pub fn survey_here(platform: &LinuxInfo, desktop: &Desktop) -> Survey {
    let laptop = std::fs::read_dir("/sys/class/power_supply")
        .map(|listing| {
            listing.flatten().any(|item| {
                std::fs::read_to_string(item.path().join("type"))
                    .is_ok_and(|kind| kind.trim() == "Battery")
            })
        })
        .unwrap_or(false);
    setup::survey(
        platform,
        desktop,
        helixforge_engine::preflight::gpus(),
        laptop,
        &helixforge_platform::binary_exists,
    )
}

/// The survey as lines of text.
pub fn survey_lines(survey: &Survey) -> Vec<(&'static str, String)> {
    let mut lines = vec![("System", survey.distribution.clone())];
    lines.push((
        "Packages",
        match &survey.package_manager {
            Some(manager) if survey.tools.is_empty() => manager.clone(),
            Some(manager) => format!("{manager}, with {}", survey.tools.join(", ")),
            None => "no package manager HelixForge knows".to_string(),
        },
    ));
    lines.push((
        "Desktop",
        survey
            .desktop_name
            .clone()
            .unwrap_or_else(|| "none found (a server or a console)".to_string()),
    ));
    lines.push((
        "Graphics",
        if survey.gpus.is_empty() {
            "none found".to_string()
        } else {
            survey
                .gpus
                .iter()
                .map(|gpu| match gpu.as_str() {
                    "nvidia" => "NVIDIA".to_string(),
                    "amd" => "AMD".to_string(),
                    "intel" => "Intel".to_string(),
                    other => other.to_string(),
                })
                .collect::<Vec<_>>()
                .join(" and ")
        },
    ));
    lines.push((
        "Machine",
        if survey.laptop {
            "a laptop"
        } else {
            "a desktop or server"
        }
        .to_string(),
    ));
    lines
}

pub fn run_setup(
    workstation: Option<String>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let found = setup::load(catalog.root()).map_err(anyhow::Error::msg)?;
    let desktop = desktop_here();
    let survey = survey_here(platform, &desktop);
    let chosen = match &workstation {
        Some(id) => Some(
            found
                .workstations
                .iter()
                .find(|known| known.id == *id || known.name.eq_ignore_ascii_case(id))
                .with_context(|| {
                    let known: Vec<&str> = found
                        .workstations
                        .iter()
                        .map(|known| known.id.as_str())
                        .collect();
                    format!(
                        "no workstation '{}'; there are: {}",
                        printable(id),
                        known.join(", ")
                    )
                })?,
        ),
        None => None,
    };
    let ids = match chosen {
        Some(known) if known.entries.is_empty() && known.when.is_empty() => {
            found.everything(&survey)
        }
        Some(known) => known.entries_for(&survey),
        None => Vec::new(),
    };
    let suggested = setup::suggestions(&ids, catalog, platform);
    if as_json {
        crate::json::print(
            "setup",
            serde_json::json!({
                "machine": survey,
                "workstations": found.workstations,
                "chosen": chosen.map(|known| &known.id),
                "suggestions": suggested,
            }),
        );
        return Ok(());
    }
    for (label, value) in survey_lines(&survey) {
        println!("{label:<10}{value}");
    }
    let Some(chosen) = chosen else {
        println!("\nWorkstations (helixforge setup <name> shows its plan):");
        for known in &found.workstations {
            println!("  {:<10} {}", known.id, known.description);
        }
        return Ok(());
    };
    println!("\n{}: {}", chosen.name, chosen.description);
    for suggestion in &suggested {
        println!(
            "  {} {}{}",
            if suggestion.available { "+" } else { "-" },
            suggestion.name,
            suggestion
                .note
                .as_deref()
                .map(|note| format!("  ({note})"))
                .unwrap_or_default()
        );
    }
    let runnable: Vec<&str> = suggested
        .iter()
        .filter(|suggestion| suggestion.available)
        .map(|suggestion| suggestion.id.as_str())
        .collect();
    if runnable.is_empty() {
        println!("\nNothing here can run on this machine.");
        return Ok(());
    }
    println!("\nThe plan, which nothing has run:");
    let entries: Vec<&Entry> = runnable.iter().filter_map(|id| catalog.find(id)).collect();
    let planned = helixforge_engine::plan(entries, platform, &|id| catalog.find(id))?;
    crate::print_plan(catalog, platform, &planned);
    println!(
        "\nTo run it, after reading it: helixforge run {}",
        runnable.join(" ")
    );
    Ok(())
}

#[derive(Subcommand)]
pub enum DesktopCommand {
    /// Save the desktop's appearance as a look, to put back later.
    Save { name: String },
    /// Put a saved look back with the desktop's own tools, after confirmation.
    Apply {
        name: String,
        /// Show the plan and stop.
        #[arg(long)]
        dry_run: bool,
        /// Skip the confirmation phrase, for deliberate automation.
        #[arg(long)]
        yes: bool,
    },
    /// List the saved looks.
    List,
    /// Delete a saved look.
    Delete { name: String },
}

fn looks_dir() -> Result<std::path::PathBuf> {
    desktop::dir().context("cannot find the configuration folder")
}

fn named(directory: &Path, name: &str) -> Result<(String, desktop::Look)> {
    let wanted = helixforge_engine::profiles::slug(name).unwrap_or_default();
    desktop::load_all(directory)
        .0
        .into_iter()
        .find(|(slug, _)| *slug == wanted)
        .with_context(|| {
            format!(
                "no look named '{}'; see helixforge desktop list",
                printable(name)
            )
        })
}

pub fn run_desktop(
    command: Option<DesktopCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let found = desktop_here();
    match command {
        None => {
            let offered = setup::load(catalog.root())
                .map(|setup| setup.for_desktop(found.kind))
                .unwrap_or_default();
            let offered = setup::suggestions(&offered, catalog, platform);
            if as_json {
                crate::json::print(
                    "desktop",
                    serde_json::json!({ "desktop": found, "entries": offered }),
                );
                return Ok(());
            }
            print!("{}", desktop_text(&found));
            let available: Vec<String> = offered
                .iter()
                .filter(|suggestion| suggestion.available)
                .map(|suggestion| format!("{} ({})", suggestion.name, suggestion.id))
                .collect();
            if !available.is_empty() {
                println!("\nFor this desktop: {}", available.join(", "));
            }
            Ok(())
        }
        Some(_) if as_json => bail!("--json is for helixforge desktop alone"),
        Some(DesktopCommand::Save { name }) => {
            let look = desktop::capture(&name, &found)
                .context("no desktop whose appearance HelixForge can read is running here")?;
            let path = desktop::save(&looks_dir()?, &look)?;
            println!(
                "Saved {} ({} settings of {}) as {}",
                printable(&look.name),
                look.settings.len(),
                found.kind.map(|kind| kind.label()).unwrap_or("the desktop"),
                path.display()
            );
            Ok(())
        }
        Some(DesktopCommand::Apply { name, dry_run, yes }) => {
            let (_, look) = named(&looks_dir()?, &name)?;
            let planned = desktop::plan_apply(&look, &found).map_err(anyhow::Error::msg)?;
            crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "APPLY")
        }
        Some(DesktopCommand::List) => {
            let (looks, problems) = desktop::load_all(&looks_dir()?);
            for problem in problems {
                eprintln!("Look skipped: {}", printable(&problem));
            }
            if looks.is_empty() {
                println!("No saved looks. helixforge desktop save <name> keeps this desktop's.");
            }
            for (slug, look) in looks {
                println!(
                    "{slug}  {} ({})\n    {}",
                    printable(&look.name),
                    printable(&look.desktop),
                    look.settings
                        .iter()
                        .map(|(key, value)| format!("{key} {}", printable(value)))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
            }
            Ok(())
        }
        Some(DesktopCommand::Delete { name }) => {
            let directory = looks_dir()?;
            let (slug, look) = named(&directory, &name)?;
            let path = directory.join(format!("{slug}.toml"));
            std::fs::remove_file(&path)
                .with_context(|| format!("cannot delete {}", path.display()))?;
            println!("Deleted {}", printable(&look.name));
            Ok(())
        }
    }
}

/// The desktop as text.
pub fn desktop_text(found: &Desktop) -> String {
    let mut text = format!(
        "Desktop   {}\n",
        match (&found.name, &found.session) {
            (Some(name), Some(session)) => format!("{name}, {session} session"),
            (Some(name), None) => name.clone(),
            (None, _) => "none found".to_string(),
        }
    );
    match &found.display_manager {
        Some(manager) => {
            text.push_str(&format!(
                "Login     {}{}\n",
                manager.name,
                manager
                    .theme
                    .as_deref()
                    .map(|theme| format!(", theme {theme}"))
                    .unwrap_or_default()
            ));
            if !manager.files.is_empty() {
                text.push_str(&format!("          {}\n", manager.files.join(", ")));
            }
        }
        None => text.push_str("Login     no display manager is enabled\n"),
    }
    if !found.appearance.is_empty() {
        text.push_str("\nAppearance\n");
    }
    for value in &found.appearance {
        text.push_str(&format!(
            "  {:<16}{}\n",
            value.label,
            value.value.as_deref().unwrap_or("not set")
        ));
    }
    for owner in &found.owners {
        text.push_str(&format!("\nNote: {owner}\n"));
    }
    if let Some(blocked) = &found.blocked {
        text.push_str(&format!("\nNot changed here: {blocked}\n"));
    }
    text
}
