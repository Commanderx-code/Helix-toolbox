//! Hardware reports: a run that worked on a real machine, written up for an
//! issue so that it can become a Hardware Tested record in
//! `catalog/validation.toml`. Containers and virtual machines cannot test
//! graphics drivers, firmware or a real Steam Deck; the people who have the
//! hardware can. A report names the distribution, the desktop, the model and
//! the graphics, never a serial number, the host name or the user.

use crate::{desktop::Desktop, doctor::System, hardware, history::Record, updates};
use helix_toolbox_core::printable;
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;
use std::path::Path;

/// Where a report is sent: a new issue from the hardware report form.
pub const ISSUES: &str = "https://github.com/Commanderx-code/Helix-toolbox/issues/new";
/// The form, in `.github/ISSUE_TEMPLATE`.
pub const TEMPLATE: &str = "hardware_report.yml";

/// What a report says about the machine.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Machine {
    /// "Garuda Linux".
    pub distribution: String,
    /// "KDE Plasma 6.5.0 (wayland)".
    pub desktop: Option<String>,
    /// "Lenovo ThinkPad T15p Gen 2i", "Steam Deck OLED".
    pub model: Option<String>,
    /// "NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)", one per card.
    pub graphics: Vec<String>,
    pub kernel: Option<String>,
    pub architecture: String,
    /// What it runs in, when it is a virtual machine or a container.
    pub virtualized: Option<String>,
}

impl Machine {
    /// The record's `system`: the distribution, desktop, model and graphics.
    pub fn system(&self) -> String {
        let mut parts = vec![self.distribution.clone()];
        parts.extend(self.desktop.clone());
        parts.extend(self.model.clone());
        if !self.graphics.is_empty() {
            parts.push(self.graphics.join(" and "));
        }
        parts.join(", ")
    }
}

/// Read the machine; `desktop` is what the Desktop screen read.
pub fn machine(system: &dyn System, platform: &LinuxInfo, desktop: &Desktop) -> Machine {
    Machine {
        distribution: printable(&platform.pretty_name),
        desktop: desktop.name.as_ref().map(|name| match &desktop.session {
            Some(session) => format!("{} ({})", printable(name), printable(session)),
            None => printable(name),
        }),
        model: model(system),
        graphics: graphics(system),
        kernel: read(system, "/proc/sys/kernel/osrelease"),
        architecture: std::env::consts::ARCH.to_string(),
        virtualized: virtualized(system),
    }
}

fn read(system: &dyn System, path: &str) -> Option<String> {
    system
        .read(Path::new(path))
        .map(|text| printable(text.trim()))
        .filter(|text| !text.is_empty())
}

/// What firmware writes when the maker left a field empty.
fn placeholder(text: &str) -> bool {
    let text = text.to_ascii_lowercase();
    [
        "to be filled by o.e.m.",
        "system product name",
        "system manufacturer",
        "system version",
        "default string",
        "not applicable",
        "not specified",
        "none",
        "o.e.m.",
        "oem",
    ]
    .contains(&text.as_str())
        || text.starts_with("type1productconfigid")
}

/// The maker and model, from the firmware's DMI table: the names, not the
/// serial numbers beside them.
pub fn model(system: &dyn System) -> Option<String> {
    let field = |name: &str| {
        read(system, &format!("/sys/class/dmi/id/{name}")).filter(|text| !placeholder(text))
    };
    let vendor = field("sys_vendor");
    let product = field("product_name");
    if vendor.as_deref() == Some("Valve") {
        return Some(
            match product.as_deref() {
                Some("Jupiter") => "Steam Deck (LCD)",
                Some("Galileo") => "Steam Deck OLED",
                _ => "a Valve machine",
            }
            .to_string(),
        );
    }
    // Lenovo keeps the name people know in the version, the part number in
    // the name.
    let product = if vendor.as_deref() == Some("LENOVO") {
        field("product_version")
            .or_else(|| field("product_family"))
            .or(product)
    } else {
        product
    };
    let vendor = vendor.map(|vendor| {
        match vendor.as_str() {
            "LENOVO" => "Lenovo",
            "ASUSTeK COMPUTER INC." => "ASUS",
            "Dell Inc." => "Dell",
            "HP" | "Hewlett-Packard" => "HP",
            "Micro-Star International Co., Ltd." => "MSI",
            "Gigabyte Technology Co., Ltd." => "Gigabyte",
            "Acer" => "Acer",
            "Framework" => "Framework",
            other => other,
        }
        .to_string()
    });
    match (vendor, product) {
        (Some(vendor), Some(product)) if product.starts_with(&vendor) => Some(product),
        (Some(vendor), Some(product)) => Some(format!("{vendor} {product}")),
        (vendor, product) => product.or(vendor),
    }
}

/// The graphics cards, as people name them, with the driver in use.
pub fn graphics(system: &dyn System) -> Vec<String> {
    hardware::pci(system)
        .iter()
        .filter(|device| {
            device.class.as_deref().is_some_and(|class| {
                [
                    "VGA compatible controller",
                    "3D controller",
                    "Display controller",
                ]
                .iter()
                .any(|kind| class.starts_with(kind))
            })
        })
        .map(|device| {
            let name = card(&device.name);
            match &device.driver {
                Some(driver) => format!("{name} ({driver})"),
                None => name,
            }
        })
        .collect()
}

/// "NVIDIA Corporation TU117M [GeForce GTX 1650 Mobile / Max-Q] [10de:1f99]
/// (rev a1)" as "NVIDIA GeForce GTX 1650 Mobile / Max-Q".
fn card(name: &str) -> String {
    let mut name = name.trim();
    if let Some((before, _)) = name.rsplit_once(" (rev ") {
        name = before;
    }
    // The vendor and device ids.
    if name.ends_with(']')
        && let Some((before, ids)) = name.rsplit_once(" [")
        && ids.len() == 10
        && ids.as_bytes()[4] == b':'
    {
        name = before;
    }
    let (vendor, device) = [
        ("NVIDIA Corporation ", "NVIDIA"),
        ("Intel Corporation ", "Intel"),
        ("Advanced Micro Devices, Inc. [AMD/ATI] ", "AMD"),
        ("Advanced Micro Devices, Inc. ", "AMD"),
    ]
    .iter()
    .find_map(|(prefix, short)| Some((*short, name.strip_prefix(prefix)?)))
    .unwrap_or(("", name));
    // The marketing name, where lspci gives one in brackets.
    let device = match device.rsplit_once('[') {
        Some((_, bracketed)) if bracketed.ends_with(']') => bracketed.trim_end_matches(']'),
        _ => device,
    };
    if vendor.is_empty() || device.starts_with(vendor) {
        device.to_string()
    } else {
        format!("{vendor} {device}")
    }
}

/// What systemd says this machine runs in, if anything: `kvm`, `docker`.
pub fn virtualized(system: &dyn System) -> Option<String> {
    let (_, text) = system.run(&["systemd-detect-virt"])?;
    let text = printable(text.trim());
    (!text.is_empty() && text != "none").then_some(text)
}

/// One `[[hardware]]` record, as `catalog/validation.toml` holds it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Claim {
    pub entry: String,
    pub system: String,
    pub version: String,
    pub date: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// A report of one run that worked.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Report {
    /// The entries that ran, with their names.
    pub entries: Vec<(String, String)>,
    pub machine: Machine,
    /// The Helix Toolbox version reporting it.
    pub version: String,
    /// The day it ran.
    pub date: String,
    pub claims: Vec<Claim>,
}

/// The newest successful run of an entry, from a history listed newest
/// first.
pub fn last_success<'a>(history: &'a [Record], id: &str) -> Option<&'a Record> {
    history
        .iter()
        .find(|record| record.success && record.entries.iter().any(|entry| entry == id))
}

/// A report of `record`'s entries, or only `only` of them; refused for a
/// run that did not succeed and on a virtual machine or in a container.
pub fn prepare(
    record: &Record,
    only: Option<&str>,
    machine: Machine,
    version: &str,
    note: &str,
) -> Result<Report, String> {
    if let Some(kind) = &machine.virtualized {
        return Err(format!(
            "This runs in a virtual machine or a container ({kind}). A hardware report is for a real machine; the Distributions workflow already tests in virtual machines and containers."
        ));
    }
    if !record.success {
        return Err(format!(
            "That run did not succeed ({}). A hardware report is for a run that worked; a bug report (helix-toolbox report) is for one that did not.",
            printable(&record.result)
        ));
    }
    let entries: Vec<(String, String)> = record
        .entries
        .iter()
        .enumerate()
        .filter(|(_, id)| only.is_none_or(|only| *id == only))
        .map(|(index, id)| {
            let name = record
                .names
                .get(index)
                .cloned()
                .unwrap_or_else(|| id.clone());
            (printable(id), printable(&name))
        })
        .collect();
    if entries.is_empty() {
        return Err("That run has no entries to report.".to_string());
    }
    let date = updates::date(record.unix_timestamp);
    let system = machine.system();
    let claims = entries
        .iter()
        .map(|(id, _)| Claim {
            entry: id.clone(),
            system: system.clone(),
            version: version.to_string(),
            date: date.clone(),
            note: printable(note.trim()),
        })
        .collect();
    Ok(Report {
        entries,
        machine,
        version: version.to_string(),
        date,
        claims,
    })
}

#[derive(Serialize)]
struct Claims<'a> {
    hardware: &'a [Claim],
}

impl Report {
    /// The records, as TOML to add to `catalog/validation.toml`.
    pub fn records(&self) -> String {
        toml::to_string(&Claims {
            hardware: &self.claims,
        })
        .unwrap_or_default()
    }

    fn names(&self) -> String {
        self.entries
            .iter()
            .map(|(_, name)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The new issue, with the form filled in from the report.
    pub fn issue_url(&self) -> String {
        let entries = self
            .entries
            .iter()
            .map(|(id, name)| format!("{name} ({id})"))
            .collect::<Vec<_>>()
            .join(", ");
        let fields = [
            ("template", TEMPLATE.to_string()),
            ("title", format!("Hardware report: {}", self.names())),
            ("entries", entries),
            ("system", self.machine.system()),
            ("version", self.version.clone()),
            ("date", self.date.clone()),
            ("records", self.records()),
        ];
        let query = fields
            .iter()
            .map(|(name, value)| format!("{name}={}", encode(value)))
            .collect::<Vec<_>>()
            .join("&");
        format!("{ISSUES}?{query}")
    }

    /// The report to read before sending it.
    pub fn text(&self) -> String {
        let mut text = format!(
            "A report that {} worked on this machine, on {}.\n\nSystem: {}\nKernel: {}, {}\nHelix Toolbox: {}\n\nThe record it asks for, in catalog/validation.toml:\n\n{}",
            self.names(),
            self.date,
            self.machine.system(),
            self.machine.kernel.as_deref().unwrap_or("unknown"),
            self.machine.architecture,
            self.version,
            self.records()
        );
        if self.machine.model.is_none() || self.machine.graphics.is_empty() {
            text.push_str(
                "\nThe model or graphics could not be read here; add them in the issue.\n",
            );
        }
        text
    }
}

/// Percent-encoding for a query value: everything but unreserved
/// characters.
fn encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        path::{Path, PathBuf},
    };

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        commands: HashMap<String, String>,
    }

    impl Fake {
        fn file(&mut self, path: &str, text: &str) {
            self.files.insert(PathBuf::from(path), text.to_string());
        }
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
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
                .map(|text| (text.trim() != "none", text.clone()))
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
            (1000, 1000, PathBuf::from("/home/me"))
        }
        fn on_path(&self, _: &str) -> bool {
            true
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const LSPCI: &str = "00:02.0 VGA compatible controller [0300]: Intel Corporation TigerLake-H GT1 [UHD Graphics] [8086:9a60] (rev 01)
\tSubsystem: Lenovo Device [17aa:22da]
\tKernel driver in use: i915
\tKernel modules: i915, xe
00:14.0 USB controller [0c03]: Intel Corporation Tiger Lake-H USB 3.2 Gen 2x1 xHCI Host Controller [8086:43ed] (rev 11)
\tKernel driver in use: xhci_hcd
01:00.0 3D controller [0302]: NVIDIA Corporation TU117M [GeForce GTX 1650 Mobile / Max-Q] [10de:1f99] (rev a1)
\tSubsystem: Lenovo Device [17aa:22da]
\tKernel driver in use: nvidia
\tKernel modules: nouveau, nvidia_drm, nvidia
";

    fn thinkpad() -> Fake {
        let mut fake = Fake::default();
        fake.file("/sys/class/dmi/id/sys_vendor", "LENOVO\n");
        fake.file("/sys/class/dmi/id/product_name", "21A7CTO1WW\n");
        fake.file(
            "/sys/class/dmi/id/product_version",
            "ThinkPad T15p Gen 2i\n",
        );
        fake.file("/sys/class/dmi/id/product_serial", "PF0SECRET\n");
        fake.file("/proc/sys/kernel/osrelease", "7.2.11-1-garuda\n");
        fake.commands.insert("lspci -nnk".into(), LSPCI.into());
        fake.commands
            .insert("systemd-detect-virt".into(), "none\n".into());
        fake
    }

    fn platform() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
            .unwrap()
    }

    fn plasma() -> Desktop {
        Desktop {
            name: Some("KDE Plasma 6.5.0".into()),
            session: Some("wayland".into()),
            ..Desktop::default()
        }
    }

    fn record(success: bool) -> Record {
        Record {
            unix_timestamp: 1_792_540_800,
            entries: vec!["nvidia-open".into(), "sddm".into()],
            names: vec!["NVIDIA open drivers".into(), "SDDM".into()],
            success,
            result: if success {
                "exit status 0"
            } else {
                "exit status 1"
            }
            .into(),
            log: None,
        }
    }

    #[test]
    fn the_machine_is_named_by_its_model_and_graphics_without_serials() {
        let machine = machine(&thinkpad(), &platform(), &plasma());
        assert_eq!(
            machine.system(),
            "Garuda Linux, KDE Plasma 6.5.0 (wayland), Lenovo ThinkPad T15p Gen 2i, Intel UHD Graphics (i915) and NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)"
        );
        assert_eq!(machine.kernel.as_deref(), Some("7.2.11-1-garuda"));
        assert_eq!(machine.virtualized, None);
        assert!(!format!("{machine:?}").contains("PF0SECRET"));
    }

    #[test]
    fn models_come_from_the_firmware_with_placeholders_left_out() {
        let mut deck = Fake::default();
        deck.file("/sys/class/dmi/id/sys_vendor", "Valve\n");
        deck.file("/sys/class/dmi/id/product_name", "Galileo\n");
        assert_eq!(model(&deck).as_deref(), Some("Steam Deck OLED"));
        deck.file("/sys/class/dmi/id/product_name", "Jupiter\n");
        assert_eq!(model(&deck).as_deref(), Some("Steam Deck (LCD)"));
        let mut desktop = Fake::default();
        desktop.file("/sys/class/dmi/id/sys_vendor", "System manufacturer\n");
        desktop.file("/sys/class/dmi/id/product_name", "System Product Name\n");
        assert_eq!(model(&desktop), None);
        let mut asus = Fake::default();
        asus.file("/sys/class/dmi/id/sys_vendor", "ASUSTeK COMPUTER INC.\n");
        asus.file(
            "/sys/class/dmi/id/product_name",
            "ROG Zephyrus G14 GA402RJ\n",
        );
        assert_eq!(
            model(&asus).as_deref(),
            Some("ASUS ROG Zephyrus G14 GA402RJ")
        );
    }

    #[test]
    fn graphics_cards_get_their_marketing_names() {
        assert_eq!(
            card(
                "Advanced Micro Devices, Inc. [AMD/ATI] Navi 23 [Radeon RX 6600/6600 XT/6600M] [1002:73ff] (rev c7)"
            ),
            "AMD Radeon RX 6600/6600 XT/6600M"
        );
        assert_eq!(
            card(
                "Advanced Micro Devices, Inc. [AMD/ATI] VanGogh [AMD Custom GPU 0405] [1002:163f] (rev ae)"
            ),
            "AMD Custom GPU 0405"
        );
        assert_eq!(
            card("Red Hat, Inc. Virtio 1.0 GPU [1af4:1050] (rev 01)"),
            "Red Hat, Inc. Virtio 1.0 GPU"
        );
    }

    #[test]
    fn a_report_holds_a_record_per_entry_and_fills_in_the_issue() {
        let machine = machine(&thinkpad(), &platform(), &plasma());
        let report = prepare(&record(true), None, machine.clone(), "1.15.0", "").unwrap();
        let records = report.records();
        let parsed: crate::reliability::Claims = toml::from_str(&records).unwrap();
        assert_eq!(parsed.hardware.len(), 2);
        assert_eq!(parsed.hardware[0].entry, "nvidia-open");
        assert_eq!(parsed.hardware[1].entry, "sddm");
        assert_eq!(parsed.hardware[0].version, "1.15.0");
        assert_eq!(parsed.hardware[0].date, report.date);
        assert!(parsed.hardware[0].system.contains("GTX 1650"));
        assert!(!records.contains("note"), "{records}");
        let url = report.issue_url();
        assert!(url.starts_with(&format!("{ISSUES}?template={TEMPLATE}&")));
        assert!(url.contains("&title=Hardware%20report%3A%20NVIDIA%20open%20drivers%2C%20SDDM&"));
        assert!(url.contains("&records=%5B%5Bhardware%5D%5D%0A"), "{url}");
        assert!(!url.contains(' ') && !url.contains('\n'));
        assert!(report.text().contains("System: Garuda Linux"));

        let one = prepare(
            &record(true),
            Some("sddm"),
            machine,
            "1.15.0",
            "theme shown at boot",
        )
        .unwrap();
        assert_eq!(one.claims.len(), 1);
        assert!(one.records().contains("note = \"theme shown at boot\""));
    }

    #[test]
    fn failed_runs_and_virtual_machines_are_not_reported() {
        let machine = machine(&thinkpad(), &platform(), &plasma());
        let refused = prepare(&record(false), None, machine.clone(), "1.15.0", "").unwrap_err();
        assert!(refused.contains("did not succeed"), "{refused}");
        let mut vm = thinkpad();
        vm.commands
            .insert("systemd-detect-virt".into(), "kvm\n".into());
        let virtual_machine = super::machine(&vm, &platform(), &plasma());
        let refused = prepare(&record(true), None, virtual_machine, "1.15.0", "").unwrap_err();
        assert!(refused.contains("(kvm)"), "{refused}");
        let refused = prepare(&record(true), Some("git"), machine, "1.15.0", "").unwrap_err();
        assert!(refused.contains("no entries"), "{refused}");
    }

    #[test]
    fn the_last_successful_run_of_an_entry_is_found() {
        let history = vec![record(false), record(true)];
        assert!(last_success(&history, "sddm").unwrap().success);
        assert!(last_success(&history, "git").is_none());
    }
}
