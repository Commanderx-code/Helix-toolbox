//! HELIX Doctor: read-only health checks that say what is wrong, show the
//! evidence, and offer a repair where a safe one exists. A repair is a plan
//! of its own, confirmed and run like any other; nothing here changes the
//! system.

use crate::{Plan, Step, quote};
use helixforge_core::{PackageManager, Risk, printable};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

/// How a check came out.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Health {
    Good,
    /// Worth knowing; not broken.
    Notice,
    Problem,
}

impl Health {
    pub fn label(self) -> &'static str {
        match self {
            Health::Good => "OK",
            Health::Notice => "Notice",
            Health::Problem => "Problem",
        }
    }
}

/// What one check found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnosis {
    /// Stable, for `helixforge doctor --repair <id>`.
    pub id: String,
    pub title: String,
    pub health: Health,
    pub summary: String,
    /// What was looked at, line by line.
    pub evidence: Vec<String>,
    /// What to do when there is no repair HelixForge can run.
    pub advice: Option<String>,
    /// A repair HelixForge can run, after its own confirmation.
    pub repair: Option<Repair>,
    /// A catalog entry that addresses it.
    pub entry: Option<String>,
}

/// A repair: a few commands, each built from quoted words.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repair {
    pub label: String,
    pub commands: Vec<Vec<String>>,
}

impl Repair {
    fn new(label: impl Into<String>, commands: Vec<Vec<&str>>) -> Self {
        Repair {
            label: label.into(),
            commands: commands
                .into_iter()
                .map(|command| command.into_iter().map(String::from).collect())
                .collect(),
        }
    }

    /// The repair as a plan of one step.
    pub fn plan(&self, id: &str) -> Plan {
        let program = self
            .commands
            .iter()
            .map(|command| {
                command
                    .iter()
                    .map(|word| quote(word))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join(" &&\n");
        // Shown in plans; the program keeps the real words, quoted.
        let display = printable(
            &self
                .commands
                .iter()
                .map(|command| command.join(" "))
                .collect::<Vec<_>>()
                .join(" && "),
        );
        let mut requires: Vec<String> = self
            .commands
            .iter()
            .filter_map(|command| command.first().cloned())
            .collect();
        requires.sort();
        requires.dedup();
        let privileged = requires.iter().any(|tool| tool == "sudo");
        Plan {
            steps: vec![Step::Shell {
                id: format!("repair-{id}"),
                name: format!("Repair: {}", self.label),
                program,
                display,
                requires: requires.clone(),
            }],
            requirements: requires,
            privileged,
            risk: if privileged { Risk::Medium } else { Risk::Low },
            depends: vec![Vec::new()],
            ..Plan::default()
        }
    }
}

/// What the checks read from the machine; tests give a made-up one.
pub trait System {
    fn read(&self, path: &Path) -> Option<String>;
    fn exists(&self, path: &Path) -> bool;
    fn list(&self, path: &Path) -> Vec<String>;
    fn link(&self, path: &Path) -> Option<PathBuf>;
    /// A command's success and standard output; None when it cannot run.
    fn run(&self, command: &[&str]) -> Option<(bool, String)>;
    /// The addresses a host name resolves to, within a few seconds.
    fn resolve(&self, host: &str) -> Result<Vec<String>, String>;
    /// Free and total bytes of the file system holding a path.
    fn space(&self, path: &Path) -> Option<(u64, u64)>;
    /// The owner's user id.
    fn owner(&self, path: &Path) -> Option<u32>;
    /// Whether the path itself is a symbolic link.
    fn is_link(&self, path: &Path) -> bool;
    /// The path with every link resolved.
    fn canonical(&self, path: &Path) -> Option<PathBuf>;
    fn user(&self) -> (u32, u32, PathBuf);
    fn on_path(&self, tool: &str) -> bool;
    /// Names of running processes.
    fn processes(&self) -> Vec<String>;
}

/// This machine.
pub struct Here;

impl System for Here {
    fn read(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list(&self, path: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(path)
            .map(|listing| {
                listing
                    .flatten()
                    .map(|item| item.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    fn link(&self, path: &Path) -> Option<PathBuf> {
        std::fs::read_link(path).ok()
    }

    fn run(&self, command: &[&str]) -> Option<(bool, String)> {
        let output = Command::new(command[0])
            .args(&command[1..])
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        Some((
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
        ))
    }

    fn resolve(&self, host: &str) -> Result<Vec<String>, String> {
        use std::net::ToSocketAddrs;
        let host = format!("{host}:443");
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let result = host
                .to_socket_addrs()
                .map(|addresses| addresses.map(|address| address.ip().to_string()).collect())
                .map_err(|error| error.to_string());
            let _ = sender.send(result);
        });
        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap_or_else(|_| Err("no answer within 5 seconds".to_string()))
    }

    fn space(&self, path: &Path) -> Option<(u64, u64)> {
        let stats = nix::sys::statvfs::statvfs(path).ok()?;
        let unit = stats.fragment_size() as u64;
        Some((
            stats.blocks_available() as u64 * unit,
            stats.blocks() as u64 * unit,
        ))
    }

    fn owner(&self, path: &Path) -> Option<u32> {
        use std::os::unix::fs::MetadataExt;
        std::fs::symlink_metadata(path)
            .ok()
            .map(|metadata| metadata.uid())
    }

    fn is_link(&self, path: &Path) -> bool {
        std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink())
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        std::fs::canonicalize(path).ok()
    }

    fn user(&self) -> (u32, u32, PathBuf) {
        (
            nix::unistd::getuid().as_raw(),
            nix::unistd::getgid().as_raw(),
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
        )
    }

    fn on_path(&self, tool: &str) -> bool {
        helixforge_platform::binary_exists(tool)
    }

    fn processes(&self) -> Vec<String> {
        self.list(Path::new("/proc"))
            .into_iter()
            .filter(|name| name.chars().all(|character| character.is_ascii_digit()))
            .filter_map(|pid| self.read(&Path::new("/proc").join(pid).join("comm")))
            .map(|name| name.trim().to_string())
            .collect()
    }
}

/// A host every Linux machine with working DNS can resolve.
const PROBE_HOST: &str = "github.com";

/// Run every check.
/// What is shown is made printable: journal lines, unit and file names and
/// error text come from outside HelixForge.
pub fn examine(system: &dyn System, manager: Option<PackageManager>) -> Vec<Diagnosis> {
    let mut found = gather(system, manager);
    for diagnosis in &mut found {
        diagnosis.title = printable(&diagnosis.title);
        diagnosis.summary = printable(&diagnosis.summary);
        diagnosis.evidence = diagnosis
            .evidence
            .iter()
            .map(|line| printable(line))
            .collect();
        diagnosis.advice = diagnosis.advice.as_deref().map(printable);
        if let Some(repair) = &mut diagnosis.repair {
            repair.label = printable(&repair.label);
        }
    }
    found
}

fn gather(system: &dyn System, manager: Option<PackageManager>) -> Vec<Diagnosis> {
    let mut found = Vec::new();
    let online = network(system, &mut found);
    dns(system, online, &mut found);
    services(system, &mut found);
    graphics(system, manager, &mut found);
    packages(system, manager, &mut found);
    permissions(system, &mut found);
    boot(system, manager, &mut found);
    space(system, manager, &mut found);
    found
}

fn diagnosis(id: &str, title: &str, health: Health, summary: impl Into<String>) -> Diagnosis {
    Diagnosis {
        id: id.to_string(),
        title: title.to_string(),
        health,
        summary: summary.into(),
        evidence: Vec::new(),
        advice: None,
        repair: None,
        entry: None,
    }
}

fn network(system: &dyn System, into: &mut Vec<Diagnosis>) -> bool {
    let routes = system
        .read(Path::new("/proc/net/route"))
        .unwrap_or_default();
    let default: Vec<&str> = routes
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut columns = line.split_whitespace();
            let interface = columns.next()?;
            (columns.next()? == "00000000").then_some(interface)
        })
        .collect();
    let mut found = if default.is_empty() {
        let mut found = diagnosis(
            "network",
            "Network",
            Health::Problem,
            "Not connected: there is no default route",
        );
        found.advice = Some(
            "Connect to a network from the desktop's network menu or with nmcli, then check again."
                .into(),
        );
        found
    } else {
        diagnosis(
            "network",
            "Network",
            Health::Good,
            format!("Connected through {}", default.join(", ")),
        )
    };
    found.evidence.push(format!(
        "default routes: {}",
        if default.is_empty() {
            "none".into()
        } else {
            default.join(", ")
        }
    ));
    into.push(found);
    !default.is_empty()
}

fn dns(system: &dyn System, online: bool, into: &mut Vec<Diagnosis>) {
    let servers: Vec<String> = system
        .read(Path::new("/etc/resolv.conf"))
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("nameserver"))
        .map(|server| server.trim().to_string())
        .collect();
    if !online {
        into.push(diagnosis(
            "dns",
            "Name resolution (DNS)",
            Health::Notice,
            "Not checked: no network",
        ));
        return;
    }
    let mut found = match system.resolve(PROBE_HOST) {
        Ok(addresses) => {
            let mut found = diagnosis(
                "dns",
                "Name resolution (DNS)",
                Health::Good,
                format!("{PROBE_HOST} resolves"),
            );
            found
                .evidence
                .push(format!("{PROBE_HOST}: {}", addresses.join(", ")));
            found
        }
        Err(error) => {
            let mut found = diagnosis(
                "dns",
                "Name resolution (DNS)",
                Health::Problem,
                format!("{PROBE_HOST} does not resolve, though the network is up"),
            );
            found.evidence.push(format!("{PROBE_HOST}: {error}"));
            let active = |unit: &str| {
                system
                    .run(&["systemctl", "is-active", unit])
                    .is_some_and(|(ok, _)| ok)
            };
            if active("systemd-resolved") {
                found.repair = Some(Repair::new(
                    "restart systemd-resolved",
                    vec![vec!["sudo", "systemctl", "restart", "systemd-resolved"]],
                ));
            } else if active("NetworkManager") {
                found.repair = Some(Repair::new(
                    "restart NetworkManager",
                    vec![vec!["sudo", "systemctl", "restart", "NetworkManager"]],
                ));
            } else {
                found.advice = Some("Check the name servers in /etc/resolv.conf, or set them in your network settings.".into());
            }
            found
        }
    };
    found.evidence.push(format!(
        "name servers: {}",
        if servers.is_empty() {
            "none in /etc/resolv.conf".into()
        } else {
            servers.join(", ")
        }
    ));
    into.push(found);
}

fn services(system: &dyn System, into: &mut Vec<Diagnosis>) {
    let mut failed = 0;
    for (user, scope) in [(false, "system"), (true, "user")] {
        let mut command = vec!["systemctl"];
        if user {
            command.push("--user");
        }
        command.extend(["--failed", "--no-legend", "--plain"]);
        let Some((_, listing)) = system.run(&command) else {
            continue;
        };
        for (unit, load) in listing.lines().filter_map(|line| {
            let mut columns = line.split_whitespace();
            Some((columns.next()?, columns.next().unwrap_or("")))
        }) {
            if !unit.contains('.') || unit.contains(['/', '\'']) {
                continue;
            }
            failed += 1;
            let id = format!("{}service:{unit}", if user { "user-" } else { "" });
            let systemctl: Vec<&str> = if user {
                vec!["systemctl", "--user"]
            } else {
                vec!["sudo", "systemctl"]
            };
            // Its unit file is gone (the package was removed): nothing can
            // restart it, and systemd only remembers that it failed.
            if load == "not-found" {
                let mut found = diagnosis(
                    &id,
                    &format!("Service {unit}"),
                    Health::Notice,
                    format!(
                        "The {scope} service {unit} was removed, but its last failure is still remembered"
                    ),
                );
                found.evidence.push(format!(
                    "systemctl lists {unit} as failed with no unit file (not-found)"
                ));
                let mut forget = systemctl.clone();
                forget.extend(["reset-failed", unit]);
                found.repair = Some(Repair::new(
                    format!("forget {unit}'s failure"),
                    vec![forget],
                ));
                into.push(found);
                continue;
            }
            let mut found = diagnosis(
                &id,
                &format!("Service {unit}"),
                Health::Problem,
                format!("The {scope} service {unit} failed"),
            );
            let mut journal = vec!["journalctl", "-n", "5", "--no-pager", "-q", "-o", "cat"];
            journal.extend(if user {
                ["--user-unit", unit]
            } else {
                ["-u", unit]
            });
            if let Some((_, lines)) = system.run(&journal) {
                found
                    .evidence
                    .extend(lines.lines().map(|line| format!("journal: {line}")));
            }
            let mut restart = systemctl;
            restart.extend(["restart", unit]);
            found.repair = Some(Repair::new(format!("restart {unit}"), vec![restart]));
            into.push(found);
        }
    }
    if failed == 0 {
        into.push(diagnosis(
            "services",
            "Services",
            Health::Good,
            "No failed services",
        ));
    }
}

fn graphics(system: &dyn System, manager: Option<PackageManager>, into: &mut Vec<Diagnosis>) {
    let devices = Path::new("/sys/bus/pci/devices");
    for slot in system.list(devices) {
        let device = devices.join(&slot);
        let class = system.read(&device.join("class")).unwrap_or_default();
        if !class.trim().starts_with("0x03") {
            continue;
        }
        let vendor = match system
            .read(&device.join("vendor"))
            .unwrap_or_default()
            .trim()
        {
            "0x10de" => "NVIDIA",
            "0x1002" => "AMD",
            "0x8086" => "Intel",
            _ => "Other",
        };
        let driver = system.link(&device.join("driver")).and_then(|link| {
            link.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        let id = format!("graphics:{slot}");
        let title = format!("{vendor} graphics ({slot})");
        let mut found = match driver.as_deref() {
            None => {
                let mut found = diagnosis(
                    &id,
                    &title,
                    Health::Problem,
                    "No driver is in use for this graphics card",
                );
                found.entry = Some(
                    if vendor == "NVIDIA" && manager == Some(PackageManager::Arch) {
                        "nvidia-drivers-hardware-acceleration".to_string()
                    } else {
                        "general-gpu-drivers".to_string()
                    },
                );
                found
            }
            Some("nouveau") => {
                let mut found = diagnosis(
                    &id,
                    &title,
                    Health::Notice,
                    "The open nouveau driver is in use; games and CUDA need NVIDIA's driver",
                );
                found.entry = Some(if manager == Some(PackageManager::Arch) {
                    "nvidia-drivers-hardware-acceleration".to_string()
                } else {
                    "general-gpu-drivers".to_string()
                });
                found
            }
            Some(driver) => diagnosis(
                &id,
                &title,
                Health::Good,
                format!("The {driver} driver is in use"),
            ),
        };
        found.evidence.push(format!(
            "{slot}: vendor {vendor}, driver {}",
            driver.as_deref().unwrap_or("none")
        ));
        into.push(found);
    }
}

fn packages(system: &dyn System, manager: Option<PackageManager>, into: &mut Vec<Diagnosis>) {
    match manager {
        Some(PackageManager::Arch) => {
            let lock = Path::new("/var/lib/pacman/db.lck");
            if !system.exists(lock) {
                into.push(diagnosis(
                    "package-lock",
                    "Package database",
                    Health::Good,
                    "pacman's database is not locked",
                ));
                return;
            }
            let running: Vec<String> = system
                .processes()
                .into_iter()
                .filter(|name| {
                    matches!(
                        name.as_str(),
                        "pacman" | "paru" | "yay" | "pamac" | "packagekitd"
                    )
                })
                .collect();
            if running.is_empty() {
                let mut found = diagnosis(
                    "package-lock",
                    "Package database",
                    Health::Problem,
                    "pacman's database is locked, but no package manager is running",
                );
                found.evidence.push(format!(
                    "{} exists; no pacman, paru, yay, pamac or packagekitd process",
                    lock.display()
                ));
                found.repair = Some(Repair::new(
                    "remove the stale pacman lock",
                    vec![vec!["sudo", "rm", "-f", "/var/lib/pacman/db.lck"]],
                ));
                into.push(found);
            } else {
                let mut found = diagnosis(
                    "package-lock",
                    "Package database",
                    Health::Notice,
                    "pacman's database is locked while a package manager runs",
                );
                found
                    .evidence
                    .push(format!("running: {}", running.join(", ")));
                found.advice = Some("Let it finish; the lock goes when it does.".into());
                into.push(found);
            }
        }
        Some(PackageManager::Debian) => match system.run(&["dpkg", "--audit"]) {
            Some((_, report)) if !report.trim().is_empty() => {
                let mut found = diagnosis(
                    "dpkg-audit",
                    "Package database",
                    Health::Problem,
                    "dpkg reports packages that are not fully installed",
                );
                found
                    .evidence
                    .extend(report.lines().take(8).map(String::from));
                found.repair = Some(Repair::new(
                    "finish configuring packages",
                    vec![vec!["sudo", "dpkg", "--configure", "-a"]],
                ));
                into.push(found);
            }
            _ => into.push(diagnosis(
                "dpkg-audit",
                "Package database",
                Health::Good,
                "dpkg reports nothing unfinished",
            )),
        },
        _ => {}
    }
}

fn permissions(system: &dyn System, into: &mut Vec<Diagnosis>) {
    let (uid, gid, home) = system.user();
    if uid == 0 || home.as_os_str().is_empty() {
        return;
    }
    let Some(real_home) = system.canonical(&home) else {
        return;
    };
    // Where a path in the home really is, if no link anywhere on its way
    // leads out of where it appears; None otherwise.
    let inside = |path: &Path| -> Option<PathBuf> {
        let expected = real_home.join(path.strip_prefix(&home).ok()?);
        (system.canonical(path)? == expected).then_some(expected)
    };
    let mut wrong: Vec<PathBuf> = Vec::new();
    // Links are never handed to chown; they are only mentioned.
    let mut links: Vec<PathBuf> = Vec::new();
    if system.owner(&real_home).is_some_and(|owner| owner != uid) {
        wrong.push(real_home.clone());
    }
    for folder in [
        home.clone(),
        home.join(".config"),
        home.join(".local/share"),
        home.join(".local/bin"),
        home.join(".cache"),
    ] {
        // A folder that is a link, or reached through one, is not looked in.
        if folder != home && (system.is_link(&folder) || inside(&folder).is_none()) {
            continue;
        }
        for name in system.list(&folder) {
            let path = folder.join(&name);
            if folder == home && !name.starts_with('.') {
                continue;
            }
            if system.is_link(&path) {
                if system.owner(&path) == Some(0) {
                    links.push(path);
                }
                continue;
            }
            if system.owner(&path) != Some(0) {
                continue;
            }
            match inside(&path) {
                Some(real) if !wrong.contains(&real) => wrong.push(real),
                Some(_) => {}
                None => links.push(path),
            }
        }
    }
    let left_alone = |path: &PathBuf| {
        format!(
            "owned by root through a link, left alone: {}",
            path.display()
        )
    };
    if wrong.is_empty() {
        let mut found = diagnosis(
            "permissions",
            "Home folder permissions",
            Health::Good,
            "Your home folder and settings are yours",
        );
        found.evidence.extend(links.iter().map(left_alone));
        into.push(found);
        return;
    }
    let mut found = diagnosis(
        "permissions",
        "Home folder permissions",
        Health::Problem,
        if wrong.len() == 1 {
            "1 item in your home belongs to root".to_string()
        } else {
            format!("{} items in your home belong to root", wrong.len())
        },
    );
    found.evidence.extend(
        wrong
            .iter()
            .map(|path| format!("owned by root: {}", path.display())),
    );
    found.advice = Some(
        "Programs running as you cannot change these. This usually comes from running a program with sudo."
            .into(),
    );
    found.evidence.extend(links.iter().map(left_alone));
    let owner = format!("{uid}:{gid}");
    // Just before the change, every path must still be where it was found,
    // with no link on its way; otherwise nothing changes.
    let mut guard: Vec<String> = vec![
        "sh".into(),
        "-c".into(),
        "for path; do [ \"$(realpath -e -- \"$path\")\" = \"$path\" ] || { echo \"HelixForge: $path moved or is reached through a link now; nothing was changed\" >&2; exit 1; }; done".into(),
        "sh".into(),
    ];
    guard.extend(wrong.iter().map(|path| path.to_string_lossy().into_owned()));
    // -h changes a link itself and -P never follows one while recursing, so
    // nothing outside these paths changes, even if one becomes a link later.
    let mut command: Vec<String> = vec![
        "sudo".into(),
        "chown".into(),
        "-R".into(),
        "-h".into(),
        "-P".into(),
        owner,
        "--".into(),
    ];
    command.extend(wrong.iter().map(|path| path.to_string_lossy().into_owned()));
    found.repair = Some(Repair {
        label: "give them back to you".into(),
        commands: vec![guard, command],
    });
    into.push(found);
}

fn boot(system: &dyn System, manager: Option<PackageManager>, into: &mut Vec<Diagnosis>) {
    let modules = if system.exists(Path::new("/usr/lib/modules")) {
        Path::new("/usr/lib/modules")
    } else {
        Path::new("/lib/modules")
    };
    let mut missing: Vec<String> = Vec::new();
    let mut evidence = Vec::new();
    for version in system.list(modules) {
        // Arch names its images after the package base; the others after the version.
        let (kernel, image) = match system.read(&modules.join(&version).join("pkgbase")) {
            Some(base) => {
                let base = base.trim().to_string();
                (
                    format!("/boot/vmlinuz-{base}"),
                    format!("/boot/initramfs-{base}.img"),
                )
            }
            None if manager == Some(PackageManager::Debian) => (
                format!("/boot/vmlinuz-{version}"),
                format!("/boot/initrd.img-{version}"),
            ),
            None => (
                format!("/boot/vmlinuz-{version}"),
                format!("/boot/initramfs-{version}.img"),
            ),
        };
        if !system.exists(Path::new(&kernel)) {
            // Modules left behind by a removed kernel, or a kernel kept elsewhere.
            continue;
        }
        if system.exists(Path::new(&image)) {
            evidence.push(format!("{kernel}: {image} present"));
        } else {
            evidence.push(format!("{kernel}: {image} MISSING"));
            missing.push(image);
        }
    }
    let regenerate: Option<Vec<&str>> = if system.on_path("mkinitcpio") {
        Some(vec!["sudo", "mkinitcpio", "-P"])
    } else if system.on_path("dracut-rebuild") {
        Some(vec!["sudo", "dracut-rebuild"])
    } else if system.on_path("update-initramfs") {
        Some(vec!["sudo", "update-initramfs", "-u", "-k", "all"])
    } else if system.on_path("dracut") {
        Some(vec!["sudo", "dracut", "--regenerate-all", "--force"])
    } else {
        None
    };
    let mut found = if missing.is_empty() {
        diagnosis(
            "initramfs",
            "Boot images",
            Health::Good,
            "Every installed kernel has its initramfs",
        )
    } else {
        let mut found = diagnosis(
            "initramfs",
            "Boot images",
            Health::Problem,
            format!(
                "{} kernel(s) have no initramfs and would not boot",
                missing.len()
            ),
        );
        match regenerate {
            Some(command) => {
                found.repair = Some(Repair::new("rebuild the initramfs images", vec![command]))
            }
            None => {
                found.advice = Some("Rebuild them with your distribution's initramfs tool.".into())
            }
        }
        found
    };
    found.evidence = evidence;
    into.push(found);
    // GRUB: a configuration without a generated menu.
    if system.exists(Path::new("/etc/default/grub")) {
        let menu = ["/boot/grub/grub.cfg", "/boot/grub2/grub.cfg"]
            .into_iter()
            .find(|path| system.exists(Path::new(path)));
        let mut found = match menu {
            Some(menu) => {
                let mut found = diagnosis(
                    "grub-menu",
                    "GRUB menu",
                    Health::Good,
                    "GRUB's menu is generated",
                );
                found.evidence.push(format!("{menu} present"));
                found
            }
            None => {
                let mut found = diagnosis(
                    "grub-menu",
                    "GRUB menu",
                    Health::Problem,
                    "/etc/default/grub exists, but GRUB has no generated menu",
                );
                let output = if system.exists(Path::new("/boot/grub2")) {
                    "/boot/grub2/grub.cfg"
                } else {
                    "/boot/grub/grub.cfg"
                };
                let tool = if system.on_path("grub2-mkconfig") {
                    "grub2-mkconfig"
                } else {
                    "grub-mkconfig"
                };
                found.repair = Some(Repair::new(
                    "generate GRUB's menu",
                    vec![vec!["sudo", tool, "-o", output]],
                ));
                found
            }
        };
        found.evidence.push("/etc/default/grub present".into());
        into.push(found);
    }
    // The EFI system partition, small on many installs.
    for esp in ["/boot/efi", "/efi"] {
        let path = Path::new(esp);
        if !system.exists(path) {
            continue;
        }
        let Some((free, total)) = system.space(path) else {
            continue;
        };
        if total == 0 {
            continue;
        }
        let mib = |bytes: u64| bytes >> 20;
        let mut found = if free < (16 << 20) || free * 10 < total {
            let mut found = diagnosis(
                "esp-space",
                "EFI partition",
                Health::Notice,
                format!("{esp} has {} MiB free of {} MiB", mib(free), mib(total)),
            );
            found.advice = Some(
                "Boot-loader updates write here; remove old boot entries or kernels if it fills."
                    .into(),
            );
            found
        } else {
            diagnosis(
                "esp-space",
                "EFI partition",
                Health::Good,
                format!("{esp} has {} MiB free of {} MiB", mib(free), mib(total)),
            )
        };
        found.evidence.push(format!(
            "{esp}: {} MiB free, {} MiB total",
            mib(free),
            mib(total)
        ));
        into.push(found);
        break;
    }
}

fn space(system: &dyn System, manager: Option<PackageManager>, into: &mut Vec<Diagnosis>) {
    let Some((free, total)) = system.space(Path::new("/")) else {
        return;
    };
    if total == 0 {
        return;
    }
    let gib = |bytes: u64| bytes as f64 / f64::from(1u32 << 30);
    let summary = format!("{:.1} GiB free of {:.1} GiB on /", gib(free), gib(total));
    let mut found = if free < (2 << 30) || free * 20 < total {
        let mut found = diagnosis("disk-space", "Disk space", Health::Problem, summary);
        found.repair = match manager {
            Some(PackageManager::Arch) if system.on_path("paccache") => Some(Repair::new(
                "keep two versions of each cached package and trim the journal",
                vec![
                    vec!["sudo", "paccache", "-rk2"],
                    vec!["sudo", "journalctl", "--vacuum-size=200M"],
                ],
            )),
            Some(PackageManager::Debian) => Some(Repair::new(
                "clear apt's package cache",
                vec![vec!["sudo", "apt-get", "clean"]],
            )),
            Some(PackageManager::Fedora) => Some(Repair::new(
                "clear dnf's package cache",
                vec![vec!["sudo", "dnf", "clean", "packages"]],
            )),
            _ => None,
        };
        found
    } else if free * 10 < total {
        diagnosis("disk-space", "Disk space", Health::Notice, summary)
    } else {
        diagnosis("disk-space", "Disk space", Health::Good, summary)
    };
    found.evidence.push(format!(
        "/: {:.1} GiB free, {:.1} GiB total",
        gib(free),
        gib(total)
    ));
    into.push(found);
}

/// A made-up machine, for tests.
#[derive(Default)]
pub struct Fake {
    pub files: HashMap<PathBuf, String>,
    pub folders: HashMap<PathBuf, Vec<String>>,
    pub links: HashMap<PathBuf, PathBuf>,
    pub commands: HashMap<String, (bool, String)>,
    pub dns: Option<Result<Vec<String>, String>>,
    pub spaces: HashMap<PathBuf, (u64, u64)>,
    pub owners: HashMap<PathBuf, u32>,
    /// Where a path really is, when a link on the way leads elsewhere.
    pub canonical: HashMap<PathBuf, PathBuf>,
    pub tools: Vec<String>,
    pub running: Vec<String>,
    pub home: PathBuf,
}

impl System for Fake {
    fn read(&self, path: &Path) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path) || self.folders.contains_key(path)
    }

    fn list(&self, path: &Path) -> Vec<String> {
        self.folders.get(path).cloned().unwrap_or_default()
    }

    fn link(&self, path: &Path) -> Option<PathBuf> {
        self.links.get(path).cloned()
    }

    fn run(&self, command: &[&str]) -> Option<(bool, String)> {
        self.commands.get(&command.join(" ")).cloned()
    }

    fn resolve(&self, _: &str) -> Result<Vec<String>, String> {
        self.dns
            .clone()
            .unwrap_or_else(|| Ok(vec!["192.0.2.1".into()]))
    }

    fn space(&self, path: &Path) -> Option<(u64, u64)> {
        self.spaces.get(path).copied()
    }

    fn owner(&self, path: &Path) -> Option<u32> {
        Some(self.owners.get(path).copied().unwrap_or(1000))
    }

    fn is_link(&self, path: &Path) -> bool {
        self.links.contains_key(path)
    }

    fn canonical(&self, path: &Path) -> Option<PathBuf> {
        Some(
            self.canonical
                .get(path)
                .cloned()
                .unwrap_or_else(|| path.to_path_buf()),
        )
    }

    fn user(&self) -> (u32, u32, PathBuf) {
        (1000, 1000, self.home.clone())
    }

    fn on_path(&self, tool: &str) -> bool {
        self.tools.iter().any(|known| known == tool)
    }

    fn processes(&self) -> Vec<String> {
        self.running.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ROUTE: &str = "Iface\tDestination\tGateway\nwlan0\t00000000\t0101A8C0\n";

    fn healthy() -> Fake {
        let mut fake = Fake {
            home: PathBuf::from("/home/me"),
            ..Fake::default()
        };
        fake.files.insert("/proc/net/route".into(), ROUTE.into());
        fake.files
            .insert("/etc/resolv.conf".into(), "nameserver 192.0.2.53\n".into());
        fake.commands.insert(
            "systemctl --failed --no-legend --plain".into(),
            (true, String::new()),
        );
        fake.commands.insert(
            "systemctl --user --failed --no-legend --plain".into(),
            (true, String::new()),
        );
        fake.folders
            .insert("/sys/bus/pci/devices".into(), vec!["0000:01:00.0".into()]);
        fake.files.insert(
            "/sys/bus/pci/devices/0000:01:00.0/class".into(),
            "0x030000\n".into(),
        );
        fake.files.insert(
            "/sys/bus/pci/devices/0000:01:00.0/vendor".into(),
            "0x10de\n".into(),
        );
        fake.links.insert(
            "/sys/bus/pci/devices/0000:01:00.0/driver".into(),
            "../../../bus/pci/drivers/nvidia".into(),
        );
        fake.folders
            .insert("/usr/lib/modules".into(), vec!["7.2.10-1-garuda".into()]);
        fake.files.insert(
            "/usr/lib/modules/7.2.10-1-garuda/pkgbase".into(),
            "linux-garuda\n".into(),
        );
        fake.files
            .insert("/boot/vmlinuz-linux-garuda".into(), String::new());
        fake.files
            .insert("/boot/initramfs-linux-garuda.img".into(), String::new());
        fake.files.insert("/etc/default/grub".into(), String::new());
        fake.files
            .insert("/boot/grub/grub.cfg".into(), String::new());
        fake.folders.insert("/boot/efi".into(), Vec::new());
        fake.spaces
            .insert("/boot/efi".into(), (290 << 20, 300 << 20));
        fake.spaces.insert("/".into(), (500 << 30, 900 << 30));
        fake.folders
            .insert("/home/me".into(), vec![".config".into(), "Music".into()]);
        fake.tools.push("dracut-rebuild".into());
        fake
    }

    fn find<'a>(found: &'a [Diagnosis], id: &str) -> &'a Diagnosis {
        found
            .iter()
            .find(|diagnosis| diagnosis.id == id)
            .unwrap_or_else(|| panic!("no {id} in {found:?}"))
    }

    #[test]
    fn a_healthy_machine_is_all_good() {
        let found = examine(&healthy(), Some(PackageManager::Arch));
        let problems: Vec<&Diagnosis> = found
            .iter()
            .filter(|diagnosis| diagnosis.health != Health::Good)
            .collect();
        assert!(problems.is_empty(), "{problems:?}");
        let ids: Vec<&str> = found
            .iter()
            .map(|diagnosis| diagnosis.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "network",
                "dns",
                "services",
                "graphics:0000:01:00.0",
                "package-lock",
                "permissions",
                "initramfs",
                "grub-menu",
                "esp-space",
                "disk-space"
            ]
        );
    }

    #[test]
    fn network_and_dns_problems_say_what_to_do() {
        let mut offline = healthy();
        offline
            .files
            .insert("/proc/net/route".into(), "Iface\tDestination\n".into());
        let found = examine(&offline, Some(PackageManager::Arch));
        assert_eq!(find(&found, "network").health, Health::Problem);
        assert!(find(&found, "network").advice.is_some());
        assert_eq!(find(&found, "dns").summary, "Not checked: no network");
        let mut broken = healthy();
        broken.dns = Some(Err("failed to lookup address information".into()));
        broken.commands.insert(
            "systemctl is-active systemd-resolved".into(),
            (false, "inactive\n".into()),
        );
        broken.commands.insert(
            "systemctl is-active NetworkManager".into(),
            (true, "active\n".into()),
        );
        let found = examine(&broken, Some(PackageManager::Arch));
        let dns = find(&found, "dns");
        assert_eq!(dns.health, Health::Problem);
        assert!(dns.evidence.iter().any(|line| line.contains("192.0.2.53")));
        assert_eq!(
            dns.repair.as_ref().unwrap().plan("dns").steps[0].display(),
            "sudo systemctl restart NetworkManager"
        );
    }

    #[test]
    fn failed_services_show_their_journal_and_restart_separately() {
        let mut fake = healthy();
        fake.commands.insert(
            "systemctl --failed --no-legend --plain".into(),
            (true, "cups.service loaded failed failed CUPS\n".into()),
        );
        fake.commands.insert(
            "systemctl --user --failed --no-legend --plain".into(),
            (
                true,
                "syncthing.service loaded failed failed Syncthing\n".into(),
            ),
        );
        fake.commands.insert(
            "journalctl -n 5 --no-pager -q -o cat -u cups.service".into(),
            (true, "cupsd: unable to bind\n".into()),
        );
        let found = examine(&fake, Some(PackageManager::Arch));
        let cups = find(&found, "service:cups.service");
        assert_eq!(cups.evidence, ["journal: cupsd: unable to bind"]);
        assert_eq!(
            cups.repair.as_ref().unwrap().plan("x").steps[0].display(),
            "sudo systemctl restart cups.service"
        );
        let user = find(&found, "user-service:syncthing.service");
        let plan = user.repair.as_ref().unwrap().plan("x");
        assert_eq!(
            plan.steps[0].display(),
            "systemctl --user restart syncthing.service"
        );
        assert!(!plan.privileged);
        assert!(found.iter().all(|diagnosis| diagnosis.id != "services"));
    }

    #[test]
    fn a_removed_service_is_forgotten_not_restarted() {
        let mut fake = healthy();
        fake.commands.insert(
            "systemctl --failed --no-legend --plain".into(),
            (
                true,
                "autorandr.service not-found failed failed autorandr.service\n".into(),
            ),
        );
        fake.commands.insert(
            "systemctl --user --failed --no-legend --plain".into(),
            (
                true,
                "old.service not-found failed failed old.service\n".into(),
            ),
        );
        let found = examine(&fake, Some(PackageManager::Arch));
        let gone = find(&found, "service:autorandr.service");
        assert_eq!(gone.health, Health::Notice);
        assert!(gone.summary.contains("was removed"), "{}", gone.summary);
        let repair = gone.repair.as_ref().unwrap();
        assert_eq!(repair.label, "forget autorandr.service's failure");
        assert_eq!(
            repair.plan("x").steps[0].display(),
            "sudo systemctl reset-failed autorandr.service"
        );
        let user = find(&found, "user-service:old.service");
        assert_eq!(
            user.repair.as_ref().unwrap().plan("x").steps[0].display(),
            "systemctl --user reset-failed old.service"
        );
    }

    #[test]
    fn text_from_outside_cannot_drive_the_terminal() {
        let mut fake = healthy();
        fake.commands.insert(
            "systemctl --failed --no-legend --plain".into(),
            (true, "x.service loaded failed failed X\n".into()),
        );
        fake.commands.insert(
            "journalctl -n 5 --no-pager -q -o cat -u x.service".into(),
            (true, "\u{1b}]52;c;bWFsaWNl\u{7}\u{1b}[2Jgone\n".into()),
        );
        fake.dns = Some(Err("bad\u{1b}[31m".into()));
        let found = examine(&fake, Some(PackageManager::Arch));
        for diagnosis in &found {
            for text in diagnosis
                .evidence
                .iter()
                .chain([&diagnosis.summary, &diagnosis.title])
            {
                assert!(!text.chars().any(char::is_control), "{text:?}");
            }
        }
        assert_eq!(
            find(&found, "service:x.service").evidence,
            ["journal: ?]52;c;bWFsaWNl??[2Jgone"]
        );
    }

    #[test]
    fn graphics_without_a_driver_point_to_an_entry() {
        let mut fake = healthy();
        fake.links.clear();
        let found = examine(&fake, Some(PackageManager::Arch));
        let card = find(&found, "graphics:0000:01:00.0");
        assert_eq!(card.health, Health::Problem);
        assert_eq!(
            card.entry.as_deref(),
            Some("nvidia-drivers-hardware-acceleration")
        );
        fake.links.insert(
            "/sys/bus/pci/devices/0000:01:00.0/driver".into(),
            "../nouveau".into(),
        );
        let found = examine(&fake, Some(PackageManager::Fedora));
        assert_eq!(find(&found, "graphics:0000:01:00.0").health, Health::Notice);
        assert_eq!(
            find(&found, "graphics:0000:01:00.0").entry.as_deref(),
            Some("general-gpu-drivers")
        );
    }

    #[test]
    fn a_stale_pacman_lock_is_removed_only_when_nothing_runs() {
        let mut fake = healthy();
        fake.files
            .insert("/var/lib/pacman/db.lck".into(), String::new());
        let found = examine(&fake, Some(PackageManager::Arch));
        let lock = find(&found, "package-lock");
        assert_eq!(lock.health, Health::Problem);
        assert_eq!(
            lock.repair.as_ref().unwrap().plan("x").steps[0].display(),
            "sudo rm -f /var/lib/pacman/db.lck"
        );
        fake.running = vec!["paru".into()];
        let found = examine(&fake, Some(PackageManager::Arch));
        let lock = find(&found, "package-lock");
        assert_eq!(lock.health, Health::Notice);
        assert!(lock.repair.is_none());
        let mut debian = healthy();
        debian.commands.insert(
            "dpkg --audit".into(),
            (
                true,
                "The following packages are only half configured:\n x\n".into(),
            ),
        );
        let found = examine(&debian, Some(PackageManager::Debian));
        assert_eq!(
            find(&found, "dpkg-audit")
                .repair
                .as_ref()
                .unwrap()
                .plan("x")
                .steps[0]
                .display(),
            "sudo dpkg --configure -a"
        );
    }

    #[test]
    fn root_owned_settings_are_given_back() {
        let mut fake = healthy();
        fake.folders.insert(
            "/home/me/.local/share".into(),
            vec!["Shelly".into(), "fonts".into()],
        );
        fake.owners.insert("/home/me/.local/share/Shelly".into(), 0);
        let found = examine(&fake, Some(PackageManager::Arch));
        let permissions = find(&found, "permissions");
        assert_eq!(permissions.health, Health::Problem);
        assert_eq!(
            permissions.evidence,
            ["owned by root: /home/me/.local/share/Shelly"]
        );
        let commands = &permissions.repair.as_ref().unwrap().commands;
        assert_eq!(commands[0][..2], ["sh", "-c"], "the guard comes first");
        assert_eq!(commands[0][4..], ["/home/me/.local/share/Shelly"]);
        assert_eq!(
            commands[1].join(" "),
            "sudo chown -R -h -P 1000:1000 -- /home/me/.local/share/Shelly"
        );
        // A link owned by root is never handed to chown, only mentioned.
        fake.folders
            .insert("/home/me/.config".into(), vec!["evil".into()]);
        fake.owners.insert("/home/me/.config/evil".into(), 0);
        fake.links
            .insert("/home/me/.config/evil".into(), "/etc/shadow".into());
        let found = examine(&fake, Some(PackageManager::Arch));
        let permissions = find(&found, "permissions");
        let commands = &permissions.repair.as_ref().unwrap().commands;
        assert!(
            !commands.concat().iter().any(|word| word.contains("evil")),
            "{commands:?}"
        );
        assert!(
            permissions
                .evidence
                .iter()
                .any(|line| line
                    == "owned by root through a link, left alone: /home/me/.config/evil")
        );
        fake.folders.remove(Path::new("/home/me/.config"));
        // A path with a quote in it stays one quoted word in the program.
        fake.folders
            .insert("/home/me/.config".into(), vec!["it's".into()]);
        fake.owners.insert("/home/me/.config/it's".into(), 0);
        let found = examine(&fake, Some(PackageManager::Arch));
        let Step::Shell { program, .. } = &find(&found, "permissions")
            .repair
            .as_ref()
            .unwrap()
            .plan("x")
            .steps[0]
        else {
            panic!("expected a shell step");
        };
        assert!(program.contains("'/home/me/.config/it'\\''s'"), "{program}");
    }

    #[test]
    fn folders_and_items_reached_through_links_are_left_alone() {
        // A folder that is a link is not looked in.
        let mut linked = healthy();
        linked
            .folders
            .insert("/home/me/.local/share".into(), vec!["dbus-1".into()]);
        linked
            .owners
            .insert("/home/me/.local/share/dbus-1".into(), 0);
        linked
            .links
            .insert("/home/me/.local/share".into(), "/usr/share".into());
        let check =
            |fake: &Fake| find(&examine(fake, Some(PackageManager::Arch)), "permissions").clone();
        assert_eq!(check(&linked).health, Health::Good);
        // Nor one reached through a link further up.
        linked.links.clear();
        linked
            .canonical
            .insert("/home/me/.local/share".into(), "/usr/share".into());
        assert_eq!(check(&linked).health, Health::Good);
        // An item that resolves elsewhere is only mentioned.
        linked.canonical.clear();
        linked.canonical.insert(
            "/home/me/.local/share/dbus-1".into(),
            "/usr/share/dbus-1".into(),
        );
        let permissions = check(&linked);
        assert_eq!(permissions.health, Health::Good);
        assert_eq!(
            permissions.evidence,
            ["owned by root through a link, left alone: /home/me/.local/share/dbus-1"]
        );
    }

    #[test]
    fn the_guard_stops_the_repair_when_a_path_is_reached_through_a_link() {
        let root = tempfile::tempdir().unwrap();
        let base = root.path().canonicalize().unwrap();
        let real = base.join("real");
        std::fs::create_dir(&real).unwrap();
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        // The repair's own guard, as permissions() builds it.
        let mut fake = healthy();
        fake.folders
            .insert("/home/me/.config".into(), vec!["x".into()]);
        fake.owners.insert("/home/me/.config/x".into(), 0);
        let found = examine(&fake, Some(PackageManager::Arch));
        let guard = find(&found, "permissions")
            .repair
            .as_ref()
            .unwrap()
            .commands[0][..4]
            .to_vec();
        let passes = |path: &Path| {
            let mut words = guard.clone();
            words.push(path.to_string_lossy().into_owned());
            let repair = Repair {
                label: "test".into(),
                commands: vec![words],
            };
            let Step::Shell { program, .. } = &repair.plan("x").steps[0] else {
                panic!("expected a shell step");
            };
            Command::new("sh")
                .arg("-c")
                .arg(program)
                .status()
                .unwrap()
                .success()
        };
        assert!(passes(&real));
        assert!(!passes(&link), "a link fails the guard");
        assert!(
            !passes(&link.join("..").join("real")),
            "so does a path through one"
        );
        assert!(!passes(&base.join("gone")), "and a path that is gone");
    }
    #[test]
    fn boot_problems_offer_the_matching_rebuild() {
        let mut fake = healthy();
        fake.files
            .remove(Path::new("/boot/initramfs-linux-garuda.img"));
        fake.files.remove(Path::new("/boot/grub/grub.cfg"));
        fake.spaces.insert("/boot/efi".into(), (8 << 20, 300 << 20));
        let found = examine(&fake, Some(PackageManager::Arch));
        let images = find(&found, "initramfs");
        assert_eq!(images.health, Health::Problem);
        assert_eq!(
            images.repair.as_ref().unwrap().plan("x").steps[0].display(),
            "sudo dracut-rebuild"
        );
        assert_eq!(
            find(&found, "grub-menu")
                .repair
                .as_ref()
                .unwrap()
                .plan("x")
                .steps[0]
                .display(),
            "sudo grub-mkconfig -o /boot/grub/grub.cfg"
        );
        assert_eq!(find(&found, "esp-space").health, Health::Notice);
        // Debian names its images after the kernel version.
        let mut debian = healthy();
        debian
            .folders
            .insert("/usr/lib/modules".into(), vec!["6.12.0-1-amd64".into()]);
        debian
            .files
            .insert("/boot/vmlinuz-6.12.0-1-amd64".into(), String::new());
        debian.tools = vec!["update-initramfs".into()];
        let found = examine(&debian, Some(PackageManager::Debian));
        assert_eq!(
            find(&found, "initramfs")
                .repair
                .as_ref()
                .unwrap()
                .plan("x")
                .steps[0]
                .display(),
            "sudo update-initramfs -u -k all"
        );
    }

    #[test]
    fn a_full_disk_offers_cache_cleaning() {
        let mut fake = healthy();
        fake.spaces.insert("/".into(), (1 << 30, 100 << 30));
        fake.tools.push("paccache".into());
        let found = examine(&fake, Some(PackageManager::Arch));
        let disk = find(&found, "disk-space");
        assert_eq!(disk.health, Health::Problem);
        assert_eq!(
            disk.repair.as_ref().unwrap().plan("x").steps[0].display(),
            "sudo paccache -rk2 && sudo journalctl --vacuum-size=200M"
        );
        fake.spaces.insert("/".into(), (8 << 30, 100 << 30));
        assert_eq!(
            find(&examine(&fake, Some(PackageManager::Arch)), "disk-space").health,
            Health::Notice
        );
    }
}
