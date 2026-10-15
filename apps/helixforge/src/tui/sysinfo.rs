//! What the dashboard shows about the machine.

use helixforge_platform::LinuxInfo;
use std::{fs, path::Path, process::Command};

pub struct Overview {
    pub rows: Vec<(&'static str, String)>,
}

impl Overview {
    pub fn gather(platform: &LinuxInfo) -> Self {
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
        if let Some(desktop) = std::env::var_os("XDG_CURRENT_DESKTOP") {
            rows.push(("Desktop", desktop.to_string_lossy().into_owned()));
        }
        if let Some(shell) = std::env::var_os("SHELL") {
            let name = Path::new(&shell)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            rows.push(("Shell", name));
        }
        if let Ok(kernel) = fs::read_to_string("/proc/sys/kernel/osrelease") {
            rows.push(("Kernel", kernel.trim().to_string()));
        }
        if let Some(cpu) = cpu_model() {
            rows.push(("CPU", cpu));
        }
        if let Some(memory) = memory_total() {
            rows.push(("Memory", memory));
        }
        if let Some(disk) = root_disk() {
            rows.push(("Root disk", disk));
        }
        Self { rows }
    }
}

fn cpu_model() -> Option<String> {
    let text = fs::read_to_string("/proc/cpuinfo").ok()?;
    text.lines()
        .find_map(|line| line.strip_prefix("model name"))
        .and_then(|rest| rest.split_once(':'))
        .map(|(_, value)| value.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn memory_total() -> Option<String> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let kib: u64 = text
        .lines()
        .find_map(|line| line.strip_prefix("MemTotal:"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    Some(format!("{:.1} GiB", kib as f64 / 1024.0 / 1024.0))
}

fn root_disk() -> Option<String> {
    let output = Command::new("df")
        .args(["-h", "--output=size,avail", "/"])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut fields = text.lines().nth(1)?.split_whitespace();
    let (size, available) = (fields.next()?, fields.next()?);
    Some(format!("{size}, {available} free"))
}
