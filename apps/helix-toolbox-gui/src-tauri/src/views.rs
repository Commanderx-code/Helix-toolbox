//! What the window reads, built field by field from the engine's types, as
//! `--json` is, so the engine can change without changing the frontend.

use base64::Engine as _;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item};
use helix_toolbox_engine::{
    confirmation::{Line, Prepared},
    execution::ExecutionEvent,
    history::Record,
    inventory::Status,
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
    fn statuses_carry_a_kind_for_their_color() {
        let absent = status(&Status::Absent {
            replaces: vec!["vim".into()],
        });
        assert_eq!(absent.kind, "absent");
        assert_eq!(absent.label, "Not installed");
        assert_eq!(absent.detail, "would replace vim");
    }
}
