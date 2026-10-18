//! How far each entry has been tested: real installs in distribution
//! containers (recorded by the Distributions workflow and carried by release
//! builds), installer tests with stand-in commands, and reports from virtual
//! and real machines (`catalog/validation.toml`), with this machine's own
//! runs beside them. Nothing here claims more than its record shows.

use crate::history;
use helixforge_core::{Action, Entry, Risk};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

/// How far an entry has been validated, best first.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Level {
    /// Run on a real machine, from a tester's report.
    HardwareTested,
    /// Run in a virtual machine, from a report.
    VmTested,
    /// Installed (or run) and verified in a distribution container by the
    /// Distributions workflow.
    ContainerTested,
    /// Its installer was run against stand-in commands and files.
    StandInTested,
    NotYetValidated,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::HardwareTested => "Hardware Tested",
            Level::VmTested => "VM Tested",
            Level::ContainerTested => "Container Tested",
            Level::StandInTested => "Stand-in Tested",
            Level::NotYetValidated => "Not Yet Validated",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Level::HardwareTested => "A tester ran it on a real machine and reported the result.",
            Level::VmTested => {
                "It ran for real in a virtual machine: the Distributions workflow boots a distribution's cloud image or live ISO, runs the entry, reboots into the result and checks it (the configuration, the kernel command line, the boot images, the services, a graphical boot menu or login screen). Real graphics hardware and firmware are not covered."
            }
            Level::ContainerTested => {
                "The Distributions workflow ran it in distribution containers and HelixForge verified the result there. Containers have no boot loader, login screen or desktop, so those parts are not covered."
            }
            Level::StandInTested => {
                "Installer tests run its script, or the helpers it calls, against stand-in commands and files, on every change and in each distribution's container. They show what it would change, without changing a system."
            }
            Level::NotYetValidated => {
                "No automated test runs it and nobody has reported running it. Its plan is built and checked, like every entry's."
            }
        }
    }
}

/// `catalog/validation.toml`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Claims {
    #[serde(default)]
    pub critical: Critical,
    #[serde(default, rename = "stand-ins")]
    pub stand_ins: Vec<StandIns>,
    #[serde(default)]
    pub hardware: Vec<Report>,
    #[serde(default)]
    pub vm: Vec<Report>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Critical {
    #[serde(default)]
    pub entries: Vec<String>,
}

/// Installer tests that cover entries with stand-ins.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct StandIns {
    pub entries: Vec<String>,
    pub tests: Vec<String>,
    #[serde(default)]
    pub covers: String,
}

/// A run reported from a virtual or real machine.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Report {
    pub entry: String,
    pub system: String,
    #[serde(default)]
    pub version: String,
    pub date: String,
    #[serde(default)]
    pub note: String,
}

/// What a Distributions run verified in each container, as release builds
/// carry it (`catalog/validation-results.json`).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Results {
    #[serde(default)]
    pub commit: String,
    #[serde(default)]
    pub run: u64,
    #[serde(default)]
    pub url: String,
    /// When the run finished, in Unix seconds.
    #[serde(default)]
    pub finished: u64,
    /// By container image: each entry and what was shown (`verified`, or
    /// `ran` for an entry with nothing to verify).
    #[serde(default)]
    pub images: BTreeMap<String, Image>,
    /// By virtual machine (arch, debian): the entries run there, rebooted
    /// into and checked by scripts/vm-tests.py.
    #[serde(default)]
    pub vms: BTreeMap<String, Image>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Image {
    #[serde(default)]
    pub entries: BTreeMap<String, String>,
}

/// Everything known about testing, from this build's catalog.
#[derive(Clone, Debug, Default)]
pub struct Evidence {
    pub claims: Claims,
    pub results: Option<Results>,
    /// Why a file could not be read, if one could not.
    pub problems: Vec<String>,
}

/// One entry's standing.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
pub struct Assessment {
    pub level: Level,
    /// Where the tests ran: container images or reported systems.
    pub systems: Vec<String>,
    /// When it was last verified, as a date, when known.
    pub verified: Option<String>,
    /// What shows it, line by line.
    pub evidence: Vec<String>,
    /// Its last successful run on this machine, in Unix seconds.
    pub here: Option<u64>,
    /// A boot, login-screen, driver or whole-system entry.
    pub critical: bool,
}

/// A virtual machine as people name it: "Arch VM".
pub fn vm_label(name: &str) -> String {
    match name {
        "arch" | "arch-themes" => "Arch VM".to_string(),
        "debian" => "Debian VM".to_string(),
        "fedora" => "Fedora VM".to_string(),
        "opensuse" => "openSUSE VM".to_string(),
        "arch-uefi" => "Arch UEFI VM".to_string(),
        "arch-iso" => "Arch live ISO VM".to_string(),
        "steamdeck" => "Steam Deck stand-in VM".to_string(),
        other => format!("{} VM", helixforge_core::printable(other)),
    }
}

/// A container image as people name the system.
pub fn image_label(image: &str) -> String {
    // The ARM64 build's containers: "debian:stable on aarch64".
    if let Some(image) = image.strip_suffix(" on aarch64") {
        return format!("{} on ARM64", image_label(image));
    }
    match image {
        "archlinux:latest" => "Arch".to_string(),
        "debian:stable" => "Debian".to_string(),
        "fedora:latest" => "Fedora".to_string(),
        "opensuse/tumbleweed:latest" => "openSUSE Tumbleweed".to_string(),
        other => match other.split_once(':') {
            Some(("ubuntu", version)) => format!("Ubuntu {version}"),
            _ => helixforge_core::printable(other),
        },
    }
}

impl Evidence {
    /// Read this build's record: `catalog/validation.toml` and, in a
    /// release build, `catalog/validation-results.json`.
    pub fn load(root: &Path) -> Self {
        let mut evidence = Evidence::default();
        let catalog = root.join("catalog");
        let claims = catalog.join("validation.toml");
        match std::fs::read_to_string(&claims) {
            Ok(text) => match toml::from_str(&text) {
                Ok(claims) => evidence.claims = claims,
                Err(error) => evidence.problems.push(format!(
                    "{}: {}",
                    claims.display(),
                    helixforge_core::printable(error.message())
                )),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => evidence
                .problems
                .push(format!("{}: {error}", claims.display())),
        }
        let results: PathBuf = catalog.join("validation-results.json");
        if let Ok(text) = std::fs::read_to_string(&results) {
            match serde_json::from_str(&text) {
                Ok(results) => evidence.results = Some(results),
                Err(error) => evidence
                    .problems
                    .push(format!("{}: {error}", results.display())),
            }
        }
        evidence
    }

    /// Whether failing would leave the machine without a working boot,
    /// login screen or display, or the entry changes disks or the whole
    /// system.
    pub fn critical(&self, entry: &Entry) -> bool {
        Risk::from_flags(&entry.flags) == Risk::High
            || self.claims.critical.entries.contains(&entry.id)
    }

    /// How an entry stands; `history` is this machine's, newest first.
    pub fn assess(&self, entry: &Entry, history: &[history::Record]) -> Assessment {
        let here = history
            .iter()
            .find(|record| record.success && record.entries.contains(&entry.id))
            .map(|record| record.unix_timestamp);
        let mut assessment = Assessment {
            level: Level::NotYetValidated,
            systems: Vec::new(),
            verified: None,
            evidence: Vec::new(),
            here,
            critical: self.critical(entry),
        };
        let better = |assessment: &mut Assessment, level: Level| {
            if level < assessment.level {
                assessment.level = level;
            }
        };
        for (kind, reports, level) in [
            (
                "a real machine",
                &self.claims.hardware,
                Level::HardwareTested,
            ),
            ("a virtual machine", &self.claims.vm, Level::VmTested),
        ] {
            for report in reports.iter().filter(|report| report.entry == entry.id) {
                better(&mut assessment, level);
                let system = helixforge_core::printable(&report.system);
                if !assessment.systems.contains(&system) {
                    assessment.systems.push(system.clone());
                }
                let date = helixforge_core::printable(&report.date);
                if assessment
                    .verified
                    .as_ref()
                    .is_none_or(|known| *known < date)
                {
                    assessment.verified = Some(date.clone());
                }
                let mut line = format!("Ran on {kind}: {system}, {date}");
                if !report.version.is_empty() {
                    line.push_str(&format!(
                        ", HelixForge {}",
                        helixforge_core::printable(&report.version)
                    ));
                }
                if !report.note.is_empty() {
                    line.push_str(&format!(" ({})", helixforge_core::printable(&report.note)));
                }
                assessment.evidence.push(line);
            }
        }
        if let Some(results) = &self.results {
            let date = crate::date(results.finished);
            let machines: Vec<String> = results
                .vms
                .iter()
                .filter(|(_, found)| {
                    found
                        .entries
                        .get(&entry.id)
                        .is_some_and(|outcome| outcome == "verified")
                })
                .map(|(name, _)| vm_label(name))
                .collect();
            if !machines.is_empty() {
                better(&mut assessment, Level::VmTested);
                for machine in &machines {
                    if !assessment.systems.contains(machine) {
                        assessment.systems.push(machine.clone());
                    }
                }
                if assessment
                    .verified
                    .as_ref()
                    .is_none_or(|known| *known < date)
                {
                    assessment.verified = Some(date.clone());
                }
                assessment.evidence.push(format!(
                    "In virtual machines, Distributions run {} on {date}: {}; run for real, rebooted into and checked",
                    results.run,
                    machines.join(", ")
                ));
            }
            let mut shown: Vec<String> = Vec::new();
            for (image, found) in &results.images {
                if let Some(outcome) = found.entries.get(&entry.id) {
                    shown.push(format!(
                        "{} ({})",
                        image_label(image),
                        if outcome == "verified" {
                            "installed and verified"
                        } else {
                            "ran"
                        }
                    ));
                    let label = image_label(image);
                    if !assessment.systems.contains(&label) {
                        assessment.systems.push(label);
                    }
                }
            }
            if !shown.is_empty() {
                better(&mut assessment, Level::ContainerTested);
                if assessment
                    .verified
                    .as_ref()
                    .is_none_or(|known| *known < date)
                {
                    assessment.verified = Some(date.clone());
                }
                assessment.evidence.push(format!(
                    "In containers, Distributions run {} on {date}: {}",
                    results.run,
                    shown.join(", ")
                ));
            }
        }
        for stand_ins in self
            .claims
            .stand_ins
            .iter()
            .filter(|group| group.entries.contains(&entry.id))
        {
            better(&mut assessment, Level::StandInTested);
            let mut line = format!(
                "Installer tests with stand-ins ({}): {}",
                stand_ins.tests.join(", "),
                helixforge_core::printable(&stand_ins.covers)
            );
            if let Some(results) = &self.results {
                let date = crate::date(results.finished);
                line.push_str(&format!(
                    "; they ran in all {} containers on {date}",
                    results.images.len()
                ));
                if assessment.level == Level::StandInTested {
                    assessment.verified = Some(date);
                    for image in results.images.keys() {
                        let label = image_label(image);
                        if !assessment.systems.contains(&label) {
                            assessment.systems.push(label);
                        }
                    }
                }
            }
            assessment.evidence.push(line);
        }
        if let Action::Package { .. } = entry.action {
            assessment.evidence.push(
                "Its commands are built by HelixForge from package names, and tested against harmless stand-ins on every change.".to_string(),
            );
        }
        if let Some(when) = here {
            assessment.evidence.push(format!(
                "Ran successfully on this machine, {}",
                crate::date(when)
            ));
        }
        assessment
    }

    /// A line on the record this build carries, for the screen's header.
    pub fn summary(&self) -> String {
        match &self.results {
            Some(results) => format!(
                "Container record: Distributions run {} on commit {}, {}, {} distributions{}.",
                results.run,
                results.commit.chars().take(12).collect::<String>(),
                crate::date(results.finished),
                results.images.len(),
                match results.vms.len() {
                    0 => String::new(),
                    1 => " and 1 virtual machine".to_string(),
                    count => format!(" and {count} virtual machines"),
                }
            ),
            None => "No container record in this build; release builds carry one.".to_string(),
        }
    }
}

/// Warnings for a plan's entries that are critical and not validated past
/// stand-ins, shown before it runs.
pub fn warnings<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    evidence: &Evidence,
) -> Vec<String> {
    let mut warnings = Vec::new();
    for id in ids {
        let Some(entry) = lookup(id) else { continue };
        if !evidence.critical(entry) {
            continue;
        }
        let assessment = evidence.assess(entry, &[]);
        if assessment.level >= Level::ContainerTested {
            warnings.push(format!(
                "{} is {}: {}",
                entry.name,
                assessment.level.label(),
                match assessment.level {
                    Level::NotYetValidated => "nothing has tested it yet",
                    Level::StandInTested => "only tested with stand-ins, never on a real system",
                    _ => "containers cannot test boot, login or drivers",
                }
            ));
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use helixforge_catalog::Catalog;

    fn evidence(claims: &str, results: Option<&str>) -> Evidence {
        Evidence {
            claims: toml::from_str(claims).unwrap(),
            results: results.map(|text| serde_json::from_str(text).unwrap()),
            problems: Vec::new(),
        }
    }

    const RESULTS: &str = r#"{"format": 1, "commit": "0123456789abcdef0123", "run": 42,
        "finished": 1792172552,
        "images": {"archlinux:latest": {"entries": {"htop": "verified"}},
                   "ubuntu:24.04": {"entries": {"htop": "verified", "system-report": "ran"}}},
        "vms": {"arch": {"entries": {"sddm": "verified"}}}}"#;

    #[test]
    fn container_images_are_named_as_systems() {
        assert_eq!(image_label("debian:stable"), "Debian");
        assert_eq!(image_label("ubuntu:22.04"), "Ubuntu 22.04");
        assert_eq!(
            image_label("opensuse/tumbleweed:latest on aarch64"),
            "openSUSE Tumbleweed on ARM64"
        );
    }

    #[test]
    fn levels_come_from_the_record_never_beyond_it() {
        let catalog = Catalog::builtin(false).unwrap();
        let found = evidence(
            r#"
            [critical]
            entries = ["sddm", "grub"]
            [[stand-ins]]
            entries = ["sddm"]
            tests = ["test_sddm"]
            covers = "the theme"
            [[hardware]]
            entry = "grub"
            system = "Garuda Linux"
            date = "2026-10-20"
            "#,
            Some(RESULTS),
        );
        let assess = |id: &str| found.assess(catalog.find(id).unwrap(), &[]);
        let htop = assess("htop");
        assert_eq!(htop.level, Level::ContainerTested);
        assert_eq!(htop.systems, ["Arch", "Ubuntu 24.04"]);
        assert_eq!(htop.verified.as_deref(), Some("2026-10-16"));
        assert!(!htop.critical);
        assert_eq!(assess("system-report").level, Level::ContainerTested);
        // Verified in a virtual machine: above its stand-in tests.
        let sddm = assess("sddm");
        assert_eq!(sddm.level, Level::VmTested);
        assert!(sddm.critical);
        assert!(
            sddm.systems.contains(&"Arch VM".to_string()),
            "{:?}",
            sddm.systems
        );
        assert!(
            sddm.evidence[0].contains("virtual machines"),
            "{:?}",
            sddm.evidence
        );
        assert!(sddm.evidence.iter().any(|line| line.contains("test_sddm")));
        assert!(
            found.summary().contains(" and 1 virtual machine."),
            "{}",
            found.summary()
        );
        let grub = assess("grub");
        assert_eq!(grub.level, Level::HardwareTested);
        assert_eq!(grub.verified.as_deref(), Some("2026-10-20"));
        assert_eq!(assess("ripgrep").level, Level::NotYetValidated);
        // This machine's runs show beside the level, never raise it.
        let ran = [history::Record {
            unix_timestamp: 1_700_000_000,
            entries: vec!["ripgrep".into()],
            names: Vec::new(),
            success: true,
            result: String::new(),
            log: None,
        }];
        let ripgrep = found.assess(catalog.find("ripgrep").unwrap(), &ran);
        assert_eq!(ripgrep.level, Level::NotYetValidated);
        assert_eq!(ripgrep.here, Some(1_700_000_000));

        // Before a run, critical entries short of a real test are named.
        // SDDM, tested in a VM, and GRUB, on hardware, are not.
        let warned = warnings(
            ["sddm", "grub", "htop", "arch-server-setup"],
            &|id| catalog.find(id),
            &found,
        );
        assert_eq!(warned.len(), 1, "{warned:?}");
        assert!(warned[0].starts_with("Arch Server Setup"), "{warned:?}");
        // Whole-system entries count as critical by their risk.
        assert!(found.critical(catalog.find("arch-server-setup").unwrap()));
    }

    #[test]
    fn a_build_without_a_record_says_so() {
        let found = evidence("", None);
        assert!(found.summary().contains("No container record"));
        let catalog = Catalog::builtin(false).unwrap();
        assert_eq!(
            found.assess(catalog.find("htop").unwrap(), &[]).level,
            Level::NotYetValidated
        );
    }

    #[test]
    fn the_catalogs_own_record_reads_and_names_real_entries() {
        let catalog = Catalog::builtin(false).unwrap();
        let found = Evidence::load(catalog.root());
        assert!(found.problems.is_empty(), "{:?}", found.problems);
        assert!(!found.claims.stand_ins.is_empty());
        for id in found
            .claims
            .stand_ins
            .iter()
            .flat_map(|group| group.entries.iter())
            .chain(found.claims.critical.entries.iter())
            .chain(found.claims.hardware.iter().map(|report| &report.entry))
            .chain(found.claims.vm.iter().map(|report| &report.entry))
        {
            assert!(catalog.find(id).is_some(), "validation.toml names {id}");
        }
    }
}
