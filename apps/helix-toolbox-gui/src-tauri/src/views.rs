//! What the window reads, built field by field from the engine's types, as
//! `--json` is, so the engine can change without changing the frontend.

use base64::Engine as _;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item};
use helix_toolbox_engine::{
    backups::Backup,
    confirmation::{Line, Prepared},
    doctor::{Diagnosis, Health},
    execution::ExecutionEvent,
    history::Record,
    inventory::Status,
    updates::Channel,
};
use serde::Serialize;
use serde_json::{Value, json};

/// A catalog area and what it lists.
#[derive(Debug, Serialize)]
pub struct AreaView {
    pub id: String,
    pub name: String,
    pub items: Vec<ItemView>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ItemView {
    Entry(EntryView),
    Group {
        id: String,
        name: String,
        description: String,
        items: Vec<ItemView>,
    },
}

#[derive(Debug, Serialize)]
pub struct EntryView {
    pub id: String,
    pub name: String,
    pub description: String,
    pub flags: String,
    /// package, script, command, nix, native or profile.
    pub kind: &'static str,
    /// Whether it can join others in the Install Plan; the others run alone.
    pub queueable: bool,
    /// Whether it asks questions while it runs.
    pub interactive: bool,
    /// The entries a profile runs, in order.
    pub runs: Vec<String>,
}

pub fn areas(catalog: &Catalog) -> Vec<AreaView> {
    catalog
        .tabs
        .iter()
        .map(|tab| AreaView {
            id: tab.id.clone(),
            name: tab.name.clone(),
            items: tab.items.iter().map(item).collect(),
        })
        .collect()
}

fn item(item: &Item) -> ItemView {
    match item {
        Item::Entry(entry) => ItemView::Entry(entry_view(entry)),
        Item::Group(group) => ItemView::Group {
            id: group.id.clone(),
            name: group.name.clone(),
            description: group.description.clone(),
            items: group.items.iter().map(self::item).collect(),
        },
    }
}

pub fn entry_view(entry: &Entry) -> EntryView {
    let (kind, interactive, runs) = match &entry.action {
        Action::Package { .. } => ("package", false, Vec::new()),
        Action::Script { interactive, .. } => ("script", *interactive, Vec::new()),
        Action::Command { .. } => ("command", false, Vec::new()),
        Action::Nix { .. } => ("nix", false, Vec::new()),
        Action::Native { .. } => ("native", false, Vec::new()),
        Action::Profile { entries } => ("profile", false, entries.clone()),
    };
    EntryView {
        id: entry.id.clone(),
        name: entry.name.clone(),
        description: entry.description.clone(),
        flags: entry.flags.clone(),
        kind,
        queueable: entry.multi_select,
        interactive,
        runs,
    }
}

/// How a package entry stands.
#[derive(Debug, Serialize)]
pub struct StatusView {
    /// installed, update, managed or absent: what color to show.
    pub kind: &'static str,
    /// The word the terminal interface shows.
    pub label: &'static str,
    /// Where it came from and at which version.
    pub detail: String,
}

pub fn status(status: &Status) -> StatusView {
    StatusView {
        kind: match status {
            Status::Installed { .. } | Status::Elsewhere { .. } => "installed",
            Status::Update { .. } => "update",
            Status::Managed { .. } => "managed",
            Status::Absent { .. } => "absent",
        },
        label: status.label(),
        detail: status.detail(),
    }
}

/// A plan ready to confirm, or why there is nothing to confirm.
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PreparedView {
    /// Nothing runs; the lines say why.
    Nothing { lines: Vec<String> },
    /// The checks before a run stop it; nothing was changed.
    Blocked { lines: Vec<String> },
    /// The plan could not be made.
    Error { lines: Vec<String> },
    Ready {
        header: Vec<String>,
        lines: Vec<Line>,
        risk: &'static str,
        privileged: bool,
    },
}

pub fn prepared(prepared: &Prepared) -> PreparedView {
    match prepared {
        Prepared::Nothing(lines) => PreparedView::Nothing {
            lines: lines.clone(),
        },
        Prepared::Blocked(lines) => PreparedView::Blocked {
            lines: lines.clone(),
        },
        Prepared::Ready(ready) => PreparedView::Ready {
            header: ready.header.clone(),
            lines: ready.lines.clone(),
            risk: ready.plan.risk.label(),
            privileged: ready.plan.privileged,
        },
    }
}

/// An event of a run, as `ExecutionEvent`'s JSON form, except that output
/// carries its bytes as base64 in `data`, not an array of numbers.
pub fn event(event: &ExecutionEvent) -> Value {
    match event {
        ExecutionEvent::Output { offset, bytes } => json!({
            "event": "output",
            "offset": offset,
            "data": base64::engine::general_purpose::STANDARD.encode(bytes),
        }),
        other => serde_json::to_value(other).unwrap_or_else(|error| {
            json!({"event": "warning", "message": format!("an event could not be read: {error}")})
        }),
    }
}

/// A health check's finding, for the Doctor screen.
#[derive(Debug, Serialize)]
pub struct DiagnosisView {
    pub id: String,
    pub title: String,
    /// good, notice or problem.
    pub health: &'static str,
    pub health_label: &'static str,
    pub summary: String,
    pub evidence: Vec<String>,
    pub advice: Option<String>,
    /// A repair the engine can run, with the commands it would run.
    pub repair: Option<RepairView>,
    /// The catalog entry that addresses it, when there is one.
    pub entry: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RepairView {
    pub label: String,
    pub commands: Vec<String>,
}

pub fn diagnosis(diagnosis: &Diagnosis) -> DiagnosisView {
    DiagnosisView {
        id: diagnosis.id.clone(),
        title: diagnosis.title.clone(),
        health: match diagnosis.health {
            Health::Good => "good",
            Health::Notice => "notice",
            Health::Problem => "problem",
        },
        health_label: diagnosis.health.label(),
        summary: diagnosis.summary.clone(),
        evidence: diagnosis.evidence.clone(),
        advice: diagnosis.advice.clone(),
        repair: diagnosis.repair.as_ref().map(|repair| RepairView {
            label: repair.label.clone(),
            commands: repair
                .commands
                .iter()
                .map(|command| command.join(" "))
                .collect(),
        }),
        entry: diagnosis.entry.clone(),
    }
}

/// An update channel, for the Updates screen.
#[derive(Debug, Serialize)]
pub struct ChannelView {
    pub id: String,
    pub name: String,
    /// How many updates wait; none when the channel cannot count them.
    pub count: Option<usize>,
    pub summary: String,
    pub pending: Option<Vec<PendingView>>,
    pub note: String,
    /// What updating runs; empty when there is nothing to run.
    pub command_line: String,
    pub caveat: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PendingView {
    pub name: String,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub fn channel(channel: &Channel) -> ChannelView {
    ChannelView {
        id: channel.id.clone(),
        name: channel.name.clone(),
        count: channel.count(),
        summary: channel.summary(),
        pending: channel.pending.as_ref().map(|pending| {
            pending
                .iter()
                .map(|item| PendingView {
                    name: helix_toolbox_core::printable(&item.name),
                    from: item.from.clone(),
                    to: item.to.clone(),
                })
                .collect()
        }),
        note: channel.note.clone(),
        command_line: if channel.commands.is_empty() {
            String::new()
        } else {
            channel.command_line()
        },
        caveat: channel.caveat.clone(),
    }
}

/// A backup Helix Toolbox or an installer left beside a file.
#[derive(Debug, Serialize)]
pub struct BackupView {
    /// Its place in the list, for `prepare_restore`.
    pub index: usize,
    pub original: String,
    pub backup: String,
    pub taken: Option<String>,
    pub system: bool,
    pub directory: bool,
}

pub fn backup(index: usize, backup: &Backup) -> BackupView {
    BackupView {
        index,
        original: helix_toolbox_core::printable(&backup.original.display().to_string()),
        backup: helix_toolbox_core::printable(&backup.backup.display().to_string()),
        taken: backup.taken.clone(),
        system: backup.system,
        directory: backup.directory,
    }
}

/// A past run, for the History screen.
#[derive(Debug, Serialize)]
pub struct RecordView {
    pub date: String,
    pub unix_timestamp: u64,
    pub names: Vec<String>,
    pub success: bool,
    pub result: String,
    pub log: Option<String>,
}

pub fn record(record: &Record) -> RecordView {
    RecordView {
        date: helix_toolbox_engine::date(record.unix_timestamp),
        unix_timestamp: record.unix_timestamp,
        names: record.names.clone(),
        success: record.success,
        result: record.result.clone(),
        log: record.log.as_ref().map(|log| log.display().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixtures() -> Catalog {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures");
        Catalog::from_directory(&root, true).expect("the fixture catalog loads")
    }

    #[test]
    fn the_catalog_becomes_areas_of_entries() {
        let areas = areas(&fixtures());
        assert_eq!(areas.len(), 1);
        assert_eq!(areas[0].name, "Tools");
        let value = serde_json::to_value(&areas[0]).unwrap();
        let items = value["items"].as_array().unwrap();
        let greeter = items
            .iter()
            .find(|item| item["id"] == "greeter")
            .expect("the greeter is listed");
        assert_eq!(greeter["type"], "entry");
        assert_eq!(greeter["kind"], "script");
        let bundle = items.iter().find(|item| item["id"] == "bundle").unwrap();
        assert_eq!(bundle["kind"], "profile");
        assert_eq!(bundle["runs"], json!(["marker", "report"]));
    }

    #[test]
    fn output_travels_as_base64_and_the_rest_as_the_engine_writes_it() {
        let output = event(&ExecutionEvent::Output {
            offset: 7,
            bytes: b"hi\r\n".to_vec(),
        });
        assert_eq!(
            output,
            json!({"event": "output", "offset": 7, "data": "aGkNCg=="})
        );
        let progress = event(&ExecutionEvent::Progress { ended: 1, total: 2 });
        assert_eq!(
            progress,
            json!({"event": "progress", "ended": 1, "total": 2})
        );
    }

    #[test]
    fn a_diagnosis_and_a_channel_keep_what_the_screens_show() {
        let found = Diagnosis {
            id: "dns".into(),
            title: "Name resolution".into(),
            health: Health::Problem,
            summary: "nothing resolves".into(),
            evidence: vec!["resolv.conf is empty".into()],
            advice: Some("set a nameserver".into()),
            repair: Some(helix_toolbox_engine::doctor::Repair {
                label: "Restart systemd-resolved".into(),
                commands: vec![vec![
                    "sudo".into(),
                    "systemctl".into(),
                    "restart".into(),
                    "systemd-resolved".into(),
                ]],
            }),
            entry: None,
        };
        let view = diagnosis(&found);
        assert_eq!(view.health, "problem");
        assert_eq!(view.health_label, found.health.label());
        assert_eq!(
            view.repair.as_ref().unwrap().commands,
            ["sudo systemctl restart systemd-resolved"]
        );
        let channel = channel(&Channel {
            id: "pacman".into(),
            name: "Arch packages".into(),
            pending: Some(vec![helix_toolbox_engine::updates::Pending {
                name: "ripgrep".into(),
                from: Some("1".into()),
                to: Some("2".into()),
            }]),
            note: String::new(),
            commands: vec![vec!["sudo".into(), "pacman".into(), "-Syu".into()]],
            requires: Vec::new(),
            caveat: None,
        });
        assert_eq!(channel.count, Some(1));
        assert_eq!(channel.command_line, "sudo pacman -Syu");
        assert_eq!(channel.pending.unwrap()[0].to.as_deref(), Some("2"));
    }

    #[test]
    fn statuses_carry_a_kind_for_their_color() {
        let absent = status(&Status::Absent {
            replaces: vec!["vim".into()],
        });
        assert_eq!(absent.kind, "absent");
        assert_eq!(absent.label, "Not installed");
        assert_eq!(absent.detail, "would replace vim");
    }
}
