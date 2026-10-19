//! `--json`: what the commands print, as one JSON document for other
//! programs. The shapes here are the stable interface, documented in
//! docs/JSON.md; they are written out field by field rather than derived from
//! the engine's types, so changing those does not change them. A new field
//! may appear in a later version; a field is never removed or changed in
//! meaning without raising `SCHEMA`.

use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item, NixAction, Risk};
use helix_toolbox_engine::{
    Plan, backups, doctor, history,
    inventory::{Status, which},
    preflight, updates,
};
use helix_toolbox_platform::LinuxInfo;
use serde_json::{Value, json};

/// The version of these shapes.
pub const SCHEMA: u32 = 1;

/// Print one command's result, wrapped with the schema version.
pub fn print(command: &str, data: Value) {
    let document = json!({
        "schema": SCHEMA,
        // Named for HelixForge in schema 1, so it stays; the same version.
        "helixforge": crate::VERSION,
        "helix_toolbox": crate::VERSION,
        "command": command,
        "data": data,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&document).unwrap_or_default()
    );
}

pub fn system(platform: &LinuxInfo) -> Value {
    let tools: serde_json::Map<String, Value> = ["sudo", "nix", "home-manager", "flatpak"]
        .into_iter()
        .map(|tool| (tool.to_string(), Value::Bool(which(tool).is_some())))
        .collect();
    json!({
        "distribution": {
            "id": platform.id,
            "name": platform.pretty_name,
            "id_like": platform.id_like,
        },
        "package_manager": platform.manager.map(|manager| manager.executable()),
        "kernel": std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|text| text.trim().to_string())
            .ok(),
        "architecture": std::env::consts::ARCH,
        "tools": tools,
    })
}

fn kind(entry: &Entry) -> &'static str {
    match entry.action {
        Action::Package { .. } => "package",
        Action::Script { .. } => "script",
        Action::Command { .. } => "command",
        Action::Nix { .. } => "nix",
        Action::Native { .. } => "native",
        Action::Profile { .. } => "profile",
    }
}

/// What every listing says about an entry.
pub fn entry(entry: &Entry) -> Value {
    json!({
        "id": entry.id,
        "name": entry.name,
        "description": entry.description,
        "flags": entry.flags,
        "kind": kind(entry),
    })
}

fn items(items: &[Item]) -> Vec<Value> {
    items
        .iter()
        .map(|item| match item {
            Item::Group(group) => json!({
                "type": "group",
                "name": group.name,
                "items": self::items(&group.items),
            }),
            Item::Entry(found) => {
                let mut value = entry(found);
                value["type"] = json!("entry");
                value
            }
        })
        .collect()
}

pub fn list(catalog: &Catalog) -> Value {
    json!({
        "tabs": catalog
            .tabs
            .iter()
            .map(|tab| json!({ "id": tab.id, "name": tab.name, "items": items(&tab.items) }))
            .collect::<Vec<_>>(),
    })
}

pub fn search(query: &str, found: &[(u32, &Entry)]) -> Value {
    json!({
        "query": query,
        "results": found
            .iter()
            .map(|(score, found)| {
                let mut value = entry(found);
                value["score"] = json!(score);
                value
            })
            .collect::<Vec<_>>(),
    })
}

/// One entry in full; `status` is how it stands, for package entries.
pub fn info(found: &Entry, status: Option<&Status>) -> Value {
    let mut value = entry(found);
    value["action"] = match &found.action {
        Action::Package {
            binary,
            packages,
            aur,
            flatpak,
            conflicts,
            removable,
        } => {
            let packages: serde_json::Map<String, Value> = packages
                .all()
                .into_iter()
                .filter_map(|(family, name)| Some((family.to_string(), json!(name?))))
                .collect();
            json!({
                "binary": binary,
                "packages": packages,
                "aur": aur,
                "flatpak": flatpak,
                "conflicts": conflicts,
                "removable": removable,
            })
        }
        Action::Script { file, interactive } => json!({
            "file": file,
            "interactive": interactive,
        }),
        Action::Command { line } => json!({ "line": line }),
        Action::Nix { action } => match action {
            NixAction::ProfileInstall { installable } => {
                json!({ "nix": "profile-install", "installable": installable })
            }
            NixAction::HomeManagerSwitch { flake } => {
                json!({ "nix": "home-manager-switch", "flake": flake })
            }
        },
        Action::Native { action } => json!({ "native": action.id() }),
        Action::Profile { entries } => json!({ "entries": entries }),
    };
    value["status"] = status.map(self::status).unwrap_or(Value::Null);
    value
}

/// How a package entry stands.
pub fn status(status: &Status) -> Value {
    let mut value = match status {
        Status::Installed {
            source,
            version,
            shadowed,
        } => json!({
            "state": "installed",
            "source": source,
            "version": version,
            "shadowed_by": shadowed,
        }),
        Status::Update {
            source,
            version,
            latest,
            shadowed,
        } => json!({
            "state": "update",
            "source": source,
            "version": version,
            "latest": latest,
            "shadowed_by": shadowed,
        }),
        Status::Managed { by, version } => json!({
            "state": "managed",
            "by": by,
            "version": version,
        }),
        Status::Elsewhere { path } => json!({ "state": "elsewhere", "path": path }),
        Status::Absent { replaces } => json!({ "state": "absent", "replaces": replaces }),
    };
    value["label"] = json!(status.label());
    value["detail"] = json!(status.detail());
    value
}

pub fn statuses(found: &[(&Entry, Option<Status>)]) -> Value {
    json!({
        "entries": found
            .iter()
            .map(|(found, state)| json!({
                "id": found.id,
                "name": found.name,
                "status": state.as_ref().map(status),
            }))
            .collect::<Vec<_>>(),
    })
}

fn risk(risk: Risk) -> &'static str {
    match risk {
        Risk::Low => "low",
        Risk::Medium => "medium",
        Risk::High => "high",
    }
}

/// A plan and what the checks before a run found.
pub fn plan(planned: &Plan, findings: &[preflight::Finding]) -> Value {
    json!({
        "steps": planned
            .steps
            .iter()
            .enumerate()
            .map(|(index, step)| json!({
                "index": index,
                "id": step.id(),
                "name": step.name(),
                "command": step.display(),
                "fallbacks": step.fallbacks(),
                "depends_on": planned.depends.get(index).cloned().unwrap_or_default(),
            }))
            .collect::<Vec<_>>(),
        "notes": planned.notes,
        "blocked": planned.blocked,
        "requirements": planned.requirements,
        "privileged": planned.privileged,
        "risk": risk(planned.risk),
        "keep_going": planned.keep_going,
        "findings": findings
            .iter()
            .map(|finding| json!({
                "severity": match finding.severity {
                    preflight::Severity::Blocker => "blocker",
                    preflight::Severity::Warning => "warning",
                },
                "message": finding.message,
            }))
            .collect::<Vec<_>>(),
        "runnable": !planned.is_empty() && !preflight::blocked(findings),
    })
}

pub fn history(records: &[history::Record]) -> Value {
    json!({
        "runs": records
            .iter()
            .map(|record| json!({
                "unix_timestamp": record.unix_timestamp,
                "entries": record.entries,
                "names": record.names,
                "success": record.success,
                "result": record.result,
                "log": record.log,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn updates(channels: &[updates::Channel], online: bool) -> Value {
    json!({
        "online": online,
        "channels": channels
            .iter()
            .map(|channel| json!({
                "id": channel.id,
                "name": channel.name,
                "count": channel.count(),
                "pending": channel.pending.as_ref().map(|pending| pending
                    .iter()
                    .map(|item| json!({ "name": item.name, "from": item.from, "to": item.to }))
                    .collect::<Vec<_>>()),
                "note": channel.note,
                "commands": channel.commands,
                "caveat": channel.caveat,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn doctor(found: &[doctor::Diagnosis]) -> Value {
    json!({
        "checks": found
            .iter()
            .map(|diagnosis| json!({
                "id": diagnosis.id,
                "title": diagnosis.title,
                "health": match diagnosis.health {
                    doctor::Health::Good => "good",
                    doctor::Health::Notice => "notice",
                    doctor::Health::Problem => "problem",
                },
                "summary": diagnosis.summary,
                "evidence": diagnosis.evidence,
                "advice": diagnosis.advice,
                "repair": diagnosis.repair.as_ref().map(|repair| json!({
                    "label": repair.label,
                    "commands": repair.commands,
                })),
                "entry": diagnosis.entry,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn backups(snapshots: Option<backups::Snapshots>, found: &[backups::Backup]) -> Value {
    json!({
        "snapshots": snapshots.map(|tool| json!({
            "tool": tool.label(),
            "restore": tool.how_to_restore(),
        })),
        "backups": found
            .iter()
            .map(|backup| json!({
                "original": backup.original,
                "backup": backup.backup,
                "taken": backup.taken,
                "system": backup.system,
                "directory": backup.directory,
            }))
            .collect::<Vec<_>>(),
    })
}

pub fn profiles(found: &[(String, helix_toolbox_engine::profiles::Profile)]) -> Value {
    json!({
        "profiles": found
            .iter()
            .map(|(slug, profile)| json!({
                "id": format!("{}{slug}", helix_toolbox_engine::profiles::PREFIX),
                "name": profile.name,
                "description": profile.description,
                "entries": profile.entries,
            }))
            .collect::<Vec<_>>(),
    })
}

/// A drift comparison: what was compared, each difference, and the entries
/// that would put back what is missing.
pub fn drift(
    report: &helix_toolbox_engine::drift::Report,
    against: &crate::drift::Against,
    restore: &[String],
) -> Value {
    let (kind, baseline, profile) = match against {
        crate::drift::Against::Baseline(baseline) => (
            "baseline",
            json!({
                "name": baseline.name,
                "taken": baseline.taken,
                "system": baseline.system,
            }),
            Value::Null,
        ),
        crate::drift::Against::Profile(id) => ("profile", Value::Null, json!(id)),
    };
    json!({
        "against": kind,
        "baseline": baseline,
        "profile": profile,
        "matched": report.matched,
        "differences": report
            .differences
            .iter()
            .map(|difference| json!({
                "kind": difference.kind,
                "label": difference.kind.label(),
                "subject": difference.subject,
                "was": difference.was,
                "now": difference.now,
                "entry": difference.entry,
                "note": difference.note,
            }))
            .collect::<Vec<_>>(),
        "unknown": report.unknown,
        "restore": restore,
    })
}

pub fn baselines(found: &[(String, helix_toolbox_engine::drift::Baseline)]) -> Value {
    json!({
        "baselines": found
            .iter()
            .map(|(slug, baseline)| json!({
                "id": slug,
                "name": baseline.name,
                "taken": baseline.taken,
                "system": baseline.system,
                "entries": baseline.entries.len(),
                "files": baseline.files.len(),
                "settings": baseline.settings.len(),
                "applied": baseline.applied.len(),
            }))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_name_their_state() {
        let value = status(&Status::Update {
            source: "pacman".into(),
            version: Some("1-1".into()),
            latest: Some("2-1".into()),
            shadowed: None,
        });
        assert_eq!(value["state"], "update");
        assert_eq!(value["latest"], "2-1");
        assert_eq!(value["label"], "Update");
        assert_eq!(value["shadowed_by"], Value::Null);
        let value = status(&Status::Absent {
            replaces: vec!["wine".into()],
        });
        assert_eq!(value["state"], "absent");
        assert_eq!(value["replaces"], json!(["wine"]));
    }

    #[test]
    fn update_channels_keep_unknown_counts_apart_from_none() {
        let channel = |pending| updates::Channel {
            id: "pacman".into(),
            name: "Arch packages".into(),
            pending,
            note: String::new(),
            commands: vec![vec!["sudo".into(), "pacman".into(), "-Syu".into()]],
            requires: Vec::new(),
            caveat: None,
        };
        let value = updates(&[channel(None), channel(Some(Vec::new()))], false);
        assert_eq!(value["channels"][0]["count"], Value::Null);
        assert_eq!(value["channels"][0]["pending"], Value::Null);
        assert_eq!(value["channels"][1]["count"], 0);
        assert_eq!(
            value["channels"][1]["commands"],
            json!([["sudo", "pacman", "-Syu"]])
        );
    }

    #[test]
    fn control_characters_stay_escaped() {
        let text = serde_json::to_string(&json!({ "name": "evil\u{1b}[2J" })).unwrap();
        assert!(!text.contains('\u{1b}'));
        assert!(text.contains("\\u001b"));
    }
}
