//! The performance and boot analyzer: how long this boot took and where the
//! time went, from systemd's own record of it; how the machine is doing now
//! (load, memory, pressure, swap); and this boot beside an earlier one, with
//! the Helix Toolbox runs in between named, so a change can be seen before
//! and after. It only reads. The one thing it writes is its own record of
//! boots, `boots.json` in the state folder, one line per boot it has seen.

use crate::{doctor::System, history::Record};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The boots kept in `boots.json`, newest first.
pub const KEPT: usize = 50;

/// How long each part of a boot took, in milliseconds, as
/// `systemd-analyze time` counts it. Firmware and loader times are known
/// only where the firmware or the boot loader tells systemd.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Phases {
    pub firmware: Option<u64>,
    pub loader: Option<u64>,
    pub kernel: u64,
    /// None without an initial RAM disk.
    pub initrd: Option<u64>,
    pub userspace: u64,
    pub total: u64,
}

impl Phases {
    /// Each part by name, the unknown ones left out.
    pub fn parts(&self) -> Vec<(&'static str, u64)> {
        [
            ("Firmware", self.firmware),
            ("Boot loader", self.loader),
            ("Kernel", Some(self.kernel)),
            ("Initial RAM disk", self.initrd),
            ("Services", Some(self.userspace)),
        ]
        .into_iter()
        .filter_map(|(name, took)| Some((name, took?)))
        .collect()
    }
}

/// A unit and how long it took to start, in milliseconds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Timed {
    pub name: String,
    pub took: u64,
}

/// A unit on the way to the login screen: when it was up, after the boot
/// reached the services, and how long it took itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub name: String,
    pub at: Option<u64>,
    pub took: Option<u64>,
}

/// One boot as systemd recorded it.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct Boot {
    /// The kernel's id for this boot.
    pub id: String,
    /// When it booted, in seconds since 1970.
    pub booted: u64,
    /// The kernel it booted, as `uname -r` says.
    #[serde(default)]
    pub kernel: String,
    pub phases: Phases,
    /// The services that took longest to start since the boot, slowest
    /// first.
    #[serde(default)]
    pub slowest: Vec<Timed>,
    /// What the login screen waited for, the login screen's target first.
    #[serde(default)]
    pub chain: Vec<Link>,
}

/// One swap space.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Swap {
    pub name: String,
    /// Compressed in memory (zram), not on a disk.
    pub zram: bool,
    pub size: u64,
    pub used: u64,
}

/// A running program and the memory it holds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Process {
    pub pid: u32,
    pub name: String,
    pub memory: u64,
}

/// How much of the last minute programs spent waiting, in percent, from
/// the kernel's pressure figures; None where the kernel has none.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Pressure {
    pub cpu: Option<f64>,
    pub memory: Option<f64>,
    pub io: Option<f64>,
}

/// How the machine is doing now.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Now {
    pub cpus: usize,
    /// The load over the last 1, 5 and 15 minutes.
    pub load: Option<[f64; 3]>,
    /// Memory in bytes.
    pub memory_total: u64,
    pub memory_available: u64,
    pub swaps: Vec<Swap>,
    pub pressure: Pressure,
    /// The processor's frequency governor, and power-profiles-daemon's
    /// profile where it runs.
    pub governor: Option<String>,
    pub power_profile: Option<String>,
    /// The programs holding the most memory, most first.
    pub heaviest: Vec<Process>,
}

impl Now {
    pub fn swap_total(&self) -> u64 {
        self.swaps.iter().map(|swap| swap.size).sum()
    }

    pub fn swap_used(&self) -> u64 {
        self.swaps.iter().map(|swap| swap.used).sum()
    }
}

/// Something worth knowing about the boot or the machine, and what would
/// change it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Hint {
    pub title: String,
    pub detail: String,
    /// A Helix Toolbox command that changes it, after its own confirmation.
    pub command: Option<String>,
    /// A unit the command changes, for the interfaces' buttons.
    pub unit: Option<String>,
    /// A catalog entry that addresses it.
    pub entry: Option<String>,
}

/// How one part or unit changed between two boots, in milliseconds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Change {
    pub name: String,
    pub before: Option<u64>,
    pub after: Option<u64>,
}

impl Change {
    /// After less before; None when either is unknown.
    pub fn difference(&self) -> Option<i64> {
        Some(self.after? as i64 - self.before? as i64)
    }
}

/// A recorded Helix Toolbox run.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Run {
    pub at: u64,
    pub names: Vec<String>,
    pub success: bool,
}

/// A later boot beside an earlier one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Comparison {
    pub before: Boot,
    pub after: Boot,
    /// The whole boot and each part.
    pub phases: Vec<Change>,
    /// The units that changed most, on the way to the login screen or among
    /// the slowest, most changed first.
    pub units: Vec<Change>,
    /// Helix Toolbox's runs between the two boots, oldest first.
    pub runs: Vec<Run>,
}

impl Comparison {
    pub fn total(&self) -> i64 {
        self.after.phases.total as i64 - self.before.phases.total as i64
    }
}

/// Everything the analyzer reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Performance {
    pub boot: Option<Boot>,
    /// Why there is no boot record.
    pub no_boot: Option<String>,
    pub now: Now,
    /// This boot beside the one before it that the record has.
    pub previous: Option<Comparison>,
    /// How many boots the record has, this one included.
    pub recorded: usize,
    pub hints: Vec<Hint>,
}

/// `boots.json` in a state folder.
pub fn boots_path(state: &Path) -> PathBuf {
    state.join("boots.json")
}

/// Read the machine, record this boot in `state`'s boots, and set it beside
/// the boot before.
pub fn gather(system: &dyn System, state: Option<&Path>, history: &[Record]) -> Performance {
    let (boot, no_boot) = match boot(system) {
        Ok(boot) => (Some(boot), None),
        Err(why) => (None, Some(why)),
    };
    let now = now(system);
    let boots = match (&boot, state) {
        (Some(boot), Some(state)) => record(&boots_path(state), boot),
        (_, Some(state)) => load(&boots_path(state)),
        _ => boot.iter().cloned().collect(),
    };
    let previous = boot.as_ref().and_then(|boot| {
        boots
            .iter()
            .find(|other| other.id != boot.id && other.booted <= boot.booted)
            .map(|before| compare(before, boot, history))
    });
    let hints = hints(boot.as_ref(), &now);
    Performance {
        boot,
        no_boot,
        now,
        previous,
        recorded: boots.len(),
        hints,
    }
}

/// The boots recorded in `path`, newest first.
pub fn load(path: &Path) -> Vec<Boot> {
    crate::watch::load(path)
}

/// Add a boot to the record in `path` unless it is there already, and give
/// the record back. A boot is recorded once, when it is first seen, nearest
/// its start.
pub fn record(path: &Path, boot: &Boot) -> Vec<Boot> {
    let mut boots = load(path);
    if !boots.iter().any(|other| other.id == boot.id) {
        boots.push(boot.clone());
        boots.sort_by_key(|other| std::cmp::Reverse(other.booted));
        boots.truncate(KEPT);
        let _ = crate::watch::store(path, &boots);
    }
    boots
}

/// A recorded boot by its place in the record (1 is the newest) or the
/// start of its id.
pub fn find<'a>(boots: &'a [Boot], wanted: &str) -> Option<&'a Boot> {
    let wanted = wanted.trim();
    if let Ok(place) = wanted.parse::<usize>() {
        return place.checked_sub(1).and_then(|index| boots.get(index));
    }
    let wanted = wanted.replace('-', "").to_lowercase();
    if wanted.len() < 4 {
        return None;
    }
    let mut found = boots
        .iter()
        .filter(|boot| boot.id.replace('-', "").starts_with(&wanted));
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

/// This boot as systemd recorded it.
pub fn boot(system: &dyn System) -> Result<Boot, String> {
    let Some((true, text)) = system.run(&[
        "systemctl",
        "show",
        "--property=FirmwareTimestampMonotonic,LoaderTimestampMonotonic,InitRDTimestampMonotonic,UserspaceTimestampMonotonic,FinishTimestampMonotonic",
    ]) else {
        return Err("systemd does not run this machine, so there is no record of the boot".into());
    };
    let value = |name: &str| -> u64 {
        text.lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .and_then(|value| value.trim().parse().ok())
            .unwrap_or(0)
    };
    let phases = phases(
        value("FirmwareTimestampMonotonic"),
        value("LoaderTimestampMonotonic"),
        value("InitRDTimestampMonotonic"),
        value("UserspaceTimestampMonotonic"),
        value("FinishTimestampMonotonic"),
    )
    .ok_or_else(|| {
        "The boot has not finished: systemd is still starting units, so it has no time yet"
            .to_string()
    })?;
    let read = |path: &str| {
        system
            .read(Path::new(path))
            .map(|text| text.trim().to_string())
            .unwrap_or_default()
    };
    let booted = read("/proc/stat")
        .lines()
        .find_map(|line| line.strip_prefix("btime ")?.trim().parse().ok())
        .unwrap_or(0);
    let slowest = system
        .run(&["systemd-analyze", "blame", "--no-pager"])
        .filter(|(success, _)| *success)
        .map(|(_, text)| parse_blame(&text))
        .unwrap_or_default();
    let chain = system
        .run(&["systemd-analyze", "critical-chain", "--no-pager"])
        .filter(|(success, _)| *success)
        .map(|(_, text)| parse_chain(&text))
        .unwrap_or_default();
    Ok(Boot {
        id: read("/proc/sys/kernel/random/boot_id"),
        booted,
        kernel: read("/proc/sys/kernel/osrelease"),
        phases,
        slowest,
        chain,
    })
}

/// The parts of a boot from systemd's monotonic times, in microseconds:
/// the firmware's and the loader's count back from the kernel's start, the
/// others forward. None until the boot has finished.
pub fn phases(
    firmware: u64,
    loader: u64,
    initrd: u64,
    userspace: u64,
    finish: u64,
) -> Option<Phases> {
    if finish == 0 || userspace == 0 {
        return None;
    }
    let ms = |microseconds: u64| microseconds / 1000;
    let (kernel, initrd) = if initrd > 0 {
        (initrd, Some(userspace.saturating_sub(initrd)))
    } else {
        (userspace, None)
    };
    Some(Phases {
        firmware: (firmware > 0).then(|| ms(firmware.saturating_sub(loader))),
        loader: (loader > 0).then(|| ms(loader)),
        kernel: ms(kernel),
        initrd: initrd.map(ms),
        userspace: ms(finish.saturating_sub(userspace)),
        total: ms(firmware.max(loader) + finish),
    })
}

/// A span as systemd writes it (`1min 25.603s`, `345ms`, `2h 1min`), in
/// milliseconds.
pub fn parse_span(text: &str) -> Option<u64> {
    let mut total = 0f64;
    let mut any = false;
    for word in text.split_whitespace() {
        let split = word
            .find(|character: char| !character.is_ascii_digit() && character != '.')
            .unwrap_or(word.len());
        let (number, unit) = word.split_at(split);
        let number: f64 = number.parse().ok()?;
        let scale = match unit {
            "d" => 86_400_000.0,
            "h" => 3_600_000.0,
            "min" => 60_000.0,
            "s" | "" => 1000.0,
            "ms" => 1.0,
            "us" | "µs" => 0.001,
            _ => return None,
        };
        total += number * scale;
        any = true;
    }
    any.then(|| total.round() as u64)
}

const UNIT_KINDS: [&str; 11] = [
    ".service",
    ".target",
    ".socket",
    ".mount",
    ".device",
    ".timer",
    ".path",
    ".slice",
    ".scope",
    ".swap",
    ".automount",
];

fn unit_name(word: &str) -> bool {
    UNIT_KINDS.iter().any(|kind| word.ends_with(kind)) && !word.starts_with('-')
}

/// The ten services that took longest, from `systemd-analyze blame`.
pub fn parse_blame(text: &str) -> Vec<Timed> {
    let mut found: Vec<Timed> = text
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let (span, name) = line.rsplit_once(char::is_whitespace)?;
            if !name.ends_with(".service") {
                return None;
            }
            Some(Timed {
                name: name.to_string(),
                took: parse_span(span)?,
            })
        })
        .collect();
    found.sort_by_key(|timed| std::cmp::Reverse(timed.took));
    found.truncate(10);
    found
}

/// The units the login screen waited for, from `systemd-analyze
/// critical-chain`.
pub fn parse_chain(text: &str) -> Vec<Link> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches(|character: char| {
                character.is_whitespace() || "└─│├|`-".contains(character)
            });
            let (name, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
            if !unit_name(name) {
                return None;
            }
            let (at, took) = match rest.split_once('+') {
                Some((at, took)) => (at, Some(took)),
                None => (rest, None),
            };
            Some(Link {
                name: name.to_string(),
                at: at.trim().strip_prefix('@').and_then(parse_span),
                took: took.and_then(parse_span),
            })
        })
        .collect()
}

/// How the machine is doing now.
pub fn now(system: &dyn System) -> Now {
    let read = |path: &str| system.read(Path::new(path)).unwrap_or_default();
    let meminfo = read("/proc/meminfo");
    let kilobytes = |name: &str| -> u64 {
        meminfo
            .lines()
            .find_map(|line| {
                let rest = line.strip_prefix(name)?.strip_prefix(':')?;
                rest.split_whitespace().next()?.parse::<u64>().ok()
            })
            .unwrap_or(0)
            * 1024
    };
    let load = {
        let numbers: Vec<f64> = read("/proc/loadavg")
            .split_whitespace()
            .take(3)
            .filter_map(|number| number.parse().ok())
            .collect();
        (numbers.len() == 3).then(|| [numbers[0], numbers[1], numbers[2]])
    };
    let swaps = read("/proc/swaps")
        .lines()
        .skip(1)
        .filter_map(|line| {
            let words: Vec<&str> = line.split_whitespace().collect();
            let [name, _, size, used, ..] = words.as_slice() else {
                return None;
            };
            Some(Swap {
                name: name.to_string(),
                zram: name.contains("/zram"),
                size: size.parse::<u64>().ok()? * 1024,
                used: used.parse::<u64>().ok()? * 1024,
            })
        })
        .collect();
    let pressure = |kind: &str| -> Option<f64> {
        read(&format!("/proc/pressure/{kind}"))
            .lines()
            .find(|line| line.starts_with("some "))?
            .split_whitespace()
            .find_map(|word| word.strip_prefix("avg60=")?.parse().ok())
    };
    let governor = Some(read(
        "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor",
    ))
    .map(|text| text.trim().to_string())
    .filter(|text| !text.is_empty());
    let power_profile = system
        .on_path("powerprofilesctl")
        .then(|| system.run(&["powerprofilesctl", "get"]))
        .flatten()
        .filter(|(success, _)| *success)
        .map(|(_, text)| text.trim().to_string())
        .filter(|text| !text.is_empty());
    let heaviest = system
        .run(&["ps", "-eo", "pid=,rss=,comm=", "--sort=-rss"])
        .filter(|(success, _)| *success)
        .map(|(_, text)| {
            text.lines()
                .filter_map(|line| {
                    let mut words = line.split_whitespace();
                    let pid = words.next()?.parse().ok()?;
                    let memory = words.next()?.parse::<u64>().ok()? * 1024;
                    let name = words.collect::<Vec<_>>().join(" ");
                    (!name.is_empty()).then_some(Process { pid, name, memory })
                })
                .take(8)
                .collect()
        })
        .unwrap_or_default();
    Now {
        cpus: read("/proc/cpuinfo")
            .lines()
            .filter(|line| line.starts_with("processor"))
            .count(),
        load,
        memory_total: kilobytes("MemTotal"),
        memory_available: kilobytes("MemAvailable"),
        swaps,
        pressure: Pressure {
            cpu: pressure("cpu"),
            memory: pressure("memory"),
            io: pressure("io"),
        },
        governor,
        power_profile,
        heaviest,
    }
}

/// A span in milliseconds for people: `850 ms`, `2.3 s`, `1 min 25 s`.
pub fn shown(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1000.0)
    } else {
        let seconds = (ms + 500) / 1000;
        format!("{} min {} s", seconds / 60, seconds % 60)
    }
}

/// A change in milliseconds with its sign: `+1.2 s`, `-850 ms`.
pub fn shown_change(difference: i64) -> String {
    let sign = if difference < 0 { "-" } else { "+" };
    format!("{sign}{}", shown(difference.unsigned_abs()))
}

/// What waiting for a unit at boot is, for the units often seen there.
fn known(unit: &str) -> Option<(&'static str, bool)> {
    Some(match unit {
        "NetworkManager-wait-online.service" | "systemd-networkd-wait-online.service" => (
            "It holds back what asks for a working network until one is up. A desktop rarely needs that before the login screen; turned off, the network still comes up, only without the boot waiting for it.",
            true,
        ),
        "systemd-udev-settle.service" => (
            "It waits for every device to settle and is deprecated; something still asks for it (helix-toolbox services show systemd-udev-settle.service says what).",
            false,
        ),
        "docker.service" | "containerd.service" | "podman.service" => (
            "It starts the container engine at boot. Its socket starts it the first time it is used instead, once the service is disabled and docker.socket enabled.",
            false,
        ),
        "lvm2-monitor.service" => (
            "It watches LVM volumes; on a machine without LVM snapshots or mirrors it has nothing to do.",
            false,
        ),
        _ => return None,
    })
}

/// What is worth knowing about the boot and the machine now.
pub fn hints(boot: Option<&Boot>, now: &Now) -> Vec<Hint> {
    let mut found = Vec::new();
    let hint = |title: String, detail: &str| Hint {
        title,
        detail: detail.to_string(),
        command: None,
        unit: None,
        entry: None,
    };
    if let Some(boot) = boot {
        if let Some(firmware) = boot.phases.firmware.filter(|&took| took >= 10_000) {
            found.push(hint(
                format!("The firmware took {} before the boot loader", shown(firmware)),
                "That is before Linux starts at all. A fast boot option in the firmware's setup, or fewer devices for it to try booting from, shortens it.",
            ));
        }
        if let Some(loader) = boot.phases.loader.filter(|&took| took >= 5000) {
            found.push(hint(
                format!("The boot loader took {}", shown(loader)),
                "Most of a boot loader's time is usually its menu's countdown (GRUB_TIMEOUT in /etc/default/grub, timeout in systemd-boot's loader.conf, TIMEOUT in Limine's); a shorter one starts sooner.",
            ));
        }
        for link in &boot.chain {
            let Some(took) = link.took.filter(|&took| took >= 2000) else {
                continue;
            };
            if !link.name.ends_with(".service") || link.name.starts_with("plymouth") {
                // Plymouth's splash waits for the login screen; it is not
                // what holds it up.
                continue;
            }
            let (detail, disable) = known(&link.name).unwrap_or((
                "It is on the way to the login screen, so the boot waits for it. helix-toolbox services show says what needs it.",
                false,
            ));
            let mut found_hint = hint(
                format!("The boot waited {} for {}", shown(took), link.name),
                detail,
            );
            found_hint.unit = Some(link.name.clone());
            found_hint.command = Some(if disable {
                format!("helix-toolbox services disable {}", link.name)
            } else {
                format!("helix-toolbox services show {}", link.name)
            });
            found.push(found_hint);
        }
    }
    if now.memory_total > 0 && now.memory_available * 10 < now.memory_total {
        let mut detail = "Less than a tenth of the memory is free, so programs wait while the kernel makes room.".to_string();
        if let Some(heaviest) = now.heaviest.first() {
            detail.push_str(&format!(
                " {} holds the most, {}.",
                heaviest.name,
                crate::storage::shown(heaviest.memory)
            ));
        }
        found.push(hint("Memory is nearly full".into(), &detail));
    }
    let swapped = now.swap_used();
    if swapped > 0 && swapped * 4 > now.memory_total && !now.swaps.iter().any(|swap| swap.zram) {
        let mut swapping = hint(
            format!("{} is swapped to disk", crate::storage::shown(swapped)),
            "Programs wait for the disk when they need it back. Compressed swap in memory (zram) is much faster than a swap file or partition.",
        );
        swapping.entry = Some("zram-swap".into());
        found.push(swapping);
    }
    for (waiting, over, what, detail) in [
        (
            now.pressure.memory,
            10.0,
            "memory",
            "The kernel's memory pressure: some program was held up while memory was found for it. The programs holding the most are listed under Now.",
        ),
        (
            now.pressure.io,
            20.0,
            "the disk",
            "The kernel's disk pressure: some program was held up reading or writing. A build, an update, an indexer or swapping is the usual reason; it passes when they finish.",
        ),
        (
            now.pressure.cpu,
            50.0,
            "the processor",
            "The kernel's processor pressure: some program was ready to run but had to wait its turn.",
        ),
    ] {
        if let Some(waiting) = waiting.filter(|&waiting| waiting >= over) {
            found.push(hint(
                format!("Programs waited for {what} {waiting:.0}% of the last minute"),
                detail,
            ));
        }
    }
    found
}

/// A later boot beside an earlier one, with the runs in between.
pub fn compare(before: &Boot, after: &Boot, history: &[Record]) -> Comparison {
    let mut phases = vec![Change {
        name: "Whole boot".into(),
        before: Some(before.phases.total),
        after: Some(after.phases.total),
    }];
    for (name, took) in after.phases.parts() {
        let earlier = before
            .phases
            .parts()
            .into_iter()
            .find(|(other, _)| *other == name)
            .map(|(_, took)| took);
        phases.push(Change {
            name: name.into(),
            before: earlier,
            after: Some(took),
        });
    }
    let times = |boot: &Boot| -> Vec<(String, u64)> {
        let mut times: Vec<(String, u64)> = boot
            .chain
            .iter()
            .filter_map(|link| Some((link.name.clone(), link.took?)))
            .chain(
                boot.slowest
                    .iter()
                    .map(|timed| (timed.name.clone(), timed.took)),
            )
            .collect();
        times.sort();
        times.dedup_by(|a, b| a.0 == b.0);
        times
    };
    let (earlier, later) = (times(before), times(after));
    let mut names: Vec<&String> = earlier.iter().chain(&later).map(|(name, _)| name).collect();
    names.sort();
    names.dedup();
    let lookup = |list: &[(String, u64)], name: &str| {
        list.iter()
            .find(|(other, _)| other == name)
            .map(|(_, took)| *took)
    };
    let mut units: Vec<Change> = names
        .into_iter()
        .map(|name| Change {
            name: name.clone(),
            before: lookup(&earlier, name),
            after: lookup(&later, name),
        })
        .filter(|change| {
            let moved = match (change.before, change.after) {
                (Some(before), Some(after)) => before.abs_diff(after),
                (None, Some(took)) | (Some(took), None) => took,
                (None, None) => 0,
            };
            moved >= 500
        })
        .collect();
    units.sort_by_key(|change| {
        std::cmp::Reverse(
            change
                .difference()
                .map(i64::unsigned_abs)
                .or(change.after)
                .or(change.before)
                .unwrap_or(0),
        )
    });
    units.truncate(10);
    let mut runs: Vec<Run> = history
        .iter()
        .filter(|record| {
            record.unix_timestamp >= before.booted && record.unix_timestamp < after.booted
        })
        .map(|record| Run {
            at: record.unix_timestamp,
            names: record.names.clone(),
            success: record.success,
        })
        .collect();
    runs.sort_by_key(|run| run.at);
    Comparison {
        before: before.clone(),
        after: after.clone(),
        phases,
        units,
        runs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        files: HashMap<&'static str, String>,
        commands: HashMap<String, String>,
        tools: Vec<&'static str>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path.to_str()?).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            path.to_str()
                .is_some_and(|path| self.files.contains_key(path))
        }
        fn list(&self, _: &Path) -> Vec<String> {
            Vec::new()
        }
        fn link(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn run(&self, command: &[&str]) -> Option<(bool, String)> {
            self.commands
                .get(&command.join(" "))
                .map(|text| (true, text.clone()))
        }
        fn resolve(&self, _: &str) -> Result<Vec<String>, String> {
            Err("offline".into())
        }
        fn space(&self, _: &Path) -> Option<(u64, u64)> {
            None
        }
        fn owner(&self, _: &Path) -> Option<u32> {
            None
        }
        fn is_link(&self, _: &Path) -> bool {
            false
        }
        fn canonical(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn user(&self) -> (u32, u32, PathBuf) {
            (1000, 1000, PathBuf::from("/home/tester"))
        }
        fn on_path(&self, tool: &str) -> bool {
            self.tools.contains(&tool)
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const SHOW: &str = "systemctl show --property=FirmwareTimestampMonotonic,LoaderTimestampMonotonic,InitRDTimestampMonotonic,UserspaceTimestampMonotonic,FinishTimestampMonotonic";

    fn machine(finish: u64) -> Fake {
        let mut fake = Fake::default();
        fake.commands.insert(
            SHOW.into(),
            format!(
                "FirmwareTimestampMonotonic=22755973\nLoaderTimestampMonotonic=12297339\nInitRDTimestampMonotonic=1065390\nUserspaceTimestampMonotonic=3727009\nFinishTimestampMonotonic={finish}\n"
            ),
        );
        fake.commands.insert(
            "systemd-analyze blame --no-pager".into(),
            "1min 25.603s archlinux-keyring-wkd-sync.service\n      4.183s plocate-updatedb.service\n      2.953s dev-tpm0.device\n       37ms power-profiles-daemon.service\n".into(),
        );
        fake.commands.insert(
            "systemd-analyze critical-chain --no-pager".into(),
            "The time when unit became active or started is printed after the \"@\" character.\nThe time the unit took to start is printed after the \"+\" character.\n\ngraphical.target @5.394s\n└─sddm.service @5.356s +37ms\n  └─NetworkManager-wait-online.service @1.003s +4.344s\n    └─plymouth-quit.service @1min 3.010s +2.344s\n".into(),
        );
        fake.files.insert(
            "/proc/sys/kernel/random/boot_id",
            "6f1c0d0e-aaaa-bbbb-cccc-000000000001\n".into(),
        );
        fake.files
            .insert("/proc/sys/kernel/osrelease", "7.2.11-1-garuda\n".into());
        fake.files
            .insert("/proc/stat", "cpu  1 2 3\nbtime 1792000000\n".into());
        fake.files.insert(
            "/proc/meminfo",
            "MemTotal:       16000000 kB\nMemFree:          100000 kB\nMemAvailable:     800000 kB\n".into(),
        );
        fake.files
            .insert("/proc/loadavg", "0.52 0.61 0.70 1/900 4242\n".into());
        fake.files.insert(
            "/proc/cpuinfo",
            "processor\t: 0\nmodel name\t: x\n\nprocessor\t: 1\n".into(),
        );
        fake.files.insert(
            "/proc/swaps",
            "Filename\t\t\t\tType\t\tSize\t\tUsed\t\tPriority\n/swap/swapfile                          file\t\t8000000\t\t6000000\t\t-2\n".into(),
        );
        fake.files.insert(
            "/proc/pressure/memory",
            "some avg10=30.00 avg60=12.50 avg300=3.00 total=1\nfull avg10=0.00 avg60=0.00 avg300=0.00 total=0\n".into(),
        );
        fake.commands.insert(
            "ps -eo pid=,rss=,comm= --sort=-rss".into(),
            "  2001 2400000 firefox\n  1500  800000 Web Content\n".into(),
        );
        fake
    }

    #[test]
    fn a_boot_is_read_in_its_parts_with_what_held_it_up() {
        let boot = boot(&machine(9121346)).unwrap();
        assert_eq!(
            boot.phases,
            Phases {
                firmware: Some(10458),
                loader: Some(12297),
                kernel: 1065,
                initrd: Some(2661),
                userspace: 5394,
                total: 31877,
            }
        );
        assert_eq!(boot.kernel, "7.2.11-1-garuda");
        assert_eq!(boot.booted, 1_792_000_000);
        // Services only, slowest first; devices are left out.
        assert_eq!(
            boot.slowest
                .iter()
                .map(|timed| (timed.name.as_str(), timed.took))
                .collect::<Vec<_>>(),
            [
                ("archlinux-keyring-wkd-sync.service", 85603),
                ("plocate-updatedb.service", 4183),
                ("power-profiles-daemon.service", 37)
            ]
        );
        assert_eq!(boot.chain.len(), 4);
        assert_eq!(boot.chain[0].name, "graphical.target");
        assert_eq!(boot.chain[0].at, Some(5394));
        assert_eq!(boot.chain[0].took, None);
        assert_eq!(boot.chain[3].at, Some(63010));
        assert_eq!(boot.chain[3].took, Some(2344));

        assert!(
            super::boot(&machine(0))
                .unwrap_err()
                .contains("has not finished")
        );
        assert!(
            super::boot(&Fake::default())
                .unwrap_err()
                .contains("systemd does not run")
        );
    }

    #[test]
    fn spans_and_phases_read_as_systemd_writes_them() {
        assert_eq!(parse_span("1min 25.603s"), Some(85603));
        assert_eq!(parse_span("345ms"), Some(345));
        assert_eq!(parse_span("2h 1min"), Some(7_260_000));
        assert_eq!(parse_span("12us"), Some(0));
        assert_eq!(parse_span("soon"), None);
        assert_eq!(parse_span(""), None);
        // No initial RAM disk, no firmware or loader times.
        let plain = phases(0, 0, 0, 2_000_000, 7_000_000).unwrap();
        assert_eq!(
            (
                plain.firmware,
                plain.loader,
                plain.kernel,
                plain.initrd,
                plain.userspace,
                plain.total
            ),
            (None, None, 2000, None, 5000, 7000)
        );
        assert_eq!(shown(850), "850 ms");
        assert_eq!(shown(2340), "2.3 s");
        assert_eq!(shown(85603), "1 min 26 s");
        assert_eq!(shown_change(-850), "-850 ms");
        assert_eq!(shown_change(1200), "+1.2 s");
    }

    #[test]
    fn the_machine_now_and_what_is_worth_knowing() {
        let fake = machine(9121346);
        let now = now(&fake);
        assert_eq!(now.cpus, 2);
        assert_eq!(now.load, Some([0.52, 0.61, 0.70]));
        assert_eq!(now.memory_available, 800_000 * 1024);
        assert_eq!(now.swap_used(), 6_000_000 * 1024);
        assert_eq!(now.pressure.memory, Some(12.5));
        assert_eq!(now.pressure.cpu, None);
        assert_eq!(now.heaviest[1].name, "Web Content");
        assert_eq!(now.heaviest[0].memory, 2_400_000 * 1024);
        let boot = boot(&fake).unwrap();
        let found = hints(Some(&boot), &now);
        let titles: Vec<&str> = found.iter().map(|hint| hint.title.as_str()).collect();
        assert_eq!(
            titles,
            [
                "The firmware took 10.5 s before the boot loader",
                "The boot loader took 12.3 s",
                "The boot waited 4.3 s for NetworkManager-wait-online.service",
                "Memory is nearly full",
                "6.1 GB is swapped to disk",
                "Programs waited for memory 12% of the last minute",
            ]
        );
        // Plymouth's wait is not offered as a cause; waiting online is,
        // with the service change that turns it off.
        assert_eq!(
            found[2].command.as_deref(),
            Some("helix-toolbox services disable NetworkManager-wait-online.service")
        );
        assert!(found[3].detail.contains("firefox holds the most"));
        assert_eq!(found[4].entry.as_deref(), Some("zram-swap"));
    }

    #[test]
    fn boots_are_recorded_once_and_compared_with_the_runs_between() {
        let state = tempfile::tempdir().unwrap();
        let path = boots_path(state.path());
        let mut before = boot(&machine(9121346)).unwrap();
        before.id = "aaaa1111".into();
        before.booted = 1000;
        record(&path, &before);
        let mut after = before.clone();
        after.id = "bbbb2222".into();
        after.booted = 5000;
        after.kernel = "7.3.0-2-garuda".into();
        after.phases.userspace += 3000;
        after.phases.total += 3000;
        after.chain[2].took = Some(7344);
        after.slowest.push(Timed {
            name: "docker.service".into(),
            took: 1500,
        });
        let kept = record(&path, &after);
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].id, "bbbb2222");
        // Seen again, the boot is not recorded twice.
        assert_eq!(record(&path, &after).len(), 2);
        assert_eq!(find(&kept, "2").unwrap().id, "aaaa1111");
        assert_eq!(find(&kept, "bbbb").unwrap().id, "bbbb2222");
        assert!(find(&kept, "3").is_none());
        assert!(find(&kept, "zz").is_none());

        let run = |at: u64, name: &str| Record {
            unix_timestamp: at,
            entries: vec![name.into()],
            names: vec![name.into()],
            success: true,
            result: "exit status 0".into(),
            log: None,
        };
        let history = [
            run(500, "Too early"),
            run(2000, "Docker"),
            run(6000, "Too late"),
        ];
        let compared = compare(&before, &after, &history);
        assert_eq!(compared.total(), 3000);
        assert_eq!(compared.phases[0].difference(), Some(3000));
        assert_eq!(
            compared
                .units
                .iter()
                .map(|change| change.name.as_str())
                .collect::<Vec<_>>(),
            ["NetworkManager-wait-online.service", "docker.service"]
        );
        assert_eq!(compared.units[1].before, None);
        assert_eq!(
            compared
                .runs
                .iter()
                .map(|run| run.names[0].as_str())
                .collect::<Vec<_>>(),
            ["Docker"]
        );

        let read = gather(&machine(9121346), Some(state.path()), &history);
        assert_eq!(read.recorded, 3);
        assert_eq!(read.previous.unwrap().before.id, "bbbb2222");
    }
}
