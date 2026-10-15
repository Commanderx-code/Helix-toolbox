//! HELIXFORGE — independent Linux utility platform.

mod tui;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use helixforge_catalog::Catalog;
use helixforge_core::Tool;
use helixforge_engine::{PlanResult, execute, plan_install, read_history};
use helixforge_platform::{LinuxInfo, binary_exists};
use std::io::{self, IsTerminal, Write};

#[derive(Parser)]
#[command(name = "helixforge", version, about = "BUILD • CONFIGURE • OPTIMIZE")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Open the interactive terminal interface (default).
    Tui,
    /// Show operating system and package manager detection.
    System,
    /// List tools available in the bundled catalog.
    List,
    /// Show one tool's details and installation status.
    Info { id: String },
    /// Generate a read-only installation plan.
    Plan { id: String },
    /// Install a catalog tool, requiring explicit consent.
    Install {
        id: String,
        /// Print the plan without making changes.
        #[arg(long)]
        dry_run: bool,
        /// Skip the interactive consent phrase (for trusted automation).
        #[arg(long)]
        yes: bool,
    },
    /// Show recent execution history.
    History {
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
}

fn main() -> Result<()> {
    let args = Cli::parse();
    let catalog = Catalog::builtin().context("could not load bundled catalog")?;
    let os =
        LinuxInfo::detect().context("HELIXFORGE currently requires Linux with /etc/os-release")?;
    match args.command.unwrap_or(Commands::Tui) {
        Commands::Tui => tui::run(&catalog, &os)?,
        Commands::System => show_system(&os),
        Commands::List => {
            for tool in catalog.all() {
                let detected = if binary_exists(&tool.detect.binary) {
                    "installed"
                } else {
                    "not detected"
                };
                println!(
                    "{:<14} {:<14} {:<14} {}",
                    tool.id, tool.category, detected, tool.name
                );
            }
        }
        Commands::Info { id } => {
            let tool = require_tool(&catalog, &id)?;
            println!(
                "{} ({})\nCategory: {}\n{}",
                tool.name, tool.id, tool.category, tool.description
            );
            println!("Executable: {}", tool.detect.binary);
            println!("Detected in PATH: {}", binary_exists(&tool.detect.binary));
            println!(
                "Package on this distro: {}",
                os.manager
                    .and_then(|m| tool.install.packages.for_manager(m))
                    .unwrap_or("unsupported")
            );
        }
        Commands::Plan { id } => print_plan(require_tool(&catalog, &id)?, &os)?,
        Commands::Install { id, dry_run, yes } => {
            let tool = require_tool(&catalog, &id)?;
            match plan_install(tool, &os)? {
                PlanResult::AlreadyDetected { binary } => {
                    println!(
                        "{} is already detected in PATH as '{}'. No changes made.",
                        tool.name, binary
                    );
                }
                PlanResult::Ready(plan) => {
                    println!(
                        "Tool: {}\nPlatform: {}\nPackage: {}\nCommand: {}",
                        plan.tool_name,
                        os.pretty_name,
                        plan.package,
                        plan.command.display()
                    );
                    if dry_run {
                        println!("DRY RUN: No changes made.");
                        return Ok(());
                    }
                    if !yes {
                        if !io::stdin().is_terminal() {
                            bail!(
                                "interactive confirmation requires a terminal; use --dry-run or --yes explicitly"
                            );
                        }
                        print!("Type INSTALL to proceed: ");
                        io::stdout().flush()?;
                        let mut input = String::new();
                        io::stdin().read_line(&mut input)?;
                        if input.trim() != "INSTALL" {
                            bail!("installation cancelled; no changes made");
                        }
                    }
                    execute(&plan).context("installation command did not succeed")?;
                    if binary_exists(&tool.detect.binary) {
                        println!("{} installed and executable detected.", tool.name);
                    } else {
                        println!(
                            "Package command completed, but '{}' was not found in PATH. Verify the installation.",
                            tool.detect.binary
                        );
                    }
                }
            }
        }
        Commands::History { limit } => {
            let items = read_history(limit)?;
            if items.is_empty() {
                println!("No recorded installations yet.");
            }
            for entry in items {
                println!(
                    "{} | {:<12} | {} | {}",
                    entry.unix_timestamp,
                    entry.tool_id,
                    if entry.success { "OK" } else { "FAILED" },
                    entry.result
                );
            }
        }
    }
    Ok(())
}

fn require_tool<'a>(catalog: &'a Catalog, id: &str) -> Result<&'a Tool> {
    catalog
        .find(id)
        .with_context(|| format!("unknown tool '{id}'; run `helixforge list`"))
}

fn show_system(os: &LinuxInfo) {
    println!("HELIXFORGE System Doctor (read-only)");
    println!("Linux: {} (ID: {})", os.pretty_name, os.id);
    println!(
        "Package manager: {}",
        os.manager.map(|m| m.label()).unwrap_or("unsupported")
    );
    if let Some(manager) = os.manager {
        println!(
            "Package command available: {}",
            binary_exists(manager.executable())
        );
    }
    println!("sudo available: {}", binary_exists("sudo"));
    println!("No changes were made.");
}

fn print_plan(tool: &Tool, os: &LinuxInfo) -> Result<()> {
    match plan_install(tool, os)? {
        PlanResult::AlreadyDetected { binary } => println!(
            "{} already detected ({binary}). No changes required.",
            tool.name
        ),
        PlanResult::Ready(plan) => {
            println!("Installation plan for {}", plan.tool_name);
            println!("  Platform: {}", os.pretty_name);
            println!("  Package:  {}", plan.package);
            println!("  Command:  {}", plan.command.display());
            println!("  Risk:     Package installation requires elevated privileges.");
            println!("  Notes:    Native package manager may prompt for approval.");
            println!("No commands executed.");
        }
    }
    Ok(())
}
