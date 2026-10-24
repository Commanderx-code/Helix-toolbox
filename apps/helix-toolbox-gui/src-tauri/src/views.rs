//! What the window reads, built field by field from the engine's types, as
//! `--json` is, so the engine can change without changing the frontend.

use base64::Engine as _;
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item};
use helix_toolbox_engine::{
    RunSession,
    backups::Backup,
    bundle::{BundledEntry, Contents, Manifest},
    confirmation::{Line, Prepared, Verdict},
    doctor::{Diagnosis, Health},
    execution::ExecutionEvent,
    hardware::{self, Hardware},
    history::Record,
    inventory::Status,
    performance::{Boot, Comparison, Performance},
    preview::{Certainty, Change, StepPreview, Tag},
    profiles,
    rows::{self, Meter, Row},
    services::Services,
    session::Reading,
    storage::Storage,
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
    match &prepared.verdict {
        Verdict::Nothing(lines) => PreparedView::Nothing {
            lines: lines.clone(),
        },
        Verdict::Blocked(lines) => PreparedView::Blocked {
            lines: lines.clone(),
        },
        Verdict::Ready { header, lines } => PreparedView::Ready {
            header: header.clone(),
            lines: lines.clone(),
            risk: prepared.plan.risk.label(),
            privileged: prepared.plan.privileged,
        },
    }
}

/// A run the terminal panel closed: the queued entries that stay queued,
/// what to read again, and why it was not recorded, if it was not.
#[derive(Debug, Default, Serialize)]
pub struct ClosedView {
    pub remaining: Vec<String>,
    /// `statuses`, `updates`, `tools` or `history`.
    pub read_again: Vec<Reading>,
    pub not_recorded: Option<String>,
}

pub fn closed(
    session: &RunSession,
    known: &dyn Fn(&str) -> bool,
    not_recorded: Option<String>,
) -> ClosedView {
    ClosedView {
        remaining: session.remaining(known),
        read_again: session.read_again(),
        not_recorded,
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
    /// How it is downloaded, as `bundle create` says it.
    pub downloads: Vec<String>,
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
    /// What its files take: "48.2 MB".
    pub size: String,
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

pub fn bundle_contents(contents: &Contents, complete: bool) -> BundleContentsView {
    BundleContentsView {
        entries: contents.entries.iter().map(bundled).collect(),
        downloads: rows::downloads(contents, complete),
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
        size: rows::bundle_size(manifest),
        flatpaks: manifest
            .flatpaks
            .iter()
            .map(rows::bundled_flatpak)
            .collect(),
        entries: manifest.entries.iter().map(bundled).collect(),
    }
}

/// A display row (`rows::Row`) as the window draws it: the label over the
/// detail, the name beside them.
#[derive(Debug, Serialize)]
pub struct DisplayRowView {
    pub name: String,
    pub label: String,
    pub detail: String,
    pub attention: bool,
}

fn display_row(row: &Row) -> DisplayRowView {
    DisplayRowView {
        name: row.name.clone(),
        label: row.label.clone(),
        detail: row.detail.clone(),
        attention: row.attention,
    }
}

#[derive(Debug, Serialize)]
pub struct MeterView {
    pub percent: u32,
    pub full: bool,
}

fn meter(meter: Meter) -> MeterView {
    MeterView {
        percent: meter.percent,
        full: meter.full,
    }
}

// Hardware

/// The Hardware screen, row by row.
#[derive(Debug, Serialize)]
pub struct HardwareView {
    /// Why fwupd could not be read, and what it lists.
    pub firmware: Vec<String>,
    /// By their place in the list, which `prepare_firmware` takes.
    pub updates: Vec<DisplayRowView>,
    pub disks: Vec<DisplayRowView>,
    pub sensors: Vec<DisplayRowView>,
    pub batteries: Vec<DisplayRowView>,
    pub pci: Vec<DeviceView>,
    pub usb: Vec<DeviceView>,
}

/// A PCI or USB device; empty where it is not known.
#[derive(Debug, Serialize)]
pub struct DeviceView {
    pub address: String,
    pub name: String,
    pub class: String,
    pub driver: String,
}

pub fn hardware(found: &Hardware) -> HardwareView {
    let shown = rows::hardware(found);
    let devices = |devices: &[hardware::Device]| {
        devices
            .iter()
            .map(|device| DeviceView {
                address: device.address.clone(),
                name: device.name.clone(),
                class: device.class.clone().unwrap_or_default(),
                driver: device.driver.clone().unwrap_or_default(),
            })
            .collect()
    };
    HardwareView {
        firmware: shown
            .note
            .iter()
            .chain(&shown.firmware)
            .map(|row| row.label.clone())
            .collect(),
        updates: shown.updates.iter().map(display_row).collect(),
        disks: shown.disks.iter().map(display_row).collect(),
        sensors: shown.sensors.iter().map(display_row).collect(),
        batteries: shown.batteries.iter().map(display_row).collect(),
        pci: devices(&found.pci),
        usb: devices(&found.usb),
    }
}

// Services

#[derive(Debug, Serialize)]
pub struct ServicesView {
    /// Whether systemd runs this machine; without it there are no units.
    pub systemd: bool,
    pub units: Vec<UnitView>,
    pub autostart: Vec<AutostartView>,
}

#[derive(Debug, Serialize)]
pub struct UnitView {
    pub name: String,
    pub description: String,
    pub user: bool,
    /// service, timer or socket.
    pub kind: String,
    /// active, inactive, failed or "not loaded".
    pub state: String,
    /// Its enablement and description, for the list.
    pub detail: String,
    pub enablement: String,
    /// How it is now: "active (running)".
    pub now: String,
    /// Why enable and disable do not apply, when they do not.
    pub note: Option<&'static str>,
    pub running: bool,
    pub failed: bool,
    pub enabled: bool,
    /// Whether enable and disable apply.
    pub switchable: bool,
}

#[derive(Debug, Serialize)]
pub struct AutostartView {
    /// The file's name, which `prepare_autostart` takes.
    pub file: String,
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub enabled: bool,
    /// Whether this desktop starts it.
    pub here: bool,
    /// on, off or "not here".
    pub state: &'static str,
    pub runs: String,
    pub from: &'static str,
    pub at_login: &'static str,
}

pub fn services(found: &Services) -> ServicesView {
    ServicesView {
        systemd: found.systemd,
        units: found
            .units
            .iter()
            .map(|unit| {
                let shown = rows::unit(unit);
                UnitView {
                    name: unit.name.clone(),
                    description: unit.description.clone(),
                    user: unit.user,
                    kind: unit.kind().to_string(),
                    state: shown.state,
                    detail: shown.detail,
                    enablement: shown.enablement,
                    now: shown.now,
                    note: shown.note,
                    running: unit.running(),
                    failed: unit.failed(),
                    enabled: unit.enabled(),
                    switchable: unit.switchable(),
                }
            })
            .collect(),
        autostart: found
            .autostart
            .iter()
            .map(|item| {
                let shown = rows::autostart(item);
                AutostartView {
                    file: item.file.clone(),
                    name: item.name.clone(),
                    comment: item.comment.clone(),
                    exec: item.exec.clone(),
                    enabled: item.enabled,
                    here: item.here,
                    state: shown.state,
                    runs: shown.runs,
                    from: shown.from,
                    at_login: shown.at_login,
                }
            })
            .collect(),
    }
}

// Storage

#[derive(Debug, Serialize)]
pub struct StorageView {
    pub filesystems: Vec<SpaceView>,
    pub items: Vec<CleanView>,
    pub kept: Vec<KeptView>,
}

#[derive(Debug, Serialize)]
pub struct SpaceView {
    pub path: String,
    /// "40.0 GB free of 500 GB".
    pub text: String,
    pub meter: MeterView,
}

#[derive(Debug, Serialize)]
pub struct CleanView {
    /// What `prepare_clean` takes.
    pub id: String,
    pub title: String,
    pub detail: String,
    pub packages: Vec<String>,
    /// What it takes now, "?" when not known.
    pub size: String,
    /// What cleaning it frees, or why it is not offered.
    pub said: String,
    pub offered: bool,
}

#[derive(Debug, Serialize)]
pub struct KeptView {
    pub title: String,
    pub detail: String,
    pub size: Option<String>,
}

pub fn storage(found: &Storage) -> StorageView {
    StorageView {
        filesystems: found
            .filesystems
            .iter()
            .map(|filesystem| {
                let shown = rows::space(filesystem);
                SpaceView {
                    path: shown.path,
                    text: shown.text,
                    meter: meter(shown.meter),
                }
            })
            .collect(),
        items: found
            .items
            .iter()
            .map(|item| {
                let shown = rows::clean(item);
                CleanView {
                    id: item.id.clone(),
                    title: item.title.clone(),
                    detail: item.detail.clone(),
                    packages: item.packages.clone(),
                    size: shown.size,
                    said: shown.said,
                    offered: shown.offered,
                }
            })
            .collect(),
        kept: found
            .kept
            .iter()
            .map(|kept| KeptView {
                title: kept.title.clone(),
                detail: kept.detail.clone(),
                size: rows::kept_size(kept),
            })
            .collect(),
    }
}

// Performance

/// The boot, the machine now, and the boots recorded before this one.
#[derive(Debug, Serialize)]
pub struct PerformanceView {
    pub boot: Option<BootView>,
    /// Why there is no boot record.
    pub no_boot: Option<String>,
    /// This boot beside the one before it.
    pub previous: Option<ComparisonView>,
    /// The recorded boots before this one, which it can be set beside.
    pub earlier: Vec<BootSummaryView>,
    pub now: Vec<FactView>,
    pub hints: Vec<HintView>,
}

#[derive(Debug, Serialize)]
pub struct BootView {
    pub id: String,
    /// Seconds since 1970.
    pub booted: u64,
    pub kernel: String,
    /// The whole boot: "26.5 s".
    pub total: String,
    pub parts: Vec<PartView>,
    /// What the login screen waited for.
    pub waited: Vec<TimedView>,
    pub slowest: Vec<TimedView>,
}

#[derive(Debug, Serialize)]
pub struct PartView {
    pub name: &'static str,
    /// Milliseconds, for its share of the bar.
    pub took: u64,
    pub shown: String,
    /// Its color: firmware, loader, kernel, initrd or services.
    pub tone: &'static str,
}

#[derive(Debug, Serialize)]
pub struct TimedView {
    pub name: String,
    pub took: String,
}

#[derive(Debug, Serialize)]
pub struct BootSummaryView {
    pub id: String,
    pub booted: u64,
    pub total: String,
}

#[derive(Debug, Serialize)]
pub struct ComparisonView {
    /// The earlier boot.
    pub before: BootSummaryView,
    /// The whole boot's change: "+5.0 s".
    pub total: String,
    /// Positive when the later boot was slower, negative when faster.
    pub sign: i8,
    pub changes: Vec<BootChangeView>,
    /// The kernels before and after, when they differ.
    pub kernel: Option<(String, String)>,
    pub runs: Vec<RunView>,
}

#[derive(Debug, Serialize)]
pub struct BootChangeView {
    pub name: String,
    pub now: String,
    pub how: String,
    pub slower: bool,
}

#[derive(Debug, Serialize)]
pub struct RunView {
    pub at: u64,
    /// What ran, "(failed)" after it when it failed.
    pub ran: String,
}

#[derive(Debug, Serialize)]
pub struct FactView {
    pub term: &'static str,
    pub text: String,
    pub note: String,
    pub meter: Option<MeterView>,
}

#[derive(Debug, Serialize)]
pub struct HintView {
    pub title: String,
    pub detail: String,
    pub command: Option<String>,
    pub unit: Option<String>,
    pub entry: Option<String>,
}

fn boot_summary(boot: &Boot) -> BootSummaryView {
    BootSummaryView {
        id: boot.id.clone(),
        booted: boot.booted,
        total: rows::boot(boot).total,
    }
}

pub fn performance(found: &Performance, boots: &[Boot]) -> PerformanceView {
    let timed = |list: Vec<(String, String)>| {
        list.into_iter()
            .map(|(name, took)| TimedView { name, took })
            .collect()
    };
    PerformanceView {
        boot: found.boot.as_ref().map(|boot| {
            let shown = rows::boot(boot);
            BootView {
                id: boot.id.clone(),
                booted: boot.booted,
                kernel: boot.kernel.clone(),
                total: shown.total,
                parts: shown
                    .parts
                    .into_iter()
                    .map(|part| PartView {
                        tone: match part.name {
                            "Firmware" => "firmware",
                            "Boot loader" => "loader",
                            "Kernel" => "kernel",
                            "Initial RAM disk" => "initrd",
                            _ => "services",
                        },
                        name: part.name,
                        took: part.took,
                        shown: part.shown,
                    })
                    .collect(),
                waited: timed(shown.waited),
                slowest: timed(shown.slowest),
            }
        }),
        no_boot: found.no_boot.clone(),
        previous: found.previous.as_ref().map(comparison),
        earlier: found
            .boot
            .as_ref()
            .map(|boot| {
                boots
                    .iter()
                    .filter(|other| other.id != boot.id && other.booted <= boot.booted)
                    .map(boot_summary)
                    .collect()
            })
            .unwrap_or_default(),
        now: rows::now(&found.now)
            .into_iter()
            .map(|fact| FactView {
                term: fact.term,
                text: fact.text,
                note: fact.note,
                meter: fact.meter.map(meter),
            })
            .collect(),
        hints: found
            .hints
            .iter()
            .map(|hint| HintView {
                title: hint.title.clone(),
                detail: hint.detail.clone(),
                command: hint.command.clone(),
                unit: hint.unit.clone(),
                entry: hint.entry.clone(),
            })
            .collect(),
    }
}

pub fn comparison(compared: &Comparison) -> ComparisonView {
    let shown = rows::comparison(compared);
    ComparisonView {
        before: boot_summary(&compared.before),
        total: shown.total,
        sign: shown.sign,
        changes: shown
            .changes
            .into_iter()
            .map(|change| BootChangeView {
                name: change.name,
                now: change.now,
                how: change.how,
                slower: change.slower,
            })
            .collect(),
        kernel: shown.kernel,
        runs: shown
            .runs
            .into_iter()
            .map(|(at, ran)| RunView { at, ran })
            .collect(),
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
    fn the_hardware_screen_gets_the_engines_rows() {
        use helix_toolbox_engine::hardware::{Device, Disk, Sensor};
        let found = Hardware {
            disks: vec![Disk {
                name: "nvme0n1".into(),
                model: Some("Sample SSD".into()),
                bytes: Some(1_000_204_886_016),
                kind: "NVMe".into(),
                ..Disk::default()
            }],
            sensors: vec![Sensor {
                chip: "coretemp".into(),
                label: "Package".into(),
                celsius: 92.0,
                high: None,
                critical: Some(100.0),
            }],
            pci: vec![Device {
                address: "00:02.0".into(),
                name: "Graphics".into(),
                class: None,
                driver: Some("i915".into()),
            }],
            ..Hardware::default()
        };
        let value = serde_json::to_value(hardware(&found)).unwrap();
        assert_eq!(value["disks"][0]["detail"], "NVMe · 1.0 TB");
        // Eight degrees under critical: not hot, as the engine has it.
        assert_eq!(value["sensors"][0]["label"], "92 °C");
        assert_eq!(value["sensors"][0]["attention"], false);
        assert_eq!(value["pci"][0]["class"], "");
        assert_eq!(value["pci"][0]["driver"], "i915");
        assert_eq!(value["firmware"], json!([]));
    }

    #[test]
    fn a_unit_says_what_applies_to_it() {
        use helix_toolbox_engine::services::Unit;
        let found = Services {
            systemd: true,
            units: vec![Unit {
                name: "logrotate.timer".into(),
                description: "Rotate logs".into(),
                enablement: Some("static".into()),
                active: Some("failed".into()),
                sub: Some("failed".into()),
                user: false,
            }],
            ..Services::default()
        };
        let value = serde_json::to_value(services(&found)).unwrap();
        let unit = &value["units"][0];
        assert_eq!(unit["kind"], "timer");
        assert_eq!(unit["detail"], "static · Rotate logs");
        assert_eq!(unit["now"], "failed");
        assert_eq!(unit["failed"], true);
        assert_eq!(unit["switchable"], false);
        assert!(
            unit["note"]
                .as_str()
                .unwrap()
                .starts_with("It is not enabled")
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
