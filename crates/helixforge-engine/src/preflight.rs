//! Checks made before a plan runs: what it needs that is missing, entries in
//! it that conflict with each other or with what is installed, repositories
//! it needs, the graphics hardware it is for, and whether the disk has room.
//! A blocker stops the run; a warning is shown with the confirmation.

use crate::{Machine, PackageState, Plan, Probe, Step, missing_requirements, package_state};
use helixforge_core::{Entry, PackageManager, valid_package_name};
use helixforge_platform::LinuxInfo;
use std::{path::Path, process::Command};

/// How serious a finding is.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Warning,
    Blocker,
}

/// One thing found before a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Finding {
    pub severity: Severity,
    pub message: String,
}

impl Finding {
    fn blocker(message: String) -> Self {
        Finding {
            severity: Severity::Blocker,
            message,
        }
    }

    fn warning(message: String) -> Self {
        Finding {
            severity: Severity::Warning,
            message,
        }
    }

    /// The finding as one line, with its severity.
    pub fn line(&self) -> String {
        match self.severity {
            Severity::Blocker => format!("Blocked: {}", self.message),
            Severity::Warning => format!("Warning: {}", self.message),
        }
    }
}

/// Whether any finding stops the run.
pub fn blocked(findings: &[Finding]) -> bool {
    findings
        .iter()
        .any(|finding| finding.severity == Severity::Blocker)
}

/// What the checks ask the machine; tests answer instead.
pub struct Facts<'a> {
    /// Package questions, as planning asks them.
    pub machine: Machine<'a>,
    /// Whether a command, or an absolute path, is present.
    pub present: &'a dyn Fn(&str) -> bool,
    /// Free bytes on the file system holding a path.
    pub free_bytes: &'a dyn Fn(&Path) -> Option<u64>,
    /// The graphics vendors found (nvidia, amd, intel).
    pub gpus: &'a dyn Fn() -> Vec<String>,
    /// The enabled pacman repositories; None off Arch or when unreadable.
    pub repositories: &'a dyn Fn() -> Option<Vec<String>>,
    /// The installed size in bytes of packages not yet installed, when the
    /// package manager can say.
    pub sizes: &'a dyn Fn(PackageManager, &[String]) -> Option<u64>,
    /// Other HelixForge runs still going, by their entries' names.
    pub active_runs: &'a dyn Fn() -> Vec<String>,
}

impl Facts<'static> {
    /// This machine.
    pub fn real() -> Self {
        Facts {
            machine: Machine::real(),
            present: &crate::requirement_present,
            free_bytes: &free_bytes,
            gpus: &gpus,
            repositories: &|| pacman_repositories(Path::new("/etc/pacman.conf")),
            sizes: &installed_size,
            active_runs: &crate::history::active,
        }
    }
}

/// Less free space than this left after a run earns a warning.
const SPARE: u64 = 1 << 30;

/// Check a plan before it runs. `lookup` finds the entry behind each step.
pub fn check<'a>(
    plan: &Plan,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    facts: &Facts,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    // One run at a time, so package managers are never driven twice at once.
    if !plan.steps.is_empty() {
        for names in (facts.active_runs)() {
            findings.push(Finding::blocker(format!(
                "another HelixForge run is still going ({names}); wait for it to finish"
            )));
        }
    }
    for missing in missing_requirements(plan, facts.present) {
        findings.push(Finding::blocker(format!(
            "{missing} is needed and is not installed"
        )));
    }
    let entries: Vec<&Entry> = plan
        .steps
        .iter()
        .filter(|step| !matches!(step, Step::Update { .. }))
        .filter(|step| !matches!(step, Step::Package { removal: true, .. }))
        .filter_map(|step| lookup(step.id()))
        .collect();
    conflicts(&entries, platform, facts, &mut findings);
    needs(&entries, platform, facts, &mut findings);
    space(plan, &entries, platform, facts, &mut findings);
    findings.sort_by_key(|finding| std::cmp::Reverse(finding.severity));
    findings
}

/// The packages an entry installs on this machine: its family's and, on
/// Arch, its AUR package.
fn installs(entry: &Entry, platform: &LinuxInfo) -> Vec<String> {
    let Some(identity) = entry.identity() else {
        return Vec::new();
    };
    let mut names = Vec::new();
    if let Some(manager) = platform.manager {
        names.extend(identity.packages.for_manager(manager).map(String::from));
        if manager == PackageManager::Arch {
            names.extend(identity.aur.map(String::from));
        }
    }
    names
}

/// Entries in the plan that replace each other, and entries that replace an
/// installed package.
fn conflicts(entries: &[&Entry], platform: &LinuxInfo, facts: &Facts, into: &mut Vec<Finding>) {
    for (index, entry) in entries.iter().enumerate() {
        let Some(identity) = entry.identity() else {
            continue;
        };
        for other in &entries[index + 1..] {
            let theirs = installs(other, platform);
            let ours = installs(entry, platform);
            let clash = identity.conflicts.iter().any(|name| theirs.contains(name))
                || other
                    .identity()
                    .is_some_and(|their| their.conflicts.iter().any(|name| ours.contains(name)));
            if clash {
                into.push(Finding::blocker(format!(
                    "{} and {} replace each other; queue only one",
                    entry.name, other.name
                )));
            }
        }
        if let Some(PackageState::Absent { replaces }) =
            package_state(entry, platform, facts.machine)
            && !replaces.is_empty()
        {
            into.push(Finding::warning(format!(
                "{} replaces {}, which is installed; the package manager asks before removing it",
                entry.name,
                replaces.join(", ")
            )));
        }
    }
}

/// Repositories and graphics hardware the entries need.
fn needs(entries: &[&Entry], platform: &LinuxInfo, facts: &Facts, into: &mut Vec<Finding>) {
    let mut enabled = None;
    let mut found = None;
    for entry in entries {
        if platform.manager == Some(PackageManager::Arch) && !entry.needs.repositories.is_empty() {
            let enabled = enabled.get_or_insert_with(|| (facts.repositories)());
            if let Some(enabled) = enabled {
                for repository in &entry.needs.repositories {
                    if !enabled.contains(repository) {
                        into.push(Finding::blocker(format!(
                            "{} needs the [{repository}] repository; enable it in /etc/pacman.conf and run sudo pacman -Sy first",
                            entry.name
                        )));
                    }
                }
            }
        }
        if !entry.needs.gpu.is_empty() {
            let found = found.get_or_insert_with(|| (facts.gpus)());
            if !entry.needs.gpu.iter().any(|vendor| found.contains(vendor)) {
                into.push(Finding::warning(format!(
                    "{} is for {} graphics, and this machine has {}",
                    entry.name,
                    entry.needs.gpu.join(" or "),
                    if found.is_empty() {
                        "none that HelixForge can see".to_string()
                    } else {
                        found.join(" and ")
                    }
                )));
            }
        }
    }
}

/// Whether the root file system has room for the packages and what the
/// entries say they need.
fn space(
    plan: &Plan,
    entries: &[&Entry],
    platform: &LinuxInfo,
    facts: &Facts,
    into: &mut Vec<Finding>,
) {
    if plan.steps.is_empty() {
        return;
    }
    let Some(free) = (facts.free_bytes)(Path::new("/")) else {
        return;
    };
    let mut need: u64 = entries
        .iter()
        .filter_map(|entry| entry.needs.space_mb)
        .map(|megabytes| megabytes << 20)
        .sum();
    // Native packages the plan installs, as far as the manager can size them.
    let mut flatpak = false;
    let mut packages = Vec::new();
    for step in &plan.steps {
        if let Step::Package {
            sources,
            removal: false,
            ..
        } = step
        {
            match sources.first().and_then(|source| source.probe.as_ref()) {
                Some(Probe::Package { name, .. }) => packages.push(name.clone()),
                Some(Probe::Flatpak(_)) => flatpak = true,
                None => {}
            }
        }
    }
    let sized = match platform.manager {
        Some(manager) if !packages.is_empty() => (facts.sizes)(manager, &packages),
        _ => None,
    };
    need += sized.unwrap_or(0);
    let shown = |bytes: u64| format!("{:.1} GiB", bytes as f64 / f64::from(1u32 << 30));
    if need > 0 && free < need {
        into.push(Finding::blocker(format!(
            "the plan needs about {} and the root file system has {} free",
            shown(need),
            shown(free)
        )));
    } else if free < need + SPARE {
        into.push(Finding::warning(format!(
            "the root file system has {} free; after this plan less than {} would be left",
            shown(free),
            shown(SPARE)
        )));
    } else if flatpak && free < 4 * SPARE {
        into.push(Finding::warning(format!(
            "the root file system has {} free, and a Flatpak with its runtime can take several GiB",
            shown(free)
        )));
    }
}

/// Free bytes for an unprivileged user on the file system holding `path`.
pub fn free_bytes(path: &Path) -> Option<u64> {
    let stats = nix::sys::statvfs::statvfs(path).ok()?;
    Some(stats.blocks_available() as u64 * stats.fragment_size() as u64)
}

/// The graphics vendors among this machine's PCI display controllers.
pub fn gpus() -> Vec<String> {
    gpus_in(Path::new("/sys/bus/pci/devices"))
}

/// `gpus` under another sysfs directory, for tests.
pub fn gpus_in(devices: &Path) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(listing) = std::fs::read_dir(devices) else {
        return found;
    };
    for device in listing.flatten() {
        let read =
            |name: &str| std::fs::read_to_string(device.path().join(name)).unwrap_or_default();
        if !read("class").trim().starts_with("0x03") {
            continue;
        }
        let vendor = match read("vendor").trim() {
            "0x10de" => "nvidia",
            "0x1002" => "amd",
            "0x8086" => "intel",
            _ => continue,
        };
        if !found.iter().any(|known| known == vendor) {
            found.push(vendor.to_string());
        }
    }
    found.sort();
    found
}

/// The repositories a pacman.conf enables: its sections other than options.
pub fn pacman_repositories(conf: &Path) -> Option<Vec<String>> {
    Some(repositories_in(&std::fs::read_to_string(conf).ok()?))
}

/// The repositories a pacman.conf's text enables.
pub fn repositories_in(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix('[')?.strip_suffix(']'))
        .filter(|name| *name != "options")
        .map(String::from)
        .collect()
}

/// The installed size of packages from the repository metadata, summed;
/// None when the manager cannot say for this family.
pub fn installed_size(manager: PackageManager, packages: &[String]) -> Option<u64> {
    let names: Vec<&str> = packages
        .iter()
        .map(String::as_str)
        .filter(|name| valid_package_name(name))
        .collect();
    if names.is_empty() {
        return None;
    }
    let (program, mut args): (&str, Vec<&str>) = match manager {
        PackageManager::Arch => ("pacman", vec!["-Si"]),
        PackageManager::Debian => ("apt-cache", vec!["show", "--no-all-versions"]),
        PackageManager::Fedora => (
            "dnf",
            vec![
                "-C",
                "-q",
                "repoquery",
                "--latest-limit=1",
                "--qf",
                "%{installsize}\n",
            ],
        ),
        _ => return None,
    };
    args.extend(&names);
    let output = Command::new(program)
        .args(&args)
        .env("LC_ALL", "C")
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    parse_sizes(manager, &text)
}

/// The summed sizes in a package manager's answer.
pub fn parse_sizes(manager: PackageManager, text: &str) -> Option<u64> {
    let sizes: Vec<u64> = match manager {
        // "Installed Size  : 4388.95 KiB"
        PackageManager::Arch => text
            .lines()
            .filter_map(|line| line.strip_prefix("Installed Size"))
            .filter_map(|rest| {
                let mut words = rest.trim_start_matches([' ', ':']).split_whitespace();
                let number: f64 = words.next()?.parse().ok()?;
                let unit = match words.next()? {
                    "B" => 1.0,
                    "KiB" => 1024.0,
                    "MiB" => 1024.0 * 1024.0,
                    "GiB" => 1024.0 * 1024.0 * 1024.0,
                    _ => return None,
                };
                Some((number * unit) as u64)
            })
            .collect(),
        // "Installed-Size: 1234", in KiB
        PackageManager::Debian => text
            .lines()
            .filter_map(|line| line.strip_prefix("Installed-Size:"))
            .filter_map(|rest| rest.trim().parse::<u64>().ok())
            .map(|kib| kib * 1024)
            .collect(),
        // One number of bytes per line
        PackageManager::Fedora => text
            .lines()
            .filter_map(|line| line.trim().parse::<u64>().ok())
            .collect(),
        _ => Vec::new(),
    };
    (!sizes.is_empty()).then(|| sizes.iter().sum())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan_with;
    use helixforge_core::{Action, Needs, PackageNames};

    fn arch() -> LinuxInfo {
        LinuxInfo {
            id: "garuda".into(),
            pretty_name: "Garuda Linux".into(),
            id_like: vec!["arch".into()],
            manager: Some(PackageManager::Arch),
        }
    }

    fn package(id: &str, arch: &str, conflicts: &[&str], needs: Needs) -> Entry {
        Entry {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            flags: "I".into(),
            multi_select: true,
            writes: Vec::new(),
            action: Action::Package {
                binary: None,
                packages: PackageNames {
                    arch: Some(arch.into()),
                    ..Default::default()
                },
                aur: None,
                flatpak: None,
                conflicts: conflicts.iter().map(|name| name.to_string()).collect(),
                removable: true,
            },
            detect: None,
            needs,
        }
    }

    fn facts_of<'a>(
        gpus: &'a dyn Fn() -> Vec<String>,
        repositories: &'a dyn Fn() -> Option<Vec<String>>,
        sizes: &'a dyn Fn(PackageManager, &[String]) -> Option<u64>,
        present: &'a dyn Fn(&str) -> bool,
        free_bytes: &'a dyn Fn(&Path) -> Option<u64>,
    ) -> Facts<'a> {
        Facts {
            machine: Machine {
                holds: &|_| false,
                on_path: &|_| false,
            },
            present,
            free_bytes,
            gpus,
            repositories,
            sizes,
            active_runs: &Vec::new,
        }
    }

    const GIB: u64 = 1 << 30;

    fn run(entries: &[&Entry], facts: &Facts) -> Vec<Finding> {
        let planned =
            plan_with(entries.iter().copied(), &arch(), &|_| None, facts.machine).unwrap();
        let lookup = |id: &str| entries.iter().copied().find(|entry| entry.id == id);
        check(&planned, &arch(), &lookup, facts)
    }

    #[test]
    fn entries_that_replace_each_other_cannot_be_queued_together() {
        let wine = package("wine", "wine", &["wine-staging"], Needs::default());
        let staging = package("wine-staging", "wine-staging", &["wine"], Needs::default());
        let meld = package("meld", "meld", &[], Needs::default());
        let plenty = |_: &Path| Some(100 * GIB);
        let facts = facts_of(&Vec::new, &|| None, &|_, _| None, &|_| true, &plenty);
        let found = run(&[&wine, &meld, &staging], &facts);
        assert_eq!(
            found,
            [Finding::blocker(
                "wine and wine-staging replace each other; queue only one".into()
            )]
        );
        assert!(blocked(&found));
        assert!(run(&[&wine, &meld], &facts).is_empty());
    }

    #[test]
    fn missing_tools_and_repositories_block_and_hardware_warns() {
        let steam = package(
            "steam",
            "steam",
            &[],
            Needs {
                repositories: vec!["multilib".into()],
                ..Default::default()
            },
        );
        let nvidia = package(
            "nvidia",
            "nvidia-dkms",
            &[],
            Needs {
                gpu: vec!["nvidia".into()],
                ..Default::default()
            },
        );
        let plenty = |_: &Path| Some(100 * GIB);
        let intel = || vec!["intel".to_string()];
        let core_only = || Some(vec!["core".to_string(), "extra".to_string()]);
        let no_pacman = |tool: &str| tool != "pacman";
        let facts = facts_of(&intel, &core_only, &|_, _| None, &no_pacman, &plenty);
        let found = run(&[&steam, &nvidia], &facts);
        let lines: Vec<String> = found.iter().map(Finding::line).collect();
        assert_eq!(
            lines,
            [
                "Blocked: pacman is needed and is not installed",
                "Blocked: steam needs the [multilib] repository; enable it in /etc/pacman.conf and run sudo pacman -Sy first",
                "Warning: nvidia is for nvidia graphics, and this machine has intel",
            ]
        );
        // With multilib on and an Nvidia card, nothing to say.
        let both = || vec!["intel".to_string(), "nvidia".to_string()];
        let multilib = || Some(vec!["core".to_string(), "multilib".to_string()]);
        let facts = facts_of(&both, &multilib, &|_, _| None, &|_| true, &plenty);
        assert!(run(&[&steam, &nvidia], &facts).is_empty());
    }

    #[test]
    fn the_disk_must_have_room_for_the_packages() {
        let big = package(
            "resolve",
            "davinci-resolve",
            &[],
            Needs {
                space_mb: Some(4096),
                ..Default::default()
            },
        );
        let sized = |_: PackageManager, names: &[String]| Some(names.len() as u64 * GIB);
        let tight = |_: &Path| Some(3 * GIB);
        let facts = facts_of(&Vec::new, &|| None, &sized, &|_| true, &tight);
        let found = run(&[&big], &facts);
        assert_eq!(
            found[0].line(),
            "Blocked: the plan needs about 5.0 GiB and the root file system has 3.0 GiB free"
        );
        let close = |_: &Path| Some(5 * GIB + GIB / 2);
        let facts = facts_of(&Vec::new, &|| None, &sized, &|_| true, &close);
        assert_eq!(
            found_severity(&run(&[&big], &facts)),
            Some(Severity::Warning)
        );
        let roomy = |_: &Path| Some(50 * GIB);
        let facts = facts_of(&Vec::new, &|| None, &sized, &|_| true, &roomy);
        assert!(run(&[&big], &facts).is_empty());
    }

    #[test]
    fn another_run_still_going_blocks() {
        let meld = package("meld", "meld", &[], Needs::default());
        let plenty = |_: &Path| Some(100 * GIB);
        let busy = || vec!["Steam, Lutris".to_string()];
        let facts = Facts {
            active_runs: &busy,
            ..facts_of(&Vec::new, &|| None, &|_, _| None, &|_| true, &plenty)
        };
        assert_eq!(
            run(&[&meld], &facts)[0].line(),
            "Blocked: another HelixForge run is still going (Steam, Lutris); wait for it to finish"
        );
    }

    fn found_severity(found: &[Finding]) -> Option<Severity> {
        found.first().map(|finding| finding.severity)
    }

    #[test]
    fn machine_facts_are_read() {
        let devices = tempfile::tempdir().unwrap();
        for (slot, class, vendor) in [
            ("0000:00:02.0", "0x030000", "0x8086"),
            ("0000:01:00.0", "0x030200", "0x10de"),
            ("0000:00:1f.3", "0x040300", "0x8086"),
        ] {
            let device = devices.path().join(slot);
            std::fs::create_dir(&device).unwrap();
            std::fs::write(device.join("class"), format!("{class}\n")).unwrap();
            std::fs::write(device.join("vendor"), format!("{vendor}\n")).unwrap();
        }
        assert_eq!(gpus_in(devices.path()), ["intel", "nvidia"]);
        let conf = devices.path().join("pacman.conf");
        std::fs::write(
            &conf,
            "[options]\nColor\n[core]\nInclude = x\n#[multilib]\n#Include = x\n[chaotic-aur]\n",
        )
        .unwrap();
        assert_eq!(pacman_repositories(&conf).unwrap(), ["core", "chaotic-aur"]);
        assert_eq!(
            parse_sizes(
                PackageManager::Arch,
                "Name : a\nInstalled Size  : 4388.95 KiB\nName : b\nInstalled Size  : 1.50 MiB\n"
            ),
            Some(4494284 + 1572864)
        );
        assert_eq!(
            parse_sizes(PackageManager::Debian, "Package: a\nInstalled-Size: 100\n"),
            Some(102400)
        );
        assert_eq!(
            parse_sizes(PackageManager::Fedora, "2048\n4096\n"),
            Some(6144)
        );
        assert_eq!(
            parse_sizes(PackageManager::Arch, "error: package not found\n"),
            None
        );
        assert!(free_bytes(Path::new("/")).is_some_and(|free| free > 0));
    }
}
