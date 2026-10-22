//! Recovery readiness: whether this machine could be brought back after a
//! bad update or a lost disk. It checks for snapshots of the system and
//! whether the boot menu can start one, a second kernel, Helix Toolbox's
//! configuration backups, a backup of the user's own files, and a Drift
//! baseline to rebuild from. It only reads. Snapshots that need root to
//! list are looked at by a plan of its own, which reads them and changes
//! nothing.

use crate::{
    Plan, Step,
    backups::{Backup, Snapshots},
    doctor::{Diagnosis, Health, System},
    drift::Baseline,
};
use helix_toolbox_core::{PackageManager, Risk};
use std::path::Path;

/// What the checks are given besides the machine.
pub struct Known<'a> {
    pub manager: Option<PackageManager>,
    pub backups: &'a [Backup],
    /// The saved Drift baselines, newest first.
    pub baselines: &'a [(String, Baseline)],
    /// Now, in seconds since 1970.
    pub now: u64,
}

/// The snapshots a tool lists without root: how many, and when the newest
/// was taken (`YYYY-MM-DD HH:MM:SS`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Listed {
    pub count: usize,
    pub newest: Option<String>,
}

const DAY: u64 = 86_400;

/// The snapshot tool this machine is set up for.
pub fn snapshot_tool(system: &dyn System) -> Option<Snapshots> {
    Snapshots::detect(&|path| system.exists(Path::new(path)), &|tool| {
        system.on_path(tool)
    })
}

/// The root file system's type, from the mount table.
pub fn root_type(system: &dyn System) -> Option<String> {
    system
        .read(Path::new("/proc/mounts"))?
        .lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace().skip(1);
            let (mount, kind) = (words.next()?, words.next()?);
            (mount == "/").then(|| kind.to_string())
        })
        .next_back()
}

/// Snapper's snapshots of the root, where this user may list them.
pub fn snapper_listed(system: &dyn System) -> Option<Listed> {
    let (true, text) = system.run(&[
        "snapper",
        "--jsonout",
        "-c",
        "root",
        "list",
        "--disable-used-space",
    ])?
    else {
        return None;
    };
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let snapshots: Vec<&serde_json::Value> = value
        .get("root")?
        .as_array()?
        .iter()
        // Number 0 is the running system itself.
        .filter(|snapshot| snapshot.get("number").and_then(|number| number.as_u64()) != Some(0))
        .collect();
    let newest = snapshots
        .iter()
        .filter_map(|snapshot| snapshot.get("date")?.as_str())
        .filter(|date| !date.is_empty())
        .max()
        .map(str::to_string);
    Some(Listed {
        count: snapshots.len(),
        newest,
    })
}

/// Every readiness check, in the order shown.
pub fn examine(system: &dyn System, known: &Known) -> Vec<Diagnosis> {
    let tool = snapshot_tool(system);
    let root = root_type(system);
    let listed = match tool {
        Some(Snapshots::Snapper) => snapper_listed(system),
        _ => None,
    };
    let mut found = vec![snapshots(tool, root.as_deref(), listed.as_ref(), known)];
    if tool.is_some() && root.as_deref() == Some("btrfs") {
        found.push(snapshot_boot(system, known.manager));
    }
    if let Some(kernels) = second_kernel(system, known.manager) {
        found.push(kernels);
    }
    found.push(configuration_backups(known.backups));
    found.push(own_files(system));
    found.push(machine_record(known));
    found
}

fn check(id: &str, title: &str, health: Health, summary: impl Into<String>) -> Diagnosis {
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

/// A date at least `days` before now, as `YYYY-MM-DD`, for comparing with
/// dates written the same way.
fn days_ago(now: u64, days: u64) -> String {
    crate::date(now.saturating_sub(days * DAY))
}

fn snapshots(
    tool: Option<Snapshots>,
    root: Option<&str>,
    listed: Option<&Listed>,
    known: &Known,
) -> Diagnosis {
    let mut evidence = vec![format!(
        "The root file system is {}",
        root.unwrap_or("of an unknown type")
    )];
    let mut found = match (tool, listed) {
        (Some(tool), Some(listed)) if listed.count == 0 => check(
            "snapshots",
            "System snapshots",
            Health::Notice,
            format!(
                "{} is set up, but has no snapshots of the system yet",
                tool.label()
            ),
        ),
        (Some(tool), Some(listed)) => {
            let newest = listed.newest.as_deref().unwrap_or("an unknown time");
            if newest
                .get(..10)
                .is_some_and(|day| day < days_ago(known.now, 30).as_str())
            {
                check(
                    "snapshots",
                    "System snapshots",
                    Health::Notice,
                    format!(
                        "{}'s newest snapshot is from {newest}, over a month ago",
                        tool.label()
                    ),
                )
            } else {
                check(
                    "snapshots",
                    "System snapshots",
                    Health::Good,
                    format!(
                        "{} has {} snapshot{}, the newest from {newest}",
                        tool.label(),
                        listed.count,
                        if listed.count == 1 { "" } else { "s" }
                    ),
                )
            }
        }
        (Some(tool), None) => {
            let mut found = check(
                "snapshots",
                "System snapshots",
                Health::Good,
                format!(
                    "{} is set up; listing its snapshots needs root",
                    tool.label()
                ),
            );
            found.advice = Some(
                "helix-toolbox recovery --check-snapshots lists them and looks inside the newest, reading only."
                    .into(),
            );
            found
        }
        (None, _) => {
            let mut found = check(
                "snapshots",
                "System snapshots",
                Health::Notice,
                if root == Some("btrfs") {
                    "The system is on Btrfs, but nothing takes snapshots of it"
                } else {
                    "Nothing takes snapshots of the system"
                },
            );
            found.advice = Some(if root == Some("btrfs") {
                "Snapper or Timeshift keep snapshots to go back to after an update goes wrong; Helix Toolbox then takes one before each run that changes the system.".into()
            } else {
                "Timeshift's rsync mode keeps snapshots on any file system; Helix Toolbox then takes one before each run that changes the system.".into()
            });
            found.entry = match (known.manager, root) {
                (Some(PackageManager::Arch), Some("btrfs")) => Some("snapper-snapshots".into()),
                (Some(PackageManager::Fedora), Some("btrfs")) => {
                    Some("btrfs-assistant-snapper-config-grub-btrfs".into())
                }
                (Some(PackageManager::Fedora), _) => None,
                _ => Some("timeshift-backup".into()),
            };
            found
        }
    };
    if let Some(tool) = tool {
        evidence.push(format!("Roll back with {}", tool.how_to_restore()));
    }
    found.evidence = evidence;
    found
}

fn snapshot_boot(system: &dyn System, manager: Option<PackageManager>) -> Diagnosis {
    let exists = |path: &str| system.exists(Path::new(path));
    let through = if exists("/etc/grub.d/41_snapshots-btrfs") {
        Some("grub-btrfs")
    } else if system.on_path("limine-snapper-sync") || exists("/etc/limine-snapper-sync.conf") {
        Some("limine-snapper-sync")
    } else if exists("/etc/grub.d/80_suse_btrfs_snapshot")
        || exists("/usr/lib/snapper/plugins/grub")
    {
        Some("openSUSE's GRUB snapshot entries")
    } else {
        None
    };
    match through {
        Some(through) => check(
            "snapshot-boot",
            "Starting from a snapshot",
            Health::Good,
            format!(
                "The boot menu lists snapshots ({through}), so a system that no longer starts can start from one"
            ),
        ),
        None => {
            let mut found = check(
                "snapshot-boot",
                "Starting from a snapshot",
                Health::Notice,
                "The boot menu does not list snapshots, so rolling back a system that no longer starts needs a live USB",
            );
            found.entry = match manager {
                Some(PackageManager::Arch) => Some("snapper-snapshots".into()),
                Some(PackageManager::Fedora) => {
                    Some("btrfs-assistant-snapper-config-grub-btrfs".into())
                }
                _ => None,
            };
            found
        }
    }
}

fn second_kernel(system: &dyn System, manager: Option<PackageManager>) -> Option<Diagnosis> {
    let kernels = crate::doctor::kernels(system, manager);
    let names: Vec<String> = kernels
        .iter()
        .map(|(kernel, _)| kernel.trim_start_matches("/boot/vmlinuz-").to_string())
        .collect();
    let mut found = match names.len() {
        // No kernel in /boot: one kept elsewhere (unified images on the
        // EFI partition), or a container.
        0 => return None,
        1 => {
            let mut found = check(
                "second-kernel",
                "A second kernel",
                Health::Notice,
                format!(
                    "Only one kernel, {}, is installed: if an update breaks it, there is no other to start",
                    names[0]
                ),
            );
            found.advice = Some(match manager {
                Some(PackageManager::Arch) => {
                    "Install linux-lts beside it; the boot menu then offers both.".into()
                }
                _ => "The package manager keeps the previous kernel after a kernel update, so a second appears with the next one.".into(),
            });
            found
        }
        count => check(
            "second-kernel",
            "A second kernel",
            Health::Good,
            format!("{count} kernels can be started: {}", names.join(", ")),
        ),
    };
    found.evidence = kernels
        .iter()
        .map(|(kernel, image)| format!("{kernel} with {image}"))
        .collect();
    Some(found)
}

fn configuration_backups(backups: &[Backup]) -> Diagnosis {
    let mut found = match backups.first() {
        Some(newest) => check(
            "configuration-backups",
            "Configuration backups",
            Health::Good,
            format!(
                "{} backup{} of files Helix Toolbox's entries changed, the newest of {} from {}",
                backups.len(),
                if backups.len() == 1 { "" } else { "s" },
                helix_toolbox_core::printable(&newest.original.display().to_string()),
                newest.taken.as_deref().unwrap_or("an unknown time")
            ),
        ),
        None => check(
            "configuration-backups",
            "Configuration backups",
            Health::Good,
            "None yet: an entry that changes a file keeps the old one beside it",
        ),
    };
    found.advice = Some(
        "helix-toolbox backups lists them; a restore keeps the current file as a backup first, so it can be undone the same way."
            .into(),
    );
    found
}

/// Backup programs for the user's own files, and their Flatpak ids.
const BACKUP_TOOLS: [(&str, &str); 8] = [
    ("borg", "Borg"),
    ("restic", "restic"),
    ("kopia", "Kopia"),
    ("rsnapshot", "rsnapshot"),
    ("duplicity", "duplicity"),
    ("deja-dup", "Déjà Dup"),
    ("pika-backup", "Pika Backup"),
    ("backintime", "Back In Time"),
];

const BACKUP_FLATPAKS: [(&str, &str); 3] = [
    ("org.gnome.DejaDup", "Déjà Dup"),
    ("org.gnome.World.PikaBackup", "Pika Backup"),
    ("com.borgbase.Vorta", "Vorta"),
];

fn own_files(system: &dyn System) -> Diagnosis {
    let (_, _, home) = system.user();
    let mut tools: Vec<&str> = BACKUP_TOOLS
        .iter()
        .filter(|(tool, _)| system.on_path(tool))
        .map(|(_, name)| *name)
        .collect();
    for (id, name) in BACKUP_FLATPAKS {
        if (system.exists(&Path::new("/var/lib/flatpak/app").join(id))
            || system.exists(&home.join(".local/share/flatpak/app").join(id)))
            && !tools.contains(&name)
        {
            tools.push(name);
        }
    }
    if system.on_path("vorta") && !tools.contains(&"Vorta") {
        tools.push("Vorta");
    }
    let home_snapshots = system.exists(Path::new("/etc/snapper/configs/home"));
    if tools.is_empty() {
        let mut found = check(
            "own-files",
            "Your own files",
            Health::Notice,
            if home_snapshots {
                "Snapper keeps snapshots of /home, but on the same disk; nothing copies your files elsewhere"
            } else {
                "Nothing Helix Toolbox knows of backs up your home folder"
            },
        );
        found.advice = Some(
            "System snapshots leave /home out, and every snapshot is on the same disk. A backup program such as Pika Backup, Déjà Dup, Borg or restic, copying to another disk or another machine, keeps your own files when the disk does not.".into(),
        );
        return found;
    }
    let mut found = check(
        "own-files",
        "Your own files",
        Health::Good,
        format!(
            "{} {} installed; Helix Toolbox cannot tell when it last ran or where its copies are",
            tools.join(", "),
            if tools.len() == 1 { "is" } else { "are" }
        ),
    );
    if home_snapshots {
        found
            .evidence
            .push("Snapper also keeps snapshots of /home".into());
    }
    found
}

fn machine_record(known: &Known) -> Diagnosis {
    let mut found = match known.baselines.first() {
        Some((_, newest)) if newest.taken + 90 * DAY < known.now => check(
            "machine-record",
            "A record of this machine",
            Health::Notice,
            format!(
                "The newest Drift baseline, {}, is from {}; what was added since is not in it",
                newest.name,
                crate::date(newest.taken)
            ),
        ),
        Some((_, newest)) => check(
            "machine-record",
            "A record of this machine",
            Health::Good,
            format!(
                "The Drift baseline {}, from {}, says what to put back on a reinstalled or new machine",
                newest.name,
                crate::date(newest.taken)
            ),
        ),
        None => check(
            "machine-record",
            "A record of this machine",
            Health::Notice,
            "No Drift baseline is saved, so nothing says what this machine had",
        ),
    };
    found.advice = Some(
        "helix-toolbox drift save records the applications, configuration files and settings; on a reinstalled machine, helix-toolbox drift compare lists what to put back. Keep a copy of ~/.config/helix-toolbox/baselines off this machine."
            .into(),
    );
    found
}

/// A plan that lists the snapshots and looks inside the newest, as root,
/// reading only: Snapper's newest snapshot is checked to be a Btrfs
/// subvolume that holds a system.
pub fn plan_check(tool: Snapshots) -> Plan {
    let (program, display, requires) = match tool {
        Snapshots::Snapper => (
            [
                "sudo snapper -c root list",
                "newest=$(sudo snapper --csvout -c root list --columns number | tail -n 1)",
                "case $newest in ''|0|number) echo 'Snapper has no snapshots of the system.'; exit 1 ;; esac",
                "sudo btrfs subvolume show \"/.snapshots/$newest/snapshot\"",
                "for path in etc/os-release usr/bin usr/lib/modules; do",
                "  if sudo test -e \"/.snapshots/$newest/snapshot/$path\"; then echo \"Snapshot $newest has /$path\"; else echo \"Snapshot $newest lacks /$path\"; exit 1; fi",
                "done",
                "echo \"Snapshot $newest holds a system to roll back to.\"",
            ]
            .join("\n"),
            "sudo snapper -c root list; sudo snapper --csvout -c root list --columns number; sudo btrfs subvolume show /.snapshots/<newest>/snapshot; sudo test -e /.snapshots/<newest>/snapshot/{etc/os-release,usr/bin,usr/lib/modules}".to_string(),
            vec!["btrfs".to_string(), "snapper".to_string(), "sudo".to_string()],
        ),
        Snapshots::Timeshift => (
            "sudo timeshift --list".to_string(),
            "sudo timeshift --list".to_string(),
            vec!["sudo".to_string(), "timeshift".to_string()],
        ),
    };
    Plan {
        steps: vec![Step::Shell {
            id: "snapshot-check".to_string(),
            name: format!("Look at {}'s snapshots", tool.label()),
            program,
            display,
            requires: requires.clone(),
            changes: Vec::new(),
        }],
        notes: vec!["It only reads: nothing is rolled back, taken or removed.".to_string()],
        requirements: requires,
        privileged: true,
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, path::PathBuf};

    #[derive(Default)]
    struct Fake {
        files: HashMap<String, String>,
        commands: HashMap<String, String>,
        tools: Vec<&'static str>,
    }

    impl Fake {
        fn file(&mut self, path: &str, text: &str) {
            self.files.insert(path.into(), text.into());
        }
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path.to_str()?).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            let path = path.to_string_lossy();
            self.files
                .keys()
                .any(|file| file == path.as_ref() || file.starts_with(&format!("{path}/")))
        }
        fn list(&self, path: &Path) -> Vec<String> {
            let prefix = format!("{}/", path.display());
            let mut names: Vec<String> = self
                .files
                .keys()
                .filter_map(|file| file.strip_prefix(&prefix))
                .filter_map(|rest| rest.split('/').next())
                .map(str::to_string)
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
            (1000, 1000, PathBuf::from("/home/tester"))
        }
        fn on_path(&self, tool: &str) -> bool {
            self.tools.contains(&tool)
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const NOW: u64 = 1_792_000_000; // 2026-10-14

    fn baseline(name: &str, taken: u64) -> (String, Baseline) {
        let mut baseline: Baseline = toml::from_str(&format!("name = \"{name}\"")).unwrap();
        baseline.taken = taken;
        (name.to_string(), baseline)
    }

    fn summaries(found: &[Diagnosis]) -> Vec<(&str, Health, &str)> {
        found
            .iter()
            .map(|diagnosis| {
                (
                    diagnosis.id.as_str(),
                    diagnosis.health,
                    diagnosis.summary.as_str(),
                )
            })
            .collect()
    }

    #[test]
    fn a_ready_machine_has_snapshots_to_boot_a_second_kernel_and_a_record() {
        let mut fake = Fake {
            tools: vec!["snapper", "restic"],
            ..Fake::default()
        };
        fake.file("/etc/snapper/configs/root", "");
        fake.file("/etc/grub.d/41_snapshots-btrfs", "");
        fake.file(
            "/proc/mounts",
            "/dev/nvme0n1p2 / btrfs rw,subvol=/@ 0 0\n/dev/nvme0n1p1 /boot/efi vfat rw 0 0\n",
        );
        fake.file("/usr/lib/modules/7.2.11-1-garuda/pkgbase", "linux-garuda\n");
        fake.file("/usr/lib/modules/7.3.0-2-garuda/pkgbase", "linux-garuda\n");
        fake.file("/usr/lib/modules/6.18.57-2-lts/pkgbase", "linux-lts\n");
        fake.file("/boot/vmlinuz-linux-garuda", "");
        fake.file("/boot/vmlinuz-linux-lts", "");
        fake.commands.insert(
            "snapper --jsonout -c root list --disable-used-space".into(),
            r#"{"root":[{"number":0,"date":""},{"number":41,"date":"2026-10-01 10:00:00"},{"number":42,"date":"2026-10-14 09:30:00"}]}"#.into(),
        );
        let baselines = [baseline("Laptop", NOW - 10 * DAY)];
        let known = Known {
            manager: Some(PackageManager::Arch),
            backups: &[],
            baselines: &baselines,
            now: NOW,
        };
        let found = examine(&fake, &known);
        assert_eq!(
            summaries(&found),
            [
                (
                    "snapshots",
                    Health::Good,
                    "Snapper has 2 snapshots, the newest from 2026-10-14 09:30:00"
                ),
                (
                    "snapshot-boot",
                    Health::Good,
                    "The boot menu lists snapshots (grub-btrfs), so a system that no longer starts can start from one"
                ),
                // Two module folders of one kernel count once.
                (
                    "second-kernel",
                    Health::Good,
                    "2 kernels can be started: linux-lts, linux-garuda"
                ),
                (
                    "configuration-backups",
                    Health::Good,
                    "None yet: an entry that changes a file keeps the old one beside it"
                ),
                (
                    "own-files",
                    Health::Good,
                    "restic is installed; Helix Toolbox cannot tell when it last ran or where its copies are"
                ),
                (
                    "machine-record",
                    Health::Good,
                    "The Drift baseline Laptop, from 2026-10-04, says what to put back on a reinstalled or new machine"
                ),
            ]
        );
    }

    #[test]
    fn what_is_missing_says_what_would_help() {
        let mut fake = Fake::default();
        fake.file("/proc/mounts", "/dev/sda2 / btrfs rw 0 0\n");
        fake.file("/usr/lib/modules/6.17.1-arch1-1/pkgbase", "linux\n");
        fake.file("/boot/vmlinuz-linux", "");
        let baselines = [baseline("Old", NOW - 200 * DAY)];
        let known = Known {
            manager: Some(PackageManager::Arch),
            backups: &[],
            baselines: &baselines,
            now: NOW,
        };
        let found = examine(&fake, &known);
        assert_eq!(
            summaries(&found)
                .iter()
                .map(|(id, health, _)| (*id, *health))
                .collect::<Vec<_>>(),
            [
                ("snapshots", Health::Notice),
                ("second-kernel", Health::Notice),
                ("configuration-backups", Health::Good),
                ("own-files", Health::Notice),
                ("machine-record", Health::Notice),
            ]
        );
        assert_eq!(
            found[0].summary,
            "The system is on Btrfs, but nothing takes snapshots of it"
        );
        assert_eq!(found[0].entry.as_deref(), Some("snapper-snapshots"));
        assert!(found[1].advice.as_deref().unwrap().contains("linux-lts"));
        assert!(found[4].summary.contains("Old, is from 2026-03-28"));

        // Snapper that lists only for root, on Fedora with one kernel so far;
        // ext4 has no snapshot boot check, and a snapshot a month old is stale.
        let mut fake = Fake {
            tools: vec!["snapper"],
            ..Fake::default()
        };
        fake.file("/etc/snapper/configs/root", "");
        fake.file("/proc/mounts", "/dev/vda1 / ext4 rw 0 0\n");
        let known = Known {
            manager: Some(PackageManager::Fedora),
            backups: &[],
            baselines: &[],
            now: NOW,
        };
        let found = examine(&fake, &known);
        assert_eq!(
            found[0].summary,
            "Snapper is set up; listing its snapshots needs root"
        );
        assert!(
            found[0]
                .advice
                .as_deref()
                .unwrap()
                .contains("--check-snapshots")
        );
        assert_ne!(found[1].id, "snapshot-boot");
        assert_ne!(found[1].id, "second-kernel");
        fake.commands.insert(
            "snapper --jsonout -c root list --disable-used-space".into(),
            r#"{"root":[{"number":0,"date":""},{"number":3,"date":"2026-08-01 10:00:00"}]}"#.into(),
        );
        assert_eq!(
            examine(&fake, &known)[0].summary,
            "Snapper's newest snapshot is from 2026-08-01 10:00:00, over a month ago"
        );
    }

    #[test]
    fn the_snapshot_check_only_reads() {
        let planned = plan_check(Snapshots::Snapper);
        assert_eq!(planned.risk, Risk::Low);
        let Step::Shell {
            program, changes, ..
        } = &planned.steps[0]
        else {
            panic!("a shell step");
        };
        assert!(changes.is_empty());
        for line in program.lines() {
            for word in [
                "delete",
                "rollback",
                "create",
                "rm ",
                "undochange",
                "modify",
            ] {
                assert!(!line.contains(word), "{line}");
            }
        }
        assert!(program.contains("btrfs subvolume show"));
        assert_eq!(
            plan_check(Snapshots::Timeshift).steps[0].display(),
            "sudo timeshift --list"
        );
    }
}
