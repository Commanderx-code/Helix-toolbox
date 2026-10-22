//! `helix-toolbox config`: Ghostty's and Fastfetch's configuration on the
//! command line (helix_toolbox_engine::configs). Showing, checking and the
//! history only read; saving a change is a plan, confirmed like any other,
//! that keeps the current file as a configuration backup first.

use anyhow::{Context, Result, bail};
use clap::{Subcommand, ValueEnum};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::printable;
use helix_toolbox_engine::configs::{self, Checked, Kind, Located};
use helix_toolbox_platform::LinuxInfo;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, ValueEnum)]
pub enum Program {
    Ghostty,
    Fastfetch,
}

impl Program {
    fn kind(self) -> Kind {
        match self {
            Program::Ghostty => Kind::Ghostty,
            Program::Fastfetch => Kind::Fastfetch,
        }
    }
}

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Which file is edited, whether it checks, the fields and the backups
    /// kept (the default).
    Show,
    /// Check the file, or another file's text as if it were it.
    Validate { file: Option<PathBuf> },
    /// Change one field (`helix-toolbox config <program>` lists them) and
    /// save; confirmed by typing SAVE.
    Set {
        key: String,
        value: String,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        yes: bool,
    },
    /// Edit a private copy in $VISUAL or $EDITOR, then check it, show what
    /// changed and save; confirmed by typing SAVE.
    Edit {
        #[arg(long)]
        yes: bool,
    },
    /// The configuration backups kept beside the file, newest first;
    /// `helix-toolbox restore <backup>` puts one back.
    History,
}

/// Helix Toolbox's private folder for copies of configurations; never a
/// shared one.
fn folder() -> Result<PathBuf> {
    configs::private_folder()
        .context("there is no state folder for Helix Toolbox's private copies; nothing is saved")
}

fn checked(kind: Kind, text: &str) -> Checked {
    configs::validate(
        kind,
        text,
        &configs::ghostty_here,
        configs::private_folder().as_deref(),
    )
}

/// What `config <program> --json` prints, also when the file cannot be
/// edited.
pub fn json(kind: Kind, found: &Result<Located, String>) -> Value {
    let located = match found {
        Ok(located) => located,
        Err(why) => {
            return json!({
                "kind": kind, "problem": why, "path": null, "live": null, "origin": null,
                "managed": null, "flake": null, "exists": null, "revision": null,
                "checked": null, "values": [], "backups": [],
            });
        }
    };
    let loaded = configs::read(located);
    let (exists, revision, checked, values, problem) = match &loaded {
        Ok(loaded) => (
            json!(loaded.exists),
            json!(loaded.revision),
            json!(checked(kind, &loaded.content)),
            configs::values(kind, &loaded.content),
            Value::Null,
        ),
        Err(why) => (
            Value::Null,
            Value::Null,
            Value::Null,
            Vec::new(),
            json!(why),
        ),
    };
    let fields: Vec<Value> = kind
        .fields()
        .iter()
        .map(|field| {
            let value = values
                .iter()
                .find(|(key, _)| *key == field.key)
                .and_then(|(_, value)| value.clone());
            json!({"key": field.key, "label": field.label, "hint": field.hint, "value": value})
        })
        .collect();
    let backups: Vec<Value> = configs::history(located)
        .iter()
        .map(|backup| json!({"backup": backup.backup, "taken": backup.taken}))
        .collect();
    json!({
        "kind": kind, "problem": problem, "path": located.path, "live": located.live,
        "origin": located.origin, "managed": located.managed, "flake": located.flake,
        "exists": exists, "revision": revision, "checked": checked, "values": fields,
        "backups": backups,
    })
}

/// Check the new text, show what changes and save it, after confirmation.
fn save(
    catalog: &Catalog,
    platform: &LinuxInfo,
    located: &Located,
    loaded: &configs::Loaded,
    after: &str,
    dry_run: bool,
    yes: bool,
) -> Result<()> {
    let before = loaded.content.as_str();
    let checked = checked(located.kind, after);
    if let Checked::Invalid(why) = &checked {
        bail!("not saved: {why}");
    }
    if before == after {
        println!("Nothing changed; nothing was saved.");
        return Ok(());
    }
    if !crate::JSON.load(std::sync::atomic::Ordering::Relaxed) {
        println!("{}", configs::diff(&located.path, before, after));
    }
    let planned = configs::plan_save(located, &loaded.revision, after, &checked, &folder()?)
        .map_err(anyhow::Error::msg)?;
    crate::confirm_and_run(catalog, platform, &planned, dry_run, yes, "SAVE")
}

pub fn config(
    program: Program,
    command: Option<ConfigCommand>,
    catalog: &Catalog,
    platform: &LinuxInfo,
    as_json: bool,
) -> Result<()> {
    let kind = program.kind();
    let found = configs::here(kind);
    match command.unwrap_or(ConfigCommand::Show) {
        ConfigCommand::Show => {
            if as_json {
                crate::json::print("config", json(kind, &found));
                return Ok(());
            }
            let located = found.map_err(anyhow::Error::msg)?;
            let loaded = configs::read(&located).map_err(anyhow::Error::msg)?;
            println!(
                "{}: {}{}",
                kind.label(),
                located.path.display(),
                if loaded.exists {
                    ""
                } else {
                    " (not there yet)"
                }
            );
            println!("Named by {}", located.origin.label());
            if located.path != located.live {
                println!(
                    "{} reads {}{}",
                    kind.label(),
                    located.live.display(),
                    if located.managed {
                        ", a Home Manager link"
                    } else {
                        ""
                    }
                );
            }
            if let Some(flake) = &located.flake {
                println!(
                    "Part of the Home Manager flake in {}: run Home Manager after a change",
                    flake.display()
                );
            }
            let checked = checked(kind, &loaded.content);
            match &checked {
                Checked::Valid => println!("Checks: valid"),
                Checked::Unchecked(why) | Checked::Invalid(why) => {
                    println!("Checks: {}: {}", checked.label(), why)
                }
            }
            println!();
            let values = configs::values(kind, &loaded.content);
            for field in kind.fields() {
                let value = values
                    .iter()
                    .find(|(key, _)| *key == field.key)
                    .and_then(|(_, value)| value.as_deref());
                println!(
                    "  {:<20} {}",
                    field.key,
                    value
                        .map(printable)
                        .unwrap_or_else(|| "(not set)".to_string())
                );
            }
            let kept = configs::history(&located);
            if !kept.is_empty() {
                println!(
                    "\n{} backups kept; the newest: {}",
                    kept.len(),
                    kept[0].backup.display()
                );
            }
            println!(
                "\nhelix-toolbox config {} set <field> <value> changes one; edit opens it in your editor.",
                kind.id()
            );
            Ok(())
        }
        ConfigCommand::Validate { file } => {
            let text = match &file {
                Some(file) => std::fs::read_to_string(file)
                    .with_context(|| format!("cannot read {}", file.display()))?,
                None => {
                    let located = found.map_err(anyhow::Error::msg)?;
                    configs::read(&located).map_err(anyhow::Error::msg)?.content
                }
            };
            let checked = checked(kind, &text);
            if as_json {
                crate::json::print("config-validate", json!({"kind": kind, "checked": checked}));
                return if matches!(checked, Checked::Invalid(_)) {
                    bail!("it does not check")
                } else {
                    Ok(())
                };
            }
            match checked {
                Checked::Valid => {
                    println!("Valid.");
                    Ok(())
                }
                Checked::Unchecked(why) => {
                    println!("Not checked: {why}.");
                    Ok(())
                }
                Checked::Invalid(why) => bail!("invalid: {why}"),
            }
        }
        ConfigCommand::Set {
            key,
            value,
            dry_run,
            yes,
        } => {
            let located = found.map_err(anyhow::Error::msg)?;
            let loaded = configs::read(&located).map_err(anyhow::Error::msg)?;
            let after =
                configs::set(kind, &loaded.content, &key, &value).map_err(anyhow::Error::msg)?;
            save(catalog, platform, &located, &loaded, &after, dry_run, yes)
        }
        ConfigCommand::Edit { yes } => {
            let located = found.map_err(anyhow::Error::msg)?;
            let loaded = configs::read(&located).map_err(anyhow::Error::msg)?;
            let copy = configs::private_copy(kind, "edit", &loaded.content, &folder()?)
                .map_err(anyhow::Error::msg)?;
            let editor = ["VISUAL", "EDITOR"]
                .iter()
                .filter_map(|name| std::env::var(name).ok())
                .find(|editor| !editor.trim().is_empty())
                .unwrap_or_else(|| "nano".to_string());
            // The editor is the user's own; it may carry arguments.
            let status = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!("{editor} \"$1\""))
                .arg("sh")
                .arg(&copy)
                .status();
            let edited = std::fs::read_to_string(&copy);
            let _ = std::fs::remove_file(&copy);
            match status {
                Ok(status) if status.success() => {}
                Ok(status) => bail!("{editor} exited with {status}; nothing was saved"),
                Err(error) => bail!("cannot start {editor}: {error}; nothing was saved"),
            }
            let after = edited.context("the edited copy cannot be read; nothing was saved")?;
            save(catalog, platform, &located, &loaded, &after, false, yes)
        }
        ConfigCommand::History => {
            let located = found.map_err(anyhow::Error::msg)?;
            let kept = configs::history(&located);
            if as_json {
                let backups: Vec<Value> = kept
                    .iter()
                    .map(|backup| json!({"backup": backup.backup, "taken": backup.taken}))
                    .collect();
                crate::json::print(
                    "config-history",
                    json!({"kind": kind, "path": located.path, "backups": backups}),
                );
                return Ok(());
            }
            if kept.is_empty() {
                println!("No backups of {} are kept.", located.path.display());
                return Ok(());
            }
            for backup in &kept {
                println!(
                    "{}  {}",
                    backup.taken.as_deref().unwrap_or("?"),
                    printable(&backup.backup.display().to_string())
                );
            }
            println!(
                "\nhelix-toolbox restore <backup> puts one back; the file there now is kept as a backup first."
            );
            Ok(())
        }
    }
}
