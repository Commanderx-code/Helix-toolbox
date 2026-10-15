//! Configuration backups in one place: the copies HelixForge's scripts leave
//! beside a file before changing it (`<file>.commander-backup-<time>`,
//! `<file>.helixforge-backup-<time>`), found in the user's configuration and
//! in the system places those scripts change, and restored in a way that can
//! itself be undone. Snapshots (Snapper, Timeshift) are taken before system
//! changes where the machine supports them; HelixForge does not roll
//! snapshots or packages back itself.

use crate::{Plan, Step, quote};
use helixforge_core::Risk;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// One backup copy and the file it was taken from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Backup {
    pub backup: PathBuf,
    pub original: PathBuf,
    /// When it was taken, `YYYY-MM-DD HH:MM:SS`, from its name.
    pub taken: Option<String>,
    /// Restoring it needs root.
    pub system: bool,
    pub directory: bool,
}

impl Backup {
    /// The time it was taken as one sortable number.
    fn stamp(&self) -> String {
        self.taken.clone().unwrap_or_default()
    }
}

/// The original's name and the time stamp, for a backup's file name.
pub fn parse_name(name: &str) -> Option<(&str, &str)> {
    for marker in [".commander-backup-", ".helixforge-backup-"] {
        if let Some((original, stamp)) = name.rsplit_once(marker)
            && !original.is_empty()
            && stamp.len() >= 14
            && stamp
                .chars()
                .all(|character| character.is_ascii_digit() || character == '-')
        {
            return Some((original, stamp));
        }
    }
    None
}

/// `20261015164340...` as `2026-10-15 16:43:40`.
fn readable(stamp: &str) -> Option<String> {
    let digits: String = stamp.chars().take_while(char::is_ascii_digit).collect();
    if digits.len() < 14 {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{}:{}",
        &digits[0..4],
        &digits[4..6],
        &digits[6..8],
        &digits[8..10],
        &digits[10..12],
        &digits[12..14]
    ))
}

/// Where backups are looked for: the user's configuration (any depth, as
/// the dotfiles installers leave them) and the system places HelixForge's
/// scripts change (shallow).
pub struct Roots {
    /// Searched to any depth, skipping caches and repositories.
    pub user: Vec<PathBuf>,
    /// The home directory itself, one level.
    pub home: Option<PathBuf>,
    /// Searched a few levels deep; restoring these needs root.
    pub system: Vec<(PathBuf, usize)>,
}

impl Roots {
    /// This user's and this machine's places.
    pub fn here() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let xdg = |name: &str, fallback: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
                .or_else(|| home.as_ref().map(|home| home.join(fallback)))
        };
        Roots {
            user: [
                xdg("XDG_CONFIG_HOME", ".config"),
                xdg("XDG_DATA_HOME", ".local/share"),
                home.as_ref().map(|home| home.join(".local/bin")),
            ]
            .into_iter()
            .flatten()
            .collect(),
            home: home.clone(),
            system: vec![
                (PathBuf::from("/etc"), 3),
                (PathBuf::from("/boot"), 3),
                (PathBuf::from("/usr/share/sddm/themes"), 1),
                (PathBuf::from("/usr/share/fonts"), 2),
            ],
        }
    }
}

/// Directories not worth searching for configuration backups.
const SKIP: [&str; 8] = [
    ".git",
    "node_modules",
    "Trash",
    "containers",
    "Steam",
    "flatpak",
    "helixforge",
    "sources",
];

/// Every backup under the roots, newest first.
pub fn discover(roots: &Roots) -> Vec<Backup> {
    let mut found = Vec::new();
    if let Some(home) = &roots.home {
        walk(home, 1, false, &mut found);
    }
    for root in &roots.user {
        walk(root, usize::MAX, false, &mut found);
    }
    for (root, depth) in &roots.system {
        walk(root, *depth, true, &mut found);
    }
    found.sort_by_key(|backup| std::cmp::Reverse(backup.stamp()));
    found.dedup_by(|a, b| a.backup == b.backup);
    found
}

fn walk(directory: &Path, depth: usize, system: bool, into: &mut Vec<Backup>) {
    if depth == 0 {
        return;
    }
    let Ok(listing) = fs::read_dir(directory) else {
        return;
    };
    for item in listing.flatten() {
        let path = item.path();
        // Links are never followed, nor offered: they belong to another owner.
        let Ok(kind) = item.file_type() else { continue };
        if kind.is_symlink() {
            continue;
        }
        let name = item.file_name().to_string_lossy().into_owned();
        if let Some((original, stamp)) = parse_name(&name) {
            into.push(Backup {
                original: directory.join(original),
                taken: readable(stamp),
                system,
                directory: kind.is_dir(),
                backup: path,
            });
            continue;
        }
        if kind.is_dir() && !SKIP.contains(&name.as_str()) {
            walk(&path, depth - 1, system, into);
        }
    }
}

/// The step that puts a backup back: the current file, if any, is moved
/// aside as a backup of its own first, so the restore can be undone too.
pub fn restore_step(backup: &Backup) -> Step {
    let sudo = if backup.system { "sudo " } else { "" };
    let original = quote(&backup.original.to_string_lossy());
    let copy = quote(&backup.backup.to_string_lossy());
    let program = format!(
        "hf_stamp=$(date +%Y%m%d%H%M%S)\n\
         if [ -e {original} ] || [ -L {original} ]; then\n  \
         {sudo}mv -- {original} {original}.helixforge-backup-\"$hf_stamp\" || exit 1\n  \
         printf 'HelixForge: the current one is kept as %s\\n' {original}.helixforge-backup-\"$hf_stamp\"\n\
         fi\n\
         {sudo}cp -a -- {copy} {original} || exit 1\n\
         printf 'HelixForge: restored %s\\n' {original}"
    );
    Step::Shell {
        id: "restore".to_string(),
        name: format!("Restore {}", backup.original.display()),
        program,
        display: format!(
            "{sudo}cp -a {} {}  (the current one is kept as a backup first)",
            backup.backup.display(),
            backup.original.display()
        ),
        requires: if backup.system {
            vec!["sudo".to_string()]
        } else {
            Vec::new()
        },
    }
}

/// A plan that restores one backup.
pub fn plan_restore(backup: &Backup) -> Plan {
    let step = restore_step(backup);
    Plan {
        requirements: step_requirements(&step),
        privileged: backup.system,
        risk: if backup.system {
            Risk::Medium
        } else {
            Risk::Low
        },
        steps: vec![step],
        depends: vec![Vec::new()],
        ..Plan::default()
    }
}

fn step_requirements(step: &Step) -> Vec<String> {
    match step {
        Step::Shell { requires, .. } => requires.clone(),
        _ => Vec::new(),
    }
}

/// A snapshot tool the machine is set up for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Snapshots {
    /// Snapper with a configuration for the root file system.
    Snapper,
    Timeshift,
}

impl Snapshots {
    /// The tool this machine has set up, if any.
    pub fn here() -> Option<Self> {
        Self::detect(
            &|path| Path::new(path).exists(),
            &helixforge_platform::binary_exists,
        )
    }

    /// `here` with the questions answered, for tests.
    pub fn detect(exists: &dyn Fn(&str) -> bool, on_path: &dyn Fn(&str) -> bool) -> Option<Self> {
        if on_path("snapper") && exists("/etc/snapper/configs/root") {
            Some(Snapshots::Snapper)
        } else if on_path("timeshift") && exists("/etc/timeshift/timeshift.json") {
            Some(Snapshots::Timeshift)
        } else {
            None
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Snapshots::Snapper => "Snapper",
            Snapshots::Timeshift => "Timeshift",
        }
    }

    /// Where its snapshots are browsed and rolled back.
    pub fn how_to_restore(self) -> &'static str {
        match self {
            Snapshots::Snapper => {
                "Btrfs Assistant, or snapper list and snapper rollback, from a terminal or the boot menu's snapshots"
            }
            Snapshots::Timeshift => "Timeshift, or timeshift --restore from a terminal",
        }
    }

    /// The step that takes a snapshot, described as `description`.
    pub fn step(self, description: &str) -> Step {
        let words: Vec<String> = match self {
            Snapshots::Snapper => vec![
                "sudo".into(),
                "snapper".into(),
                "-c".into(),
                "root".into(),
                "create".into(),
                "--cleanup-algorithm".into(),
                "number".into(),
                "--description".into(),
                description.into(),
            ],
            Snapshots::Timeshift => vec![
                "sudo".into(),
                "timeshift".into(),
                "--create".into(),
                "--comments".into(),
                description.into(),
            ],
        };
        let program = words
            .iter()
            .map(|word| quote(word))
            .collect::<Vec<_>>()
            .join(" ");
        Step::Shell {
            id: "snapshot".to_string(),
            name: format!("Take a {} snapshot", self.label()),
            program,
            display: words.join(" "),
            requires: vec![self.tool().to_string(), "sudo".to_string()],
        }
    }

    fn tool(self) -> &'static str {
        match self {
            Snapshots::Snapper => "snapper",
            Snapshots::Timeshift => "timeshift",
        }
    }
}

/// Whether a plan changes the system enough to snapshot first: risk high, or
/// a step whose entry changes privileged files, the kernel or disks.
pub fn worth_a_snapshot(plan: &Plan, flags: &dyn Fn(&str) -> Option<String>) -> bool {
    plan.risk == Risk::High
        || plan.steps.iter().any(|step| {
            flags(step.id()).is_some_and(|flags| {
                flags
                    .split_whitespace()
                    .any(|flag| matches!(flag, "PFM" | "K" | "D" | "SI"))
            })
        })
}

/// Put a snapshot step first, taken before the plan's other steps run.
pub fn with_snapshot(plan: &mut Plan, tool: Snapshots) {
    let names: Vec<&str> = plan.steps.iter().map(Step::name).collect();
    let description = format!("HelixForge, before: {}", names.join(", "));
    let step = tool.step(&description);
    plan.requirements.extend(step_requirements(&step));
    plan.requirements.sort();
    plan.requirements.dedup();
    plan.privileged = true;
    plan.steps.insert(0, step);
    // Every index moves up by one; nothing depends on the snapshot.
    let mut depends = vec![Vec::new()];
    depends.extend(
        plan.depends
            .iter()
            .map(|list| list.iter().map(|index| index + 1).collect()),
    );
    plan.depends = depends;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell_program;
    use std::process::Command;

    #[test]
    fn backup_names_give_the_original_and_the_time() {
        assert_eq!(
            parse_name("grub.commander-backup-20261015164340-4242"),
            Some(("grub", "20261015164340-4242"))
        );
        assert_eq!(
            parse_name("pacman.conf.helixforge-backup-20261015164340"),
            Some(("pacman.conf", "20261015164340"))
        );
        assert_eq!(
            parse_name("example.commander-backup-20261015142214095844"),
            Some(("example", "20261015142214095844"))
        );
        assert_eq!(parse_name("grub.backup"), None);
        assert_eq!(parse_name(".commander-backup-20261015164340"), None);
        assert_eq!(parse_name("x.commander-backup-2026"), None);
        assert_eq!(
            readable("20261015164340-1").as_deref(),
            Some("2026-10-15 16:43:40")
        );
    }

    #[test]
    fn backups_are_found_newest_first_and_links_are_left_alone() {
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let config = home.join(".config");
        let etc = root.path().join("etc");
        let write = |path: &Path| {
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "x").unwrap();
        };
        write(&config.join("ghostty/config.commander-backup-20261001000000-1"));
        write(&config.join("deep/a/b/c/d/e.helixforge-backup-20261003000000"));
        fs::create_dir_all(config.join("nvim.commander-backup-20261002000000-7/lua")).unwrap();
        write(&home.join(".bashrc.commander-backup-20260901000000-1"));
        write(&config.join("node_modules/x.commander-backup-20261005000000-1"));
        write(&etc.join("default/grub.commander-backup-20261004000000-9"));
        write(&etc.join("a/b/c/d/too-deep.commander-backup-20261006000000-1"));
        std::os::unix::fs::symlink(
            etc.join("default/grub.commander-backup-20261004000000-9"),
            config.join("linked.commander-backup-20261007000000-1"),
        )
        .unwrap();
        let roots = Roots {
            user: vec![config.clone()],
            home: Some(home.clone()),
            system: vec![(etc.clone(), 3)],
        };
        let found = discover(&roots);
        let shown: Vec<(String, bool, bool)> = found
            .iter()
            .map(|backup| {
                (
                    backup
                        .original
                        .strip_prefix(root.path())
                        .unwrap()
                        .display()
                        .to_string(),
                    backup.system,
                    backup.directory,
                )
            })
            .collect();
        assert_eq!(
            shown,
            [
                ("etc/default/grub".to_string(), true, false),
                ("home/.config/deep/a/b/c/d/e".to_string(), false, false),
                ("home/.config/nvim".to_string(), false, true),
                ("home/.config/ghostty/config".to_string(), false, false),
                ("home/.bashrc".to_string(), false, false),
            ]
        );
        assert_eq!(found[0].taken.as_deref(), Some("2026-10-04 00:00:00"));
    }

    #[test]
    fn a_restore_keeps_the_current_one_and_can_be_undone() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("config");
        let backup = root.path().join("config.commander-backup-20261001000000-1");
        fs::write(&original, "current").unwrap();
        fs::write(&backup, "older").unwrap();
        let found = Backup {
            backup: backup.clone(),
            original: original.clone(),
            taken: readable("20261001000000"),
            system: false,
            directory: false,
        };
        let plan = plan_restore(&found);
        assert_eq!(plan.risk, Risk::Low);
        assert!(!plan.privileged);
        assert!(plan.steps[0].display().starts_with("cp -a "));
        let run = |plan: &Plan| {
            let output = Command::new("sh")
                .arg("-c")
                .arg(shell_program(plan, Path::new("/lib"), Path::new("/hf")))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        };
        run(&plan);
        assert_eq!(fs::read_to_string(&original).unwrap(), "older");
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            "older",
            "the backup stays"
        );
        let kept: Vec<PathBuf> = fs::read_dir(root.path())
            .unwrap()
            .flatten()
            .map(|item| item.path())
            .filter(|path| path.to_string_lossy().contains(".helixforge-backup-"))
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(fs::read_to_string(&kept[0]).unwrap(), "current");
        // A folder, with nothing in its place now.
        let folder = root.path().join("nvim");
        let saved = root.path().join("nvim.commander-backup-20261001000000-1");
        fs::create_dir_all(saved.join("lua")).unwrap();
        fs::write(saved.join("lua/init.lua"), "old").unwrap();
        let plan = plan_restore(&Backup {
            backup: saved,
            original: folder.clone(),
            taken: None,
            system: false,
            directory: true,
        });
        run(&plan);
        assert_eq!(
            fs::read_to_string(folder.join("lua/init.lua")).unwrap(),
            "old"
        );
        // A system file needs sudo.
        let system = plan_restore(&Backup {
            backup: PathBuf::from("/etc/default/grub.commander-backup-20261001000000-1"),
            original: PathBuf::from("/etc/default/grub"),
            taken: None,
            system: true,
            directory: false,
        });
        assert!(system.privileged);
        assert_eq!(system.requirements, ["sudo"]);
        assert!(system.steps[0].display().starts_with("sudo cp -a "));
    }

    #[test]
    fn snapshots_go_first_when_the_system_changes() {
        let snapper = |path: &str| path == "/etc/snapper/configs/root";
        assert_eq!(
            Snapshots::detect(&snapper, &|_| true),
            Some(Snapshots::Snapper)
        );
        assert_eq!(Snapshots::detect(&|_| false, &|_| true), None);
        assert_eq!(
            Snapshots::detect(
                &|path| path == "/etc/timeshift/timeshift.json",
                &|tool| tool == "timeshift"
            ),
            Some(Snapshots::Timeshift)
        );
        let mut plan = Plan {
            steps: vec![
                Step::Command {
                    id: "grub".into(),
                    name: "GRUB Theme".into(),
                    line: "true".into(),
                },
                Step::Command {
                    id: "after".into(),
                    name: "After".into(),
                    line: "true".into(),
                },
            ],
            depends: vec![vec![], vec![0]],
            risk: Risk::Medium,
            ..Plan::default()
        };
        let flags = |id: &str| (id == "grub").then(|| "I PFM MP".to_string());
        assert!(worth_a_snapshot(&plan, &flags));
        assert!(!worth_a_snapshot(&plan, &|_| Some("I".to_string())));
        with_snapshot(&mut plan, Snapshots::Snapper);
        assert_eq!(plan.steps[0].name(), "Take a Snapper snapshot");
        assert_eq!(
            plan.steps[0].display(),
            "sudo snapper -c root create --cleanup-algorithm number --description HelixForge, before: GRUB Theme, After"
        );
        assert_eq!(plan.depends, [vec![], vec![], vec![1]]);
        assert!(plan.privileged);
        assert!(plan.requirements.contains(&"snapper".to_string()));
    }
}
