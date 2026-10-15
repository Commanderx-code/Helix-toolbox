//! A few lines about the machine for the sidebar.

use helixforge_platform::LinuxInfo;
use std::{fs, process::Command};

pub fn gather(platform: &LinuxInfo) -> Vec<(&'static str, String)> {
    let mut entries = vec![("OS", platform.pretty_name.clone())];
    if let Some(cpu) = cpu_model() {
        entries.push(("CPU", cpu));
    }
    if let Some(memory) = memory_total() {
        entries.push(("RAM", memory));
    }
    if let Some(disk) = root_disk() {
        entries.push(("DISK", disk));
    }
    entries
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
