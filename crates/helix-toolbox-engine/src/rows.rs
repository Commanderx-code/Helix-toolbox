//! Display rows: what the engine found, turned into the text both
//! interfaces show. The terminal interface draws these rows and the
//! window's `views.rs` serializes them, so a size, a threshold or a
//! sentence is decided once, here, and the screens only lay it out.
//!
//! A row often has two forms of the same text: `label` and `detail` for a
//! row of two lines (the window), and `line` for a list of single lines
//! (the terminal interface). Both are built from the same pieces. What a
//! key does stays with the interface that has the key.

use crate::{
    bundle::{BundledEntry, BundledFlatpak, Contents, Manifest},
    hardware::{self, Hardware},
    performance::{self, Boot, Change, Comparison, Now},
    services::{Autostart, Unit},
    storage::{self, Filesystem, Item, Kept},
};
use helix_toolbox_core::printable;
use std::path::Path;

/// One line of a screen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// What it is, when the row names something: a disk's device name, a
    /// sensor's chip and label.
    pub name: String,
    /// The main text.
    pub label: String,
    /// The facts after the label, joined with " · ".
    pub detail: String,
    /// The whole row on one line, for a list of single lines.
    pub line: String,
    /// Whether it wants attention: a hot sensor, a worn battery, an update.
    pub attention: bool,
    /// The longer account, shown on request.
    pub more: String,
}

/// How full something is, for a bar.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Meter {
    /// Used, in percent, rounded.
    pub percent: u32,
    /// Over nine tenths used: worth a warning color.
    pub full: bool,
}

impl Meter {
    pub fn of(used: u64, total: u64) -> Self {
        let share = used.min(total) as f64 / total.max(1) as f64;
        Meter {
            percent: (share * 100.0).round() as u32,
            full: share > 0.9,
        }
    }
}

/// The non-empty parts, joined with " · ".
fn facts<const N: usize>(parts: [String; N]) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ")
}

/// A temperature for people: "38 °C".
pub fn celsius(degrees: f64) -> String {
    format!("{degrees:.0} °C")
}

// Hardware

/// The Hardware screen's rows, section by section; each interface puts
/// them in its own order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HardwareRows {
    /// Why fwupd could not be read, when it could not.
    pub note: Option<Row>,
    /// How many devices fwupd lists and how many it can update.
    pub firmware: Option<Row>,
    /// The firmware updates, in the engine's order: a row's index is its
    /// update's index.
    pub updates: Vec<Row>,
    pub disks: Vec<Row>,
    /// Every sensor, for a grid of them.
    pub sensors: Vec<Row>,
    /// The sensors in one row: how many, the warmest, how many are hot.
    pub temperatures: Row,
    pub batteries: Vec<Row>,
    pub pci: Vec<Row>,
    pub usb: Vec<Row>,
}

/// Below this share of its design capacity, a battery is worth replacing.
pub const WORN_BATTERY: u32 = 70;

pub fn hardware(found: &Hardware) -> HardwareRows {
    HardwareRows {
        note: found.firmware.note.as_ref().map(|note| Row {
            label: note.clone(),
            line: note.clone(),
            more: note.clone(),
            ..Row::default()
        }),
        firmware: found.firmware.available.then(|| firmware(found)),
        updates: found.firmware.updates.iter().map(update).collect(),
        disks: found.disks.iter().map(disk).collect(),
        sensors: found.sensors.iter().map(sensor).collect(),
        temperatures: temperatures(&found.sensors),
        batteries: found.batteries.iter().map(battery).collect(),
        pci: found.pci.iter().map(device).collect(),
        usb: found.usb.iter().map(device).collect(),
    }
}

fn firmware(found: &Hardware) -> Row {
    let updatable: Vec<&hardware::FirmwareDevice> = found
        .firmware
        .devices
        .iter()
        .filter(|device| device.updatable)
        .collect();
    let listing: Vec<String> = updatable
        .iter()
        .map(|device| match &device.version {
            Some(version) => format!("{} {version}", device.name),
            None => device.name.clone(),
        })
        .collect();
    let label = format!(
        "{} devices, {} updatable through fwupd{}",
        found.firmware.devices.len(),
        updatable.len(),
        if found.firmware.updates.is_empty() {
            "; no updates in the metadata fwupd has"
        } else {
            ""
        }
    );
    Row {
        line: label.clone(),
        label,
        more: format!(
            "Updatable through fwupd:\n{}\n\nThe updates come from the metadata fwupd already has; fwupdmgr refresh fetches newer metadata.",
            listing.join("\n")
        ),
        ..Row::default()
    }
}

fn update(update: &hardware::FirmwareUpdate) -> Row {
    let versions = format!("{} → {}", update.from.as_deref().unwrap_or("?"), update.to);
    Row {
        name: String::new(),
        label: update.device.clone(),
        detail: facts([
            versions.clone(),
            update.summary.clone().unwrap_or_default(),
            update.urgency.clone().unwrap_or_default(),
        ]),
        line: format!("Update for {}: {versions}", update.device),
        attention: true,
        more: format!(
            "{}\nNow: {}\nAvailable: {}\nUrgency: {}\n\n{}",
            update.device,
            update.from.as_deref().unwrap_or("unknown"),
            update.to,
            update.urgency.as_deref().unwrap_or("not given"),
            update.summary.as_deref().unwrap_or("No summary.")
        ),
    }
}

fn disk(disk: &hardware::Disk) -> Row {
    let size = disk.bytes.map(hardware::size).unwrap_or_default();
    let model = disk.model.as_deref().unwrap_or("unknown model");
    let temperature = disk.celsius.map(celsius).unwrap_or_default();
    let mut line = format!("{}  {}  {size}  {model}", disk.name, disk.kind);
    if !temperature.is_empty() {
        line.push_str(&format!("  {temperature}"));
    }
    Row {
        name: disk.name.clone(),
        label: model.to_string(),
        detail: facts([
            disk.kind.clone(),
            size,
            disk.firmware
                .as_deref()
                .map(|firmware| format!("firmware {firmware}"))
                .unwrap_or_default(),
            temperature,
        ]),
        more: format!(
            "{line}\nFirmware: {}",
            disk.firmware.as_deref().unwrap_or("unknown")
        ),
        line,
        attention: false,
    }
}

fn sensor(sensor: &hardware::Sensor) -> Row {
    let name = format!("{} {}", sensor.chip, sensor.label);
    let critical = sensor
        .critical
        .map(|critical| format!("critical {critical:.0}"))
        .unwrap_or_default();
    Row {
        line: format!(
            "{name}: {:.1} °C{}",
            sensor.celsius,
            if critical.is_empty() {
                String::new()
            } else {
                format!(" ({critical})")
            }
        ),
        name,
        label: celsius(sensor.celsius),
        detail: critical,
        attention: sensor.hot(),
        more: String::new(),
    }
}

fn temperatures(sensors: &[hardware::Sensor]) -> Row {
    let Some(warmest) = sensors
        .iter()
        .max_by(|a, b| a.celsius.total_cmp(&b.celsius))
    else {
        let label = "No temperature sensors found".to_string();
        return Row {
            line: label.clone(),
            label,
            ..Row::default()
        };
    };
    let hot = sensors.iter().filter(|sensor| sensor.hot()).count();
    let label = format!(
        "{} sensors; the warmest {} {} at {}{}",
        sensors.len(),
        warmest.chip,
        warmest.label,
        celsius(warmest.celsius),
        if hot == 0 {
            String::new()
        } else {
            format!("; {hot} near their limit")
        }
    );
    Row {
        line: label.clone(),
        label,
        attention: hot > 0,
        more: sensors
            .iter()
            .map(|sensor| self::sensor(sensor).line)
            .collect::<Vec<_>>()
            .join("\n"),
        ..Row::default()
    }
}

fn battery(battery: &hardware::Battery) -> Row {
    let charge = battery
        .capacity
        .map(|capacity| format!("{capacity}% charged"))
        .unwrap_or_else(|| "charge unknown".to_string());
    let health = battery
        .health
        .map(|health| format!("holds {health}% of its design capacity"))
        .unwrap_or_default();
    let cycles = battery
        .cycles
        .map(|cycles| format!("{cycles} cycles"))
        .unwrap_or_default();
    let line = format!(
        "{}: {}",
        battery.name,
        [charge.as_str(), health.as_str(), cycles.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(", ")
    );
    Row {
        name: battery.name.clone(),
        label: match &battery.status {
            Some(status) => format!("{charge}, {}", status.to_lowercase()),
            None => charge.clone(),
        },
        detail: facts([
            health,
            cycles,
            battery.model.clone().unwrap_or_default(),
            battery.technology.clone().unwrap_or_default(),
        ]),
        attention: battery.health.is_some_and(|health| health < WORN_BATTERY),
        more: format!(
            "{line}\nStatus: {}\nModel: {}\nType: {}\n\nBelow about {WORN_BATTERY}% of its design capacity a battery is usually worth replacing.",
            battery.status.as_deref().unwrap_or("unknown"),
            battery.model.as_deref().unwrap_or("unknown"),
            battery.technology.as_deref().unwrap_or("unknown")
        ),
        line,
    }
}

fn device(device: &hardware::Device) -> Row {
    let line = match &device.driver {
        Some(driver) => format!("{}  [{driver}]", device.name),
        None => device.name.clone(),
    };
    Row {
        name: device.address.clone(),
        label: device.name.clone(),
        detail: device.class.clone().unwrap_or_default(),
        more: format!(
            "{line}\nAddress: {}{}",
            device.address,
            device
                .class
                .as_deref()
                .map(|class| format!("\nClass or id: {class}"))
                .unwrap_or_default()
        ),
        line,
        attention: false,
    }
}

// Services

/// A systemd unit's state in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct UnitRow {
    /// active, inactive, failed, or "not loaded".
    pub state: String,
    /// Its enablement and its description: "enabled · CUPS Scheduler".
    pub detail: String,
    /// Whether it starts at boot or login: enabled, static, or "no unit
    /// file of its own".
    pub enablement: String,
    /// How it is now, with systemd's finer state when that says more:
    /// "active (running)".
    pub now: String,
    /// Why enable and disable do not apply, when they do not.
    pub note: Option<&'static str>,
}

pub fn unit(unit: &Unit) -> UnitRow {
    let state = unit.active.as_deref().unwrap_or("not loaded");
    UnitRow {
        state: state.to_string(),
        detail: facts([
            unit.enablement
                .clone()
                .unwrap_or_else(|| "no unit file".to_string()),
            unit.description.clone(),
        ]),
        enablement: unit
            .enablement
            .clone()
            .unwrap_or_else(|| "no unit file of its own".to_string()),
        now: match unit.sub.as_deref() {
            Some(sub) if Some(sub) != unit.active.as_deref() => format!("{state} ({sub})"),
            _ => state.to_string(),
        },
        note: (!unit.switchable()).then_some(
            "It is not enabled or disabled itself: another unit or a request starts it.",
        ),
    }
}

/// An application started at login, in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AutostartRow {
    /// on, off, or "not here" when this desktop does not start it.
    pub state: &'static str,
    /// What it runs.
    pub runs: String,
    /// Whose file it is.
    pub from: &'static str,
    /// Whether it starts at login here.
    pub at_login: &'static str,
}

pub fn autostart(item: &Autostart) -> AutostartRow {
    AutostartRow {
        state: match (item.here, item.enabled) {
            (false, _) => "not here",
            (true, true) => "on",
            (true, false) => "off",
        },
        runs: if item.exec.is_empty() {
            "nothing named".to_string()
        } else {
            item.exec.clone()
        },
        from: match (item.system, item.own.is_some()) {
            (true, true) => "the system · your file overrides it",
            (true, false) => "the system",
            (false, _) => "your own",
        },
        at_login: if !item.here {
            "not in this desktop"
        } else if item.enabled {
            "started"
        } else {
            "not started"
        },
    }
}

// Storage

/// A file system's space.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SpaceRow {
    pub path: String,
    /// "40.0 GB free of 500 GB".
    pub text: String,
    pub meter: Meter,
}

pub fn space(filesystem: &Filesystem) -> SpaceRow {
    SpaceRow {
        path: filesystem.path.clone(),
        text: format!(
            "{} free of {}",
            storage::shown(filesystem.free),
            storage::shown(filesystem.total)
        ),
        meter: Meter::of(
            filesystem.total.saturating_sub(filesystem.free),
            filesystem.total,
        ),
    }
}

/// Something that can be cleaned, in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CleanRow {
    /// What it takes now, "?" when that is not known.
    pub size: String,
    /// What cleaning it frees, or why it is not offered.
    pub said: String,
    /// Whether it is offered here.
    pub offered: bool,
}

pub fn clean(item: &Item) -> CleanRow {
    CleanRow {
        size: item
            .size
            .map(storage::shown)
            .unwrap_or_else(|| "?".to_string()),
        said: match (&item.not_offered, item.frees) {
            (Some(why), _) => why.clone(),
            (None, Some(frees)) => format!("frees {}", storage::shown(frees)),
            (None, None) => "frees what it finds".to_string(),
        },
        offered: item.not_offered.is_none(),
    }
}

/// What a kept item takes, when that is known.
pub fn kept_size(kept: &Kept) -> Option<String> {
    kept.size.map(storage::shown)
}

// Performance

/// A boot in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BootRows {
    /// The whole boot: "26.5 s".
    pub total: String,
    /// Each part that is known, in order.
    pub parts: Vec<Part>,
    /// What the login screen waited for, with how long each took.
    pub waited: Vec<(String, String)>,
    /// The slowest services since the boot, with how long each took.
    pub slowest: Vec<(String, String)>,
}

/// A part of a boot.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Part {
    /// "Kernel", "Initial RAM disk".
    pub name: &'static str,
    pub took: u64,
    /// "1.0 s".
    pub shown: String,
}

pub fn boot(boot: &Boot) -> BootRows {
    BootRows {
        total: performance::shown(boot.phases.total),
        parts: boot
            .phases
            .parts()
            .into_iter()
            .map(|(name, took)| Part {
                name,
                took,
                shown: performance::shown(took),
            })
            .collect(),
        waited: boot
            .chain
            .iter()
            .filter_map(|link| Some((link.name.clone(), performance::shown(link.took?))))
            .collect(),
        slowest: boot
            .slowest
            .iter()
            .map(|timed| (timed.name.clone(), performance::shown(timed.took)))
            .collect(),
    }
}

/// A change smaller than this, in milliseconds, is not worth a row.
pub const NOTICEABLE_MS: u64 = 100;

/// How a part or a unit changed between two boots.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ChangeRow {
    pub name: String,
    /// How long it takes now, or "not now".
    pub now: String,
    /// "+1.2 s", "-850 ms", "new" or "gone".
    pub how: String,
    /// Slower, or new: worth a warning color.
    pub slower: bool,
}

/// A change worth a row: a tenth of a second or more, or new, or gone.
pub fn change(change: &Change) -> Option<ChangeRow> {
    let (how, slower) = match change.difference() {
        Some(difference) if difference.unsigned_abs() < NOTICEABLE_MS => return None,
        Some(difference) => (performance::shown_change(difference), difference > 0),
        None if change.before.is_none() => ("new".to_string(), true),
        None => ("gone".to_string(), false),
    };
    Some(ChangeRow {
        name: change.name.clone(),
        now: change
            .after
            .map(performance::shown)
            .unwrap_or_else(|| "not now".to_string()),
        how,
        slower,
    })
}

/// A later boot beside an earlier one, in words.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ComparisonRows {
    /// The whole boot's change: "+5.0 s".
    pub total: String,
    /// Greater than zero when the later boot was slower, less when faster.
    pub sign: i8,
    /// The parts after the whole boot, then the units, worth a row.
    pub changes: Vec<ChangeRow>,
    /// The kernels before and after, when they differ.
    pub kernel: Option<(String, String)>,
    /// Helix Toolbox's runs in between, oldest first: when, and what ran.
    pub runs: Vec<(u64, String)>,
}

pub fn comparison(compared: &Comparison) -> ComparisonRows {
    ComparisonRows {
        total: performance::shown_change(compared.total()),
        sign: compared.total().signum() as i8,
        changes: compared
            .phases
            .iter()
            .skip(1)
            .chain(&compared.units)
            .filter_map(change)
            .collect(),
        kernel: (!compared.before.kernel.is_empty()
            && compared.before.kernel != compared.after.kernel)
            .then(|| {
                (
                    compared.before.kernel.clone(),
                    compared.after.kernel.clone(),
                )
            }),
        runs: compared
            .runs
            .iter()
            .map(|run| {
                (
                    run.at,
                    format!(
                        "{}{}",
                        run.names.join(", "),
                        if run.success { "" } else { " (failed)" }
                    ),
                )
            })
            .collect(),
    }
}

/// One fact about the machine now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Fact {
    /// "Memory", "Swap".
    pub term: &'static str,
    pub text: String,
    /// What the text means, said after it: "(1, 5 and 15 minutes) on 8
    /// processors".
    pub note: String,
    /// How full, for a bar.
    pub meter: Option<Meter>,
}

/// The machine now, fact by fact.
pub fn now(now: &Now) -> Vec<Fact> {
    let fact = |term, text: String| Fact {
        term,
        text,
        ..Fact::default()
    };
    let mut facts = Vec::new();
    if let Some([one, five, fifteen]) = now.load {
        facts.push(Fact {
            note: format!("(1, 5 and 15 minutes) on {} processors", now.cpus),
            ..fact("Load", format!("{one:.2} {five:.2} {fifteen:.2}"))
        });
    }
    if now.memory_total > 0 {
        facts.push(Fact {
            meter: Some(Meter::of(
                now.memory_total.saturating_sub(now.memory_available),
                now.memory_total,
            )),
            ..fact(
                "Memory",
                format!(
                    "{} free of {}",
                    storage::shown(now.memory_available),
                    storage::shown(now.memory_total)
                ),
            )
        });
    }
    facts.push(fact(
        "Swap",
        if now.swaps.is_empty() {
            "none".to_string()
        } else {
            format!(
                "{} used of {}{}",
                storage::shown(now.swap_used()),
                storage::shown(now.swap_total()),
                if now.swaps.iter().any(|swap| swap.zram) {
                    " (zram)"
                } else {
                    ""
                }
            )
        },
    ));
    let pressure: Vec<String> = [
        ("processor", now.pressure.cpu),
        ("memory", now.pressure.memory),
        ("disk", now.pressure.io),
    ]
    .into_iter()
    .filter_map(|(what, waited)| Some(format!("{what} {:.0}%", waited?)))
    .collect();
    if !pressure.is_empty() {
        facts.push(fact("Waiting, last minute", pressure.join(", ")));
    }
    let processor: Vec<String> = [
        now.governor
            .as_ref()
            .map(|governor| format!("governor {governor}")),
        now.power_profile
            .as_ref()
            .map(|profile| format!("power profile {profile}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !processor.is_empty() {
        facts.push(fact("Processor", processor.join(", ")));
    }
    if !now.heaviest.is_empty() {
        facts.push(fact(
            "Most memory",
            now.heaviest
                .iter()
                .take(5)
                .map(|process| {
                    format!(
                        "{} {}",
                        printable(&process.name),
                        storage::shown(process.memory)
                    )
                })
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    facts
}

// Offline bundles

/// An entry of a bundle, on one line.
pub fn bundled_entry(entry: &BundledEntry) -> String {
    match (&entry.network, &entry.flatpak) {
        (None, Some(id)) => format!("Offline: {} (the Flatpak {id})", printable(&entry.name)),
        (None, None) => format!("Offline: {}", printable(&entry.name)),
        (Some(why), _) => format!(
            "Needs the network: {} ({})",
            printable(&entry.name),
            printable(why)
        ),
    }
}

/// A Flatpak a bundle copies: "app/org.gimp.GIMP/x86_64/stable at commit
/// 0123456789ab, from flathub".
pub fn bundled_flatpak(flatpak: &BundledFlatpak) -> String {
    format!(
        "{} at commit {}, from {}",
        printable(&flatpak.reference),
        printable(flatpak.commit.get(..12).unwrap_or(&flatpak.commit)),
        printable(&flatpak.remote)
    )
}

/// How a bundle of these contents is downloaded, as `bundle create` says
/// it.
pub fn downloads(found: &Contents, complete: bool) -> Vec<String> {
    let mut lines = Vec::new();
    let files = found
        .downloads
        .iter()
        .filter(|download| !download.file.ends_with(".sig"))
        .count();
    if !found.dnf.is_empty() {
        lines.push(format!(
            "dnf downloads {} with their dependencies.",
            found.dnf.join(", ")
        ));
    } else if !found.zypper.is_empty() {
        lines.push(format!(
            "zypper downloads {} with {}, as you, without root.",
            found.zypper.join(", "),
            if complete {
                "every dependency"
            } else {
                "the dependencies this machine lacks"
            }
        ));
    } else if files > 0 || found.flatpaks.is_empty() {
        lines.push(format!(
            "{files} package {} to download{}.",
            if files == 1 { "file" } else { "files" },
            if complete {
                ", every dependency"
            } else {
                ", with the dependencies this machine lacks"
            }
        ));
    }
    if !found.flatpaks.is_empty() {
        lines.push(format!(
            "flatpak create-usb copies {}, with the runtimes they need, from this machine's installation.",
            found
                .flatpaks
                .iter()
                .map(|flatpak| printable(&flatpak.reference))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    lines
}

/// What a bundle of these entries would hold and how it is downloaded,
/// line by line, as `bundle create` says it and the menu shows it.
pub fn bundle_contents(found: &Contents, complete: bool) -> Vec<String> {
    let mut lines: Vec<String> = found.entries.iter().map(bundled_entry).collect();
    lines.extend(downloads(found, complete));
    lines
}

/// What a checked bundle holds, line by line, as `bundle verify` says it.
pub fn manifest(directory: &Path, manifest: &Manifest) -> Vec<String> {
    let mut lines = vec![format!(
        "{}: made for {} {} with Helix Toolbox {}{}; {} files, every one matching its checksum.",
        directory.display(),
        printable(&manifest.distribution),
        printable(&manifest.architecture),
        printable(&manifest.helixforge),
        if manifest.release {
            " (a release build)"
        } else {
            ""
        },
        manifest.files.len()
    )];
    for flatpak in &manifest.flatpaks {
        lines.push(format!("  Flatpak: {}", bundled_flatpak(flatpak)));
    }
    for entry in &manifest.entries {
        lines.push(match &entry.network {
            None => format!("  Offline: {}", printable(&entry.name)),
            Some(why) => format!(
                "  Needs the network: {} ({})",
                printable(&entry.name),
                printable(why)
            ),
        });
    }
    lines
}

/// What a bundle's files take: "48.2 MB".
pub fn bundle_size(manifest: &Manifest) -> String {
    storage::shown(manifest.files.iter().map(|file| file.bytes).sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bundle::Download,
        hardware::{Battery, Disk, Firmware, FirmwareDevice, FirmwareUpdate, Sensor},
        performance::{Phases, Swap},
    };

    fn sensor(celsius: f64, high: Option<f64>, critical: Option<f64>) -> Sensor {
        Sensor {
            chip: "coretemp".into(),
            label: "Package".into(),
            celsius,
            high,
            critical,
        }
    }

    #[test]
    fn a_sensor_is_hot_at_its_high_mark_or_within_five_degrees_of_critical() {
        let shown = |celsius, high, critical| {
            hardware(&Hardware {
                sensors: vec![sensor(celsius, high, critical)],
                ..Hardware::default()
            })
        };
        // Six degrees under critical is not yet hot; five is.
        assert!(!shown(94.0, None, Some(100.0)).sensors[0].attention);
        assert!(shown(95.0, None, Some(100.0)).sensors[0].attention);
        assert!(shown(80.0, Some(80.0), Some(100.0)).sensors[0].attention);
        assert!(!shown(79.0, Some(80.0), None).sensors[0].attention);
        let row = &shown(52.4, Some(100.0), Some(100.0)).sensors[0];
        assert_eq!(row.name, "coretemp Package");
        assert_eq!(row.label, "52 °C");
        assert_eq!(row.detail, "critical 100");
        let two = hardware(&Hardware {
            sensors: vec![
                sensor(52.0, None, Some(100.0)),
                sensor(97.0, None, Some(100.0)),
            ],
            ..Hardware::default()
        });
        assert_eq!(
            two.temperatures.label,
            "2 sensors; the warmest coretemp Package at 97 °C; 1 near their limit"
        );
        assert!(two.temperatures.attention);
        assert_eq!(
            hardware(&Hardware::default()).temperatures.label,
            "No temperature sensors found"
        );
    }

    #[test]
    fn disks_are_sized_in_decimal_units() {
        let found = hardware(&Hardware {
            disks: vec![
                Disk {
                    name: "nvme0n1".into(),
                    model: Some("Samsung SSD 990 PRO 1TB".into()),
                    bytes: Some(1_000_204_886_016),
                    kind: "NVMe".into(),
                    firmware: Some("8B2QJXD7".into()),
                    celsius: Some(38.0),
                },
                Disk {
                    name: "sda".into(),
                    bytes: Some(500_107_862_016),
                    kind: "SSD".into(),
                    ..Disk::default()
                },
            ],
            ..Hardware::default()
        });
        let nvme = &found.disks[0];
        assert_eq!(nvme.label, "Samsung SSD 990 PRO 1TB");
        assert_eq!(nvme.detail, "NVMe · 1.0 TB · firmware 8B2QJXD7 · 38 °C");
        assert_eq!(
            nvme.line,
            "nvme0n1  NVMe  1.0 TB  Samsung SSD 990 PRO 1TB  38 °C"
        );
        let sata = &found.disks[1];
        assert_eq!(sata.label, "unknown model");
        assert_eq!(sata.detail, "SSD · 500 GB");
    }

    #[test]
    fn fwupd_and_its_updates_read_the_same_in_both_interfaces() {
        let found = hardware(&Hardware {
            firmware: Firmware {
                available: true,
                devices: vec![
                    FirmwareDevice {
                        name: "UEFI dbx".into(),
                        version: Some("371".into()),
                        updatable: true,
                        ..FirmwareDevice::default()
                    },
                    FirmwareDevice {
                        name: "Webcam".into(),
                        ..FirmwareDevice::default()
                    },
                ],
                updates: vec![FirmwareUpdate {
                    device: "Bolt Receiver".into(),
                    from: None,
                    to: "MPR05.06".into(),
                    urgency: Some("medium".into()),
                    ..FirmwareUpdate::default()
                }],
                note: None,
            },
            ..Hardware::default()
        });
        let firmware = found.firmware.unwrap();
        assert_eq!(firmware.label, "2 devices, 1 updatable through fwupd");
        assert!(firmware.more.contains("UEFI dbx 371"));
        let update = &found.updates[0];
        assert_eq!(update.label, "Bolt Receiver");
        assert_eq!(update.detail, "? → MPR05.06 · medium");
        assert_eq!(update.line, "Update for Bolt Receiver: ? → MPR05.06");
        assert!(update.attention);
        let quiet = hardware(&Hardware {
            firmware: Firmware {
                available: true,
                ..Firmware::default()
            },
            ..Hardware::default()
        });
        assert_eq!(
            quiet.firmware.unwrap().label,
            "0 devices, 0 updatable through fwupd; no updates in the metadata fwupd has"
        );
    }

    #[test]
    fn a_battery_under_seventy_percent_of_its_design_wants_attention() {
        let shown = |health| {
            hardware(&Hardware {
                batteries: vec![Battery {
                    name: "BAT0".into(),
                    status: Some("Discharging".into()),
                    capacity: Some(80),
                    health,
                    cycles: Some(662),
                    technology: Some("Li-ion".into()),
                    ..Battery::default()
                }],
                ..Hardware::default()
            })
            .batteries
            .remove(0)
        };
        let worn = shown(Some(69));
        assert!(worn.attention);
        assert_eq!(worn.label, "80% charged, discharging");
        assert_eq!(
            worn.detail,
            "holds 69% of its design capacity · 662 cycles · Li-ion"
        );
        assert_eq!(
            worn.line,
            "BAT0: 80% charged, holds 69% of its design capacity, 662 cycles"
        );
        assert!(!shown(Some(70)).attention);
        let unknown = shown(None);
        assert!(!unknown.attention);
        assert_eq!(unknown.detail, "662 cycles · Li-ion");
        let charge = hardware(&Hardware {
            batteries: vec![Battery::default()],
            ..Hardware::default()
        });
        assert_eq!(charge.batteries[0].label, "charge unknown");
    }

    #[test]
    fn a_unit_and_an_autostart_entry_in_words() {
        let shown = unit(&Unit {
            name: "pipewire.service".into(),
            description: "PipeWire".into(),
            enablement: Some("indirect".into()),
            active: Some("active".into()),
            sub: Some("running".into()),
            user: true,
        });
        assert_eq!(shown.detail, "indirect · PipeWire");
        assert_eq!(shown.now, "active (running)");
        assert_eq!(shown.note, None);
        let loose = unit(&Unit {
            name: "dbus-x.service".into(),
            ..Unit::default()
        });
        assert_eq!(loose.state, "not loaded");
        assert_eq!(loose.detail, "no unit file");
        assert_eq!(loose.enablement, "no unit file of its own");
        assert!(loose.note.unwrap().starts_with("It is not enabled"));
        let overridden = autostart(&Autostart {
            exec: String::new(),
            enabled: false,
            here: true,
            system: true,
            own: Some("/home/me/.config/autostart/x.desktop".into()),
            ..Autostart::default()
        });
        assert_eq!(overridden.state, "off");
        assert_eq!(overridden.runs, "nothing named");
        assert_eq!(overridden.from, "the system · your file overrides it");
        assert_eq!(overridden.at_login, "not started");
        // A file of the user's own overrides nothing.
        let own = autostart(&Autostart {
            here: false,
            own: Some("/home/me/.config/autostart/y.desktop".into()),
            ..Autostart::default()
        });
        assert_eq!((own.state, own.from), ("not here", "your own"));
    }

    #[test]
    fn storage_is_decimal_and_a_disk_over_nine_tenths_full_is_marked() {
        let space = space(&Filesystem {
            path: "/".into(),
            free: 40_000_000_000,
            total: 500_000_000_000,
        });
        assert_eq!(space.text, "40.0 GB free of 500 GB");
        assert_eq!(
            space.meter,
            Meter {
                percent: 92,
                full: true
            }
        );
        assert!(!Meter::of(90, 100).full);
        assert!(Meter::of(91, 100).full);
        assert_eq!(Meter::of(0, 0).percent, 0);
        let offered = clean(&Item {
            size: Some(2_000_000_000),
            frees: Some(1_500_000_000),
            ..Item::default()
        });
        assert_eq!(
            (
                offered.size.as_str(),
                offered.said.as_str(),
                offered.offered
            ),
            ("2.0 GB", "frees 1.5 GB", true)
        );
        let unknown = clean(&Item::default());
        assert_eq!(
            (unknown.size.as_str(), unknown.said.as_str()),
            ("?", "frees what it finds")
        );
        let not = clean(&Item {
            not_offered: Some("the trash is empty".into()),
            ..Item::default()
        });
        assert_eq!(
            (not.said.as_str(), not.offered),
            ("the trash is empty", false)
        );
    }

    fn recorded(id: &str, booted: u64, userspace: u64) -> Boot {
        Boot {
            id: id.into(),
            booted,
            kernel: format!("7.2.{booted}"),
            phases: Phases {
                kernel: 1000,
                userspace,
                total: 1000 + userspace,
                ..Phases::default()
            },
            ..Boot::default()
        }
    }

    #[test]
    fn a_change_under_a_tenth_of_a_second_is_left_out() {
        let change = |before, after| {
            self::change(&Change {
                name: "unit".into(),
                before,
                after,
            })
        };
        assert_eq!(change(Some(1000), Some(1099)), None);
        let slower = change(Some(1000), Some(1100)).unwrap();
        assert_eq!(
            (slower.now.as_str(), slower.how.as_str(), slower.slower),
            ("1.1 s", "+100 ms", true)
        );
        let faster = change(Some(2000), Some(850)).unwrap();
        assert_eq!((faster.how.as_str(), faster.slower), ("-1.1 s", false));
        let new = change(None, Some(40)).unwrap();
        assert_eq!((new.how.as_str(), new.slower), ("new", true));
        let gone = change(Some(40), None).unwrap();
        assert_eq!(
            (gone.now.as_str(), gone.how.as_str(), gone.slower),
            ("not now", "gone", false)
        );
        let compared = comparison(&performance::compare(
            &recorded("before", 2000, 14_000),
            &recorded("after", 3000, 19_000),
            &[],
        ));
        assert_eq!((compared.total.as_str(), compared.sign), ("+5.0 s", 1));
        assert_eq!(
            compared.kernel,
            Some(("7.2.2000".to_string(), "7.2.3000".to_string()))
        );
        assert_eq!(compared.changes[0].name, "Services");
        let parts = super::boot(&recorded("now", 3000, 19_000)).parts;
        assert_eq!(
            parts
                .iter()
                .map(|part| format!("{} {}", part.name, part.shown))
                .collect::<Vec<_>>(),
            ["Kernel 1.0 s", "Services 19.0 s"]
        );
    }

    #[test]
    fn the_machine_now_in_facts() {
        let facts = now(&Now {
            cpus: 16,
            load: Some([0.52, 0.61, 0.7]),
            memory_total: 16_000_000_000,
            memory_available: 1_000_000_000,
            swaps: vec![Swap {
                name: "/dev/zram0".into(),
                zram: true,
                size: 8_000_000_000,
                used: 120_000_000,
            }],
            power_profile: Some("balanced".into()),
            ..Now::default()
        });
        let text = |term| {
            facts
                .iter()
                .find(|fact| fact.term == term)
                .map(|fact| fact.text.as_str())
        };
        assert_eq!(text("Load"), Some("0.52 0.61 0.70"));
        assert_eq!(facts[0].note, "(1, 5 and 15 minutes) on 16 processors");
        assert_eq!(text("Memory"), Some("1.0 GB free of 16.0 GB"));
        assert!(facts[1].meter.unwrap().full);
        assert_eq!(text("Swap"), Some("120 MB used of 8.0 GB (zram)"));
        assert_eq!(text("Processor"), Some("power profile balanced"));
        assert_eq!(text("Waiting, last minute"), None);
        assert_eq!(
            now(&Now::default())
                .iter()
                .map(|fact| (fact.term, fact.text.as_str()))
                .collect::<Vec<_>>(),
            [("Swap", "none")]
        );
    }

    #[test]
    fn a_bundle_is_described_as_the_command_line_says_it() {
        let download = |file: &str| Download {
            url: format!("https://example.com/{file}"),
            file: file.into(),
            checksum: None,
        };
        let found = Contents {
            entries: vec![
                BundledEntry {
                    name: "Ripgrep".into(),
                    offline: true,
                    ..BundledEntry::default()
                },
                BundledEntry {
                    name: "Marker".into(),
                    network: Some("a script, which downloads as it runs".into()),
                    ..BundledEntry::default()
                },
            ],
            downloads: vec![download("ripgrep.pkg"), download("ripgrep.pkg.sig")],
            ..Contents::default()
        };
        assert_eq!(
            downloads(&found, true),
            ["1 package file to download, every dependency."]
        );
        assert_eq!(
            bundle_contents(&found, false),
            [
                "Offline: Ripgrep",
                "Needs the network: Marker (a script, which downloads as it runs)",
                "1 package file to download, with the dependencies this machine lacks."
            ]
        );
        let flatpak = BundledFlatpak {
            reference: "app/org.gimp.GIMP/x86_64/stable".into(),
            commit: "0123456789abcdef".into(),
            remote: "flathub".into(),
            ..BundledFlatpak::default()
        };
        assert_eq!(
            bundled_flatpak(&flatpak),
            "app/org.gimp.GIMP/x86_64/stable at commit 0123456789ab, from flathub"
        );
        // A commit shorter than twelve characters is shown whole.
        let short = BundledFlatpak {
            commit: "abc".into(),
            ..flatpak
        };
        assert!(bundled_flatpak(&short).contains("at commit abc,"));
    }
}
