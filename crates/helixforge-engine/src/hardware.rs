//! The hardware and firmware center: read-only facts about the machine's
//! firmware (through fwupd), disks, temperature sensors, batteries and PCI
//! and USB devices, from `/sys` and tools that answer without root. A SMART
//! check, which needs root, and a firmware update are plans of their own,
//! each run only after it is confirmed.

use crate::{Plan, Step, doctor::System, quote};
use helixforge_core::{Risk, printable};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Everything the center shows.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Hardware {
    pub firmware: Firmware,
    pub disks: Vec<Disk>,
    pub sensors: Vec<Sensor>,
    pub batteries: Vec<Battery>,
    pub pci: Vec<Device>,
    pub usb: Vec<Device>,
}

/// What fwupd knows.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Firmware {
    /// Whether fwupd's command is installed.
    pub available: bool,
    pub devices: Vec<FirmwareDevice>,
    /// Updates from the metadata fwupd already has, apart from the devices.
    pub updates: Vec<FirmwareUpdate>,
    /// Why something could not be read.
    pub note: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FirmwareDevice {
    pub id: String,
    pub name: String,
    pub vendor: Option<String>,
    pub version: Option<String>,
    pub updatable: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct FirmwareUpdate {
    pub device_id: String,
    pub device: String,
    pub from: Option<String>,
    pub to: String,
    pub summary: Option<String>,
    pub urgency: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Disk {
    /// The block device's name: nvme0n1, sda.
    pub name: String,
    pub model: Option<String>,
    pub bytes: Option<u64>,
    /// NVMe, SSD or HDD.
    pub kind: String,
    pub firmware: Option<String>,
    pub celsius: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Sensor {
    pub chip: String,
    pub label: String,
    pub celsius: f64,
    pub high: Option<f64>,
    pub critical: Option<f64>,
}

impl Sensor {
    /// At or past its high mark, or within five degrees of critical.
    pub fn hot(&self) -> bool {
        self.high.is_some_and(|high| self.celsius >= high)
            || self
                .critical
                .is_some_and(|critical| self.celsius >= critical - 5.0)
    }
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Battery {
    pub name: String,
    pub status: Option<String>,
    /// Charge now, percent.
    pub capacity: Option<u32>,
    /// What it holds when full against what it held new, percent.
    pub health: Option<u32>,
    pub cycles: Option<u32>,
    pub model: Option<String>,
    pub technology: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Device {
    /// The PCI slot or USB bus and device.
    pub address: String,
    pub class: Option<String>,
    pub name: String,
    /// The PCI driver in use.
    pub driver: Option<String>,
}

/// Read the machine; fwupd answers from the metadata it already has.
pub fn gather(system: &dyn System) -> Hardware {
    Hardware {
        firmware: firmware(system),
        disks: disks(system),
        sensors: sensors(system),
        batteries: batteries(system),
        pci: pci(system),
        usb: usb(system),
    }
}

fn read(system: &dyn System, path: &Path) -> Option<String> {
    system
        .read(path)
        .map(|text| printable(text.trim()))
        .filter(|text| !text.is_empty())
}

fn number(system: &dyn System, path: &Path) -> Option<f64> {
    read(system, path)?.parse().ok()
}

fn firmware(system: &dyn System) -> Firmware {
    let mut firmware = Firmware {
        available: system.on_path("fwupdmgr"),
        ..Firmware::default()
    };
    if !firmware.available {
        firmware.note = Some("fwupd is not installed, so firmware is not listed".to_string());
        return firmware;
    }
    match system.run(&["fwupdmgr", "get-devices", "--json"]) {
        Some((true, text)) => {
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            for device in parsed["Devices"].as_array().into_iter().flatten() {
                let (Some(id), Some(name)) = (device["DeviceId"].as_str(), device["Name"].as_str())
                else {
                    continue;
                };
                let flags: Vec<&str> = device["Flags"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|flag| flag.as_str())
                    .collect();
                firmware.devices.push(FirmwareDevice {
                    id: printable(id),
                    name: printable(name),
                    vendor: device["Vendor"].as_str().map(printable),
                    version: device["Version"].as_str().map(printable),
                    updatable: flags.contains(&"updatable"),
                });
            }
        }
        _ => firmware.note = Some("fwupd's service did not answer".to_string()),
    }
    if let Some((_, text)) = system.run(&["fwupdmgr", "get-updates", "--json"]) {
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
        for device in parsed["Devices"].as_array().into_iter().flatten() {
            let Some(id) = device["DeviceId"].as_str().filter(|id| valid_device_id(id)) else {
                continue;
            };
            let Some(release) = device["Releases"]
                .as_array()
                .and_then(|releases| releases.first())
            else {
                continue;
            };
            let Some(to) = release["Version"].as_str() else {
                continue;
            };
            firmware.updates.push(FirmwareUpdate {
                device_id: id.to_string(),
                device: printable(device["Name"].as_str().unwrap_or(id)),
                from: device["Version"].as_str().map(printable),
                to: printable(to),
                summary: release["Summary"].as_str().map(printable),
                urgency: release["Urgency"].as_str().map(printable),
            });
        }
    }
    firmware
}

/// fwupd's device ids are 40 hexadecimal digits.
pub fn valid_device_id(id: &str) -> bool {
    id.len() == 40 && id.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// A block device name a SMART check may name.
pub fn valid_disk(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 32
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn disks(system: &dyn System) -> Vec<Disk> {
    let block = Path::new("/sys/block");
    let mut found = Vec::new();
    for name in system.list(block) {
        // Partitions are not listed here; loop, RAM and optical devices are skipped.
        if !valid_disk(&name)
            || ["loop", "ram", "zram", "sr", "dm", "md", "fd"]
                .iter()
                .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let device = block.join(&name);
        let nvme = name.starts_with("nvme");
        let controller = name
            .strip_prefix("nvme")
            .and_then(|rest| rest.split('n').next())
            .map(|number| PathBuf::from(format!("/sys/class/nvme/nvme{number}")));
        let model = controller
            .as_ref()
            .and_then(|controller| read(system, &controller.join("model")))
            .or_else(|| read(system, &device.join("device/model")));
        let firmware = controller
            .as_ref()
            .and_then(|controller| read(system, &controller.join("firmware_rev")))
            .or_else(|| read(system, &device.join("device/rev")));
        let rotational = read(system, &device.join("queue/rotational")).as_deref() == Some("1");
        // USB bridges often call any disk rotational; say only that it is on USB.
        let on_usb = system
            .link(&device)
            .is_some_and(|target| target.to_string_lossy().contains("/usb"));
        let bytes = read(system, &device.join("size"))
            .and_then(|sectors| sectors.parse::<u64>().ok())
            .map(|sectors| sectors * 512);
        if bytes == Some(0) {
            continue;
        }
        let celsius = controller.as_ref().and_then(|controller| {
            system
                .list(controller)
                .into_iter()
                .find(|entry| entry.starts_with("hwmon"))
                .and_then(|hwmon| number(system, &controller.join(hwmon).join("temp1_input")))
                .map(|milli| milli / 1000.0)
        });
        found.push(Disk {
            name,
            model,
            bytes,
            kind: if nvme {
                "NVMe".to_string()
            } else if on_usb {
                "USB".to_string()
            } else if rotational {
                "HDD".to_string()
            } else {
                "SSD".to_string()
            },
            firmware,
            celsius,
        });
    }
    found
}

fn sensors(system: &dyn System) -> Vec<Sensor> {
    let root = Path::new("/sys/class/hwmon");
    let mut found = Vec::new();
    for hwmon in system.list(root) {
        let directory = root.join(&hwmon);
        let chip = read(system, &directory.join("name")).unwrap_or_else(|| hwmon.clone());
        let mut inputs: Vec<String> = system
            .list(&directory)
            .into_iter()
            .filter(|name| name.starts_with("temp") && name.ends_with("_input"))
            .collect();
        inputs.sort_by_key(|name| {
            name.trim_start_matches("temp")
                .trim_end_matches("_input")
                .parse::<u32>()
                .unwrap_or(0)
        });
        for input in inputs {
            let stem = input.trim_end_matches("_input");
            // Sensors with nothing attached read zero or less.
            let Some(milli) = number(system, &directory.join(&input)).filter(|milli| *milli > 0.0)
            else {
                continue;
            };
            let degrees = |suffix: &str| {
                number(system, &directory.join(format!("{stem}_{suffix}")))
                    .map(|milli| milli / 1000.0)
                    .filter(|value| *value > 0.0 && *value < 200.0)
            };
            found.push(Sensor {
                chip: chip.clone(),
                label: read(system, &directory.join(format!("{stem}_label")))
                    .unwrap_or_else(|| stem.to_string()),
                celsius: milli / 1000.0,
                high: degrees("max"),
                critical: degrees("crit"),
            });
        }
    }
    found
}

fn batteries(system: &dyn System) -> Vec<Battery> {
    let root = Path::new("/sys/class/power_supply");
    let mut found = Vec::new();
    for name in system.list(root) {
        let directory = root.join(&name);
        if read(system, &directory.join("type")).as_deref() != Some("Battery") {
            continue;
        }
        let value = |file: &str| {
            read(system, &directory.join(file)).and_then(|text| text.parse::<u64>().ok())
        };
        let health = match (value("energy_full"), value("energy_full_design")) {
            (Some(full), Some(design)) if design > 0 => Some(full * 100 / design),
            _ => match (value("charge_full"), value("charge_full_design")) {
                (Some(full), Some(design)) if design > 0 => Some(full * 100 / design),
                _ => None,
            },
        };
        found.push(Battery {
            name,
            status: read(system, &directory.join("status")),
            capacity: value("capacity").map(|value| value as u32),
            health: health.map(|value| value as u32),
            cycles: value("cycle_count")
                .map(|value| value as u32)
                .filter(|cycles| *cycles > 0),
            model: read(system, &directory.join("model_name")),
            technology: read(system, &directory.join("technology")),
        });
    }
    found
}

fn pci(system: &dyn System) -> Vec<Device> {
    let Some((true, text)) = system.run(&["lspci", "-nnk"]) else {
        return Vec::new();
    };
    let mut found: Vec<Device> = Vec::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('\t') {
            if let Some(driver) = rest.strip_prefix("Kernel driver in use: ")
                && let Some(last) = found.last_mut()
            {
                last.driver = Some(printable(driver.trim()));
            }
            continue;
        }
        let Some((address, rest)) = line.split_once(' ') else {
            continue;
        };
        let (class, name) = match rest.split_once(": ") {
            Some((class, name)) => (Some(printable(class)), printable(name)),
            None => (None, printable(rest)),
        };
        found.push(Device {
            address: printable(address),
            class,
            name,
            driver: None,
        });
    }
    found
}

fn usb(system: &dyn System) -> Vec<Device> {
    let Some((true, text)) = system.run(&["lsusb"]) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (bus, rest) = line.split_once(": ID ")?;
            let (id, name) = rest.split_once(' ').unwrap_or((rest, ""));
            // Root hubs are the controllers themselves.
            if name.contains("root hub") {
                return None;
            }
            Some(Device {
                address: printable(
                    bus.trim_start_matches("Bus ")
                        .replace(" Device ", ":")
                        .as_str(),
                ),
                class: Some(printable(id)),
                name: printable(name.trim()),
                driver: None,
            })
        })
        .collect()
}

/// A plan that updates one device's firmware through fwupd. High risk: a
/// failed firmware update can leave the device unusable.
pub fn plan_firmware(update: &FirmwareUpdate) -> Option<Plan> {
    if !valid_device_id(&update.device_id) {
        return None;
    }
    let words = ["sudo", "fwupdmgr", "update", update.device_id.as_str()];
    Some(Plan {
        steps: vec![Step::Shell {
            id: "firmware-update".to_string(),
            name: format!(
                "Update the firmware of {} to {}",
                update.device, update.to
            ),
            program: words.iter().map(|word| quote(word)).collect::<Vec<_>>().join(" "),
            display: words.join(" "),
            requires: vec!["sudo".to_string(), "fwupdmgr".to_string()],
            changes: Vec::new(),
        }],
        notes: vec![
            "Keep the machine on mains power and do not switch it off until fwupd says it has finished; some updates finish at the next reboot.".to_string(),
        ],
        requirements: vec!["fwupdmgr".to_string(), "sudo".to_string()],
        privileged: true,
        risk: Risk::High,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

/// A plan that reads a disk's SMART health and attributes. It only reads,
/// but needs root to reach the disk.
pub fn plan_smart(disk: &str) -> Option<Plan> {
    if !valid_disk(disk) {
        return None;
    }
    let device = format!("/dev/{disk}");
    let words = [
        "sudo",
        "smartctl",
        "--health",
        "--attributes",
        device.as_str(),
    ];
    Some(Plan {
        steps: vec![Step::Shell {
            id: "smart-check".to_string(),
            name: format!("SMART health of {disk}"),
            program: words
                .iter()
                .map(|word| quote(word))
                .collect::<Vec<_>>()
                .join(" "),
            display: words.join(" "),
            requires: vec!["sudo".to_string(), "smartctl".to_string()],
            changes: Vec::new(),
        }],
        notes: vec!["It only reads the disk's own health record.".to_string()],
        requirements: vec!["smartctl".to_string(), "sudo".to_string()],
        privileged: true,
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

/// Bytes as GB or TB, as disks are sold.
pub fn size(bytes: u64) -> String {
    let gb = bytes as f64 / 1e9;
    if gb >= 1000.0 {
        format!("{:.1} TB", gb / 1000.0)
    } else {
        format!("{gb:.0} GB")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        commands: HashMap<String, String>,
        tools: Vec<&'static str>,
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
            self.files.keys().any(|known| known.starts_with(path))
        }
        fn list(&self, path: &Path) -> Vec<String> {
            let mut names: Vec<String> = self
                .files
                .keys()
                .filter_map(|known| known.strip_prefix(path).ok())
                .filter_map(|rest| rest.components().next())
                .map(|part| part.as_os_str().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names.dedup();
            names
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
            (1000, 1000, PathBuf::from("/home/me"))
        }
        fn on_path(&self, tool: &str) -> bool {
            self.tools.contains(&tool)
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const DEVICE: &str = "9ea4da9853eae668f5bd65b10d14e4363f619406";

    fn machine() -> Fake {
        let mut fake = Fake {
            tools: vec!["fwupdmgr"],
            ..Fake::default()
        };
        fake.file("/sys/block/nvme0n1/size", "1953525168\n");
        fake.file("/sys/block/nvme0n1/queue/rotational", "0\n");
        fake.file(
            "/sys/class/nvme/nvme0/model",
            "Samsung SSD 990 PRO 1TB            \n",
        );
        fake.file("/sys/class/nvme/nvme0/firmware_rev", "8B2QJXD7\n");
        fake.file("/sys/class/nvme/nvme0/hwmon3/temp1_input", "38850\n");
        fake.file("/sys/block/loop0/size", "100\n");
        fake.file("/sys/block/sda/size", "3907029168\n");
        fake.file("/sys/block/sda/queue/rotational", "1\n");
        fake.file("/sys/block/sda/device/model", "WDC WD20EZRZ\n");
        fake.file("/sys/class/hwmon/hwmon7/name", "coretemp\n");
        fake.file("/sys/class/hwmon/hwmon7/temp1_label", "Package id 0\n");
        fake.file("/sys/class/hwmon/hwmon7/temp1_input", "97000\n");
        fake.file("/sys/class/hwmon/hwmon7/temp1_max", "100000\n");
        fake.file("/sys/class/hwmon/hwmon7/temp1_crit", "100000\n");
        fake.file("/sys/class/hwmon/hwmon7/temp2_input", "45000\n");
        fake.file("/sys/class/power_supply/AC/type", "Mains\n");
        fake.file("/sys/class/power_supply/BAT0/type", "Battery\n");
        fake.file("/sys/class/power_supply/BAT0/status", "Charging\n");
        fake.file("/sys/class/power_supply/BAT0/capacity", "100\n");
        fake.file("/sys/class/power_supply/BAT0/energy_full", "53950000\n");
        fake.file(
            "/sys/class/power_supply/BAT0/energy_full_design",
            "68000000\n",
        );
        fake.file("/sys/class/power_supply/BAT0/cycle_count", "662\n");
        fake.commands.insert(
            "fwupdmgr get-devices --json".into(),
            format!(r#"{{"Devices": [{{"DeviceId": "{DEVICE}", "Name": "Bolt Receiver", "Version": "MPR05.04", "Vendor": "Logitech", "Flags": ["updatable"]}}, {{"Name": "no id"}}]}}"#),
        );
        fake.commands.insert(
            "fwupdmgr get-updates --json".into(),
            format!(r#"{{"Devices": [{{"DeviceId": "{DEVICE}", "Name": "Bolt Receiver", "Version": "MPR05.04", "Releases": [{{"Version": "MPR05.06", "Summary": "Fixes pairing\u001b[2J", "Urgency": "medium"}}]}}, {{"DeviceId": "../../etc", "Releases": [{{"Version": "1"}}]}}]}}"#),
        );
        fake.commands.insert(
            "lspci -nnk".into(),
            "00:02.0 VGA compatible controller [0300]: Intel Corporation UHD Graphics [8086:9a60]\n\tSubsystem: Lenovo\n\tKernel driver in use: i915\n01:00.0 VGA compatible controller [0300]: NVIDIA Corporation TU117M [10de:1f99]\n".into(),
        );
        fake.commands.insert(
            "lsusb".into(),
            "Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub\nBus 002 Device 003: ID 046d:c548 Logitech, Inc. Logi Bolt Receiver\n".into(),
        );
        fake
    }

    #[test]
    fn the_machine_is_read_without_root() {
        let found = gather(&machine());
        assert_eq!(found.firmware.devices.len(), 1);
        assert!(found.firmware.devices[0].updatable);
        // Only well-formed device ids, and only plain text, come through.
        assert_eq!(found.firmware.updates.len(), 1);
        let update = &found.firmware.updates[0];
        assert_eq!(update.to, "MPR05.06");
        assert!(!update.summary.as_ref().unwrap().contains('\u{1b}'));
        let names: Vec<&str> = found.disks.iter().map(|disk| disk.name.as_str()).collect();
        assert_eq!(names, ["nvme0n1", "sda"]);
        assert_eq!(found.disks[0].kind, "NVMe");
        assert_eq!(
            found.disks[0].model.as_deref(),
            Some("Samsung SSD 990 PRO 1TB")
        );
        assert_eq!(found.disks[0].celsius, Some(38.85));
        assert_eq!(size(found.disks[0].bytes.unwrap()), "1.0 TB");
        assert_eq!(found.disks[1].kind, "HDD");
        assert_eq!(found.sensors.len(), 2);
        assert!(found.sensors[0].hot());
        assert!(!found.sensors[1].hot());
        assert_eq!(found.batteries.len(), 1);
        assert_eq!(found.batteries[0].health, Some(79));
        assert_eq!(found.batteries[0].cycles, Some(662));
        assert_eq!(found.pci.len(), 2);
        assert_eq!(found.pci[0].driver.as_deref(), Some("i915"));
        assert_eq!(found.pci[1].driver, None);
        assert_eq!(found.usb.len(), 1);
        assert_eq!(found.usb[0].name, "Logitech, Inc. Logi Bolt Receiver");
    }

    #[test]
    fn without_fwupd_firmware_says_why() {
        let mut fake = machine();
        fake.tools.clear();
        let found = gather(&fake);
        assert!(!found.firmware.available);
        assert!(found.firmware.note.as_ref().unwrap().contains("fwupd"));
    }

    #[test]
    fn firmware_and_smart_plans_name_only_checked_values() {
        let found = gather(&machine());
        let planned = plan_firmware(&found.firmware.updates[0]).unwrap();
        assert_eq!(planned.risk, Risk::High);
        assert!(planned.privileged);
        assert!(planned.steps[0].display().ends_with(DEVICE));
        let bad = FirmwareUpdate {
            device_id: "x; reboot".into(),
            ..found.firmware.updates[0].clone()
        };
        assert!(plan_firmware(&bad).is_none());
        let smart = plan_smart("nvme0n1").unwrap();
        assert_eq!(
            smart.steps[0].display(),
            "sudo smartctl --health --attributes /dev/nvme0n1"
        );
        assert!(plan_smart("../sda").is_none());
        assert!(plan_smart("sda;x").is_none());
    }
}
