//! What an interface's dashboard shows about the machine.

use crate::{doctor::System, host::Host};
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;
use std::path::Path;

/// Labeled facts about the machine, in the order to show them. In JSON, each
/// row is a `[label, value]` pair.
#[derive(Clone, Debug, Serialize)]
pub struct Overview {
    pub rows: Vec<(&'static str, String)>,
}

impl Overview {
    /// Read them from the host; this asks `df` for the root disk.
    pub fn gather(host: &Host, platform: &LinuxInfo) -> Self {
        let mut rows = vec![
            ("Distribution", platform.pretty_name.clone()),
            (
                "Package manager",
                platform
                    .manager
                    .map(|manager| manager.executable().to_string())
                    .unwrap_or_else(|| "none supported".to_string()),
            ),
        ];
        if let Some(desktop) = host.desktop() {
            rows.push(("Desktop", desktop));
        }
        if let Some(shell) = host.variable("SHELL") {
            let name = Path::new(&shell)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            rows.push(("Shell", name));
        }
        if let Some(kernel) = host.read(Path::new("/proc/sys/kernel/osrelease")) {
            rows.push(("Kernel", kernel.trim().to_string()));
        }
        if let Some(cpu) = cpu_model(host) {
            rows.push(("CPU", cpu));
        }
        if let Some(memory) = memory_total(host) {
            rows.push(("Memory", memory));
        }
        if let Some(disk) = root_disk(host) {
            rows.push(("Root disk", disk));
        }
        Self { rows }
    }
}

fn cpu_model(system: &dyn System) -> Option<String> {
    let text = system.read(Path::new("/proc/cpuinfo"))?;
    text.lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, value)| value.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn memory_total(system: &dyn System) -> Option<String> {
    let text = system.read(Path::new("/proc/meminfo"))?;
    let kib: u64 = text
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(format!("{:.1} GiB", kib as f64 / 1024.0 / 1024.0))
}

fn root_disk(system: &dyn System) -> Option<String> {
    let (_, text) = system.run(&["df", "-h", "--output=size,avail", "/"])?;
    let mut fields = text.lines().nth(1)?.split_whitespace();
    let (size, available) = (fields.next()?, fields.next()?);
    Some(format!("{size}, {available} free"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::Fake;
    use helix_toolbox_core::PackageManager;

    #[test]
    fn the_overview_is_read_from_the_host() {
        let mut fake = Fake::default();
        fake.files.insert(
            "/proc/sys/kernel/osrelease".into(),
            "6.0.0-made-up\n".into(),
        );
        fake.files.insert(
            "/proc/cpuinfo".into(),
            "processor\t: 0\nmodel name\t: Made   Up CPU\n".into(),
        );
        fake.files.insert(
            "/proc/meminfo".into(),
            "MemTotal:       16777216 kB\n".into(),
        );
        fake.commands.insert(
            "df -h --output=size,avail /".into(),
            (true, " Size Avail\n 932G  500G\n".into()),
        );
        let host = Host::made_up(fake)
            .with_variable("XDG_CURRENT_DESKTOP", "MadeUpDE")
            .with_variable("SHELL", "/usr/bin/fish");
        let platform = LinuxInfo {
            id: "arch".into(),
            pretty_name: "Arch Linux".into(),
            id_like: vec![],
            manager: Some(PackageManager::Arch),
        };
        let rows: Vec<String> = Overview::gather(&host, &platform)
            .rows
            .iter()
            .map(|(label, value)| format!("{label}: {value}"))
            .collect();
        assert_eq!(
            rows,
            [
                "Distribution: Arch Linux",
                "Package manager: pacman",
                "Desktop: MadeUpDE",
                "Shell: fish",
                "Kernel: 6.0.0-made-up",
                "CPU: Made Up CPU",
                "Memory: 16.0 GiB",
                "Root disk: 932G, 500G free",
            ]
        );
        // A made-up host with nothing to say has no more than the platform.
        let bare = Overview::gather(&Host::made_up(Fake::default()), &platform);
        assert_eq!(bare.rows.len(), 2);
    }
}
