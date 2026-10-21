//! What the window reads, built field by field from the engine's types, as
//! `--json` is, so the engine can change without changing the frontend.

use base64::Engine as _;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item};
use helix_toolbox_engine::{
    backups::Backup,
    bundle::{BundledEntry, Contents, Manifest},
    confirmation::{Line, Prepared},
    doctor::{Diagnosis, Health},
    execution::ExecutionEvent,
    history::Record,
    inventory::Status,
    preview::{Certainty, Change, StepPreview, Tag},
    profiles,
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
    /// A package entry that offers removal.
    pub removable: bool,
    /// One of the user's own profiles, which can be exported and deleted.
    pub yours: bool,
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
            items: tab.items.iter().map(|found| item(found, false)).collect(),
        })
        .collect()
}

/// `yours`: inside the user's profiles' group.
fn item(item: &Item, yours: bool) -> ItemView {
    match item {
        Item::Entry(entry) => ItemView::Entry(EntryView {
            yours,
            ..entry_view(entry)
        }),
        Item::Group(group) => ItemView::Group {
            id: group.id.clone(),
            name: group.name.clone(),
            description: group.description.clone(),
            items: group
                .items
                .iter()
                .map(|found| self::item(found, group.id == profiles::GROUP))
                .collect(),
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
        removable: matches!(
            entry.action,
            Action::Package {
                removable: true,
                ..
            }
        ),
        yours: false,
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

/// What a plan is expected to change, step by step, as the terminal
/// interface's change preview shows it.
#[derive(Debug, Serialize)]
pub struct ChangesView {
    pub steps: Vec<StepChangesView>,
    /// A pinned source the plan reads is not downloaded yet; with it, the
    /// files it writes can be compared.
    pub download: bool,
}

#[derive(Debug, Serialize)]
pub struct StepChangesView {
    pub name: String,
    pub changes: Vec<ChangeView>,
}

#[derive(Debug, Serialize)]
pub struct ChangeView {
    /// known, expected or unknown: how the change is known.
    pub certainty: &'static str,
    pub text: String,
    pub diff: Option<DiffView>,
}

/// A file now and after the step, the differing rows with three around
/// each; a null row marks rows left out.
#[derive(Debug, Serialize)]
pub struct DiffView {
    pub path: String,
    pub changed: bool,
    pub rows: Vec<Option<RowView>>,
}

#[derive(Debug, Serialize)]
pub struct RowView {
    pub before: Option<String>,
    pub after: Option<String>,
    /// same, changed, removed or added.
    pub tag: &'static str,
}

pub fn changes(steps: &[StepPreview], download: bool) -> ChangesView {
    ChangesView {
        steps: steps
            .iter()
            .map(|step| StepChangesView {
                name: step.name.clone(),
                changes: step.changes.iter().map(change).collect(),
            })
            .collect(),
        download,
    }
}

fn change(change: &Change) -> ChangeView {
    ChangeView {
        certainty: match change.certainty {
            Certainty::Known => "known",
            Certainty::Expected => "expected",
            Certainty::Unknown => "unknown",
        },
        text: change.text.clone(),
        diff: change.diff.as_ref().map(|diff| DiffView {
            path: diff.path.clone(),
            changed: diff.changed,
            rows: diff
                .hunks(3)
                .into_iter()
                .map(|row| {
                    row.map(|row| RowView {
                        before: row.before.clone(),
                        after: row.after.clone(),
                        tag: match row.tag {
                            Tag::Same => "same",
                            Tag::Changed => "changed",
                            Tag::Removed => "removed",
                            Tag::Added => "added",
                        },
                    })
                })
                .collect(),
        }),
    }
}

/// What a bundle of some entries would hold, before anything is
/// downloaded.
#[derive(Debug, Serialize)]
pub struct BundleContentsView {
    pub entries: Vec<BundledEntryView>,
    /// Package files to download, not counting their signatures.
    pub files: usize,
    /// What dnf or zypper downloads itself, with the dependencies.
    pub dnf: Vec<String>,
    pub zypper: Vec<String>,
    /// Flatpaks copied from this machine's installation.
    pub flatpaks: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BundledEntryView {
    pub name: String,
    pub offline: bool,
    /// Why it needs the network, if it does.
    pub network: Option<String>,
    pub flatpak: Option<String>,
}

/// A bundle on this machine that matched its checksums. Its text comes
/// from the bundle, so it is made printable.
#[derive(Debug, Serialize)]
pub struct BundleView {
    pub distribution: String,
    pub architecture: String,
    /// The Helix Toolbox it was made with, and whether a release build.
    pub version: String,
    pub release: bool,
    pub created: String,
    pub complete: bool,
    pub files: usize,
    pub bytes: u64,
    pub flatpaks: Vec<String>,
    pub entries: Vec<BundledEntryView>,
}

fn bundled(entry: &BundledEntry) -> BundledEntryView {
    BundledEntryView {
        name: helix_toolbox_core::printable(&entry.name),
        offline: entry.offline,
        network: entry.network.as_deref().map(helix_toolbox_core::printable),
        flatpak: entry.flatpak.as_deref().map(helix_toolbox_core::printable),
    }
}

pub fn bundle_contents(contents: &Contents) -> BundleContentsView {
    BundleContentsView {
        entries: contents.entries.iter().map(bundled).collect(),
        files: contents
            .downloads
            .iter()
            .filter(|download| !download.file.ends_with(".sig"))
            .count(),
        dnf: contents.dnf.clone(),
        zypper: contents.zypper.clone(),
        flatpaks: contents
            .flatpaks
            .iter()
            .map(|flatpak| helix_toolbox_core::printable(&flatpak.reference))
            .collect(),
    }
}

pub fn bundle(manifest: &Manifest) -> BundleView {
    let printable = helix_toolbox_core::printable;
    BundleView {
        distribution: printable(&manifest.distribution),
        architecture: printable(&manifest.architecture),
        version: printable(&manifest.helixforge),
        release: manifest.release,
        created: helix_toolbox_engine::date(manifest.created),
        complete: manifest.complete,
        files: manifest.files.len(),
        bytes: manifest.files.iter().map(|file| file.bytes).sum(),
        flatpaks: manifest
            .flatpaks
            .iter()
            .map(|flatpak| {
                format!(
                    "{} at commit {}, from {}",
                    printable(&flatpak.reference),
                    printable(flatpak.commit.get(..12).unwrap_or(&flatpak.commit)),
                    printable(&flatpak.remote)
                )
            })
            .collect(),
        entries: manifest.entries.iter().map(bundled).collect(),
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
    fn a_package_entry_says_whether_it_offers_removal() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/packages");
        let catalog = Catalog::from_directory(&root, true).expect("the package fixtures load");
        let sample = catalog.find("fallback-sample").unwrap();
        assert!(entry_view(sample).removable);
        let greeter = fixtures();
        assert!(!entry_view(greeter.find("greeter").unwrap()).removable);
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

    #[test]
    fn a_changed_file_comes_with_its_differing_rows_only() {
        let before: String = (1..=20).map(|n| format!("line {n}\n")).collect();
        let after = before.replace("line 2\n", "line two\n");
        let file = Change {
            certainty: Certainty::Known,
            area: helix_toolbox_engine::preview::Area::File,
            text: "replaces ~/.bashrc".into(),
            diff: helix_toolbox_engine::preview::diff("~/.bashrc", &before, &after),
        };
        let service = Change {
            certainty: Certainty::Unknown,
            area: helix_toolbox_engine::preview::Area::Service,
            text: "may restart a unit it chooses".into(),
            diff: None,
        };
        let steps = [StepPreview {
            id: "bash".into(),
            name: "Bash".into(),
            changes: vec![file, service],
        }];
        let value = serde_json::to_value(changes(&steps, true)).unwrap();
        assert_eq!(value["download"], true);
        let shown = &value["steps"][0]["changes"];
        assert_eq!(shown[0]["certainty"], "known");
        assert_eq!(shown[1]["certainty"], "unknown");
        assert!(shown[1]["diff"].is_null());
        let diff = &shown[0]["diff"];
        assert_eq!(diff["changed"], true);
        // Line 2 and three on either side; the rest is left out.
        let rows = diff["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[1]["tag"], "changed");
        assert_eq!(rows[1]["before"], "line 2");
        assert_eq!(rows[1]["after"], "line two");
    }
}
