//! Checks after a run beyond packages. A script entry is checked for what
//! its `detect` table says it installs and for its `[[verify]]` claims:
//! files and folders, words in files such as `/proc/cmdline`, settings in
//! configuration files, and systemd units. They are read from the machine
//! as the user and change nothing. What cannot be read here is said to be
//! unchecked, never passed. A claim that holds only once the machine has
//! started again is kept beside the history and checked after the reboot.

use crate::{Check, Plan, Step, doctor::System, history};
use helix_toolbox_core::{Claim, Verify, printable};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file beside the history that keeps checks for after a reboot.
pub const AFTER_REBOOT: &str = "after-reboot.json";
/// How long a kept check is shown.
const KEPT_FOR: u64 = 30 * 86_400;

/// What one check found.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Found {
    Holds(String),
    Fails(String),
    /// It could not be checked here, and why.
    Unsure(String),
    /// Its `when_exists` path is not here, so it does not apply.
    NotHere,
}

/// Fill in `~/` and `{grub}` for this machine.
pub fn expand(path: &str, system: &dyn System) -> PathBuf {
    let grub = if system.exists(Path::new("/boot/grub2")) {
        "/boot/grub2"
    } else {
        "/boot/grub"
    };
    let path = path.replace("{grub}", grub);
    match path.strip_prefix("~/") {
        Some(rest) => system.user().2.join(rest),
        None => PathBuf::from(path),
    }
}

/// A claim in words, for the check's name: "GRUB_THEME in /etc/default/grub".
pub fn describe(claim: &Claim) -> String {
    match claim {
        Claim::Path { path, absent, .. } if *absent => format!("{path} is gone"),
        Claim::Path { path, .. } => path.clone(),
        Claim::Text {
            path,
            word,
            line,
            absent,
        } => {
            let what = word
                .as_deref()
                .map(|word| format!("\"{word}\""))
                .or_else(|| line.as_deref().map(|line| format!("the line \"{line}\"")))
                .unwrap_or_default();
            if *absent {
                format!("no {what} in {path}")
            } else {
                format!("{what} in {path}")
            }
        }
        Claim::Setting {
            files,
            section,
            key,
            ..
        } => {
            let file = files.last().map(String::as_str).unwrap_or_default();
            match section {
                Some(section) => format!("{key} in [{section}] of {file}"),
                None => format!("{key} in {file}"),
            }
        }
        Claim::Service { unit, user, .. } if *user => format!("your {unit}"),
        Claim::Service { unit, .. } => unit.clone(),
    }
}

/// A file's text, or why it cannot be read: missing, or closed to this user.
fn text(system: &dyn System, path: &Path) -> Result<String, Found> {
    match system.read(path) {
        Some(text) => Ok(text),
        None if system.exists(path) => Err(Found::Unsure(format!(
            "{} cannot be read as you",
            path.display()
        ))),
        None if !system.can_look(path) => Err(Found::Unsure(format!(
            "the folder holding {} is closed to you",
            path.display()
        ))),
        None => Err(Found::Fails(format!("{} is missing", path.display()))),
    }
}

/// The value a key last gets in `text`, within `section` when one is
/// named: `key=value`, `key = value` or `key="value"`, comments left out.
pub fn setting(text: &str, section: Option<&str>, key: &str) -> Option<String> {
    let mut current: Option<String> = None;
    let mut found = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some(name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            current = Some(name.trim().to_string());
            continue;
        }
        if section.is_some_and(|wanted| current.as_deref() != Some(wanted)) {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        let value = value.trim();
        let unquoted = ['"', '\''].iter().find_map(|quote| {
            value
                .strip_prefix(*quote)
                .and_then(|rest| rest.strip_suffix(*quote))
        });
        found = Some(unquoted.unwrap_or(value).to_string());
    }
    found
}

/// Check one claim on this machine.
pub fn evaluate(verify: &Verify, system: &dyn System) -> Found {
    if let Some(when) = &verify.when_exists
        && !system.exists(&expand(when, system))
    {
        return Found::NotHere;
    }
    match &verify.claim {
        Claim::Path { path, is, absent } => {
            let path = expand(path, system);
            let there = system.exists(&path);
            if !there && !system.can_look(&path) {
                return Found::Unsure(format!(
                    "the folder holding {} is closed to you",
                    path.display()
                ));
            }
            match (there, absent) {
                (true, true) => Found::Fails(format!("{} is still there", path.display())),
                (false, true) => Found::Holds(format!("{} is gone", path.display())),
                (false, false) => Found::Fails(format!("{} is missing", path.display())),
                (true, false) => {
                    let directory = system.is_dir(&path);
                    match is.as_deref() {
                        Some("file") if directory => {
                            Found::Fails(format!("{} is a folder, not a file", path.display()))
                        }
                        Some("directory") if !directory => {
                            Found::Fails(format!("{} is not a folder", path.display()))
                        }
                        _ => Found::Holds(format!("{} is there", path.display())),
                    }
                }
            }
        }
        Claim::Text {
            path,
            word,
            line,
            absent,
        } => {
            let path = expand(path, system);
            let text = match text(system, &path) {
                Ok(text) => text,
                Err(Found::Fails(_)) if *absent => {
                    return Found::Holds(format!("{} is not there", path.display()));
                }
                Err(found) => return found,
            };
            let (has, what) = match (word, line) {
                (Some(word), _) => (
                    text.split_whitespace().any(|found| found == word),
                    format!("\"{word}\""),
                ),
                (None, Some(line)) => (
                    text.lines().any(|found| found.trim() == line),
                    format!("the line \"{line}\""),
                ),
                (None, None) => (false, String::new()),
            };
            match (has, absent) {
                (true, false) => Found::Holds(format!("{} has {what}", path.display())),
                (false, true) => Found::Holds(format!("{} has no {what}", path.display())),
                (true, true) => Found::Fails(format!("{} still has {what}", path.display())),
                (false, false) => Found::Fails(format!("{} has no {what}", path.display())),
            }
        }
        Claim::Setting {
            files,
            section,
            key,
            equals,
            contains,
            points_to,
            unset,
        } => {
            let mut value = None;
            let mut read = Vec::new();
            for file in files {
                let path = expand(file, system);
                match text(system, &path) {
                    Ok(text) => {
                        read.push(path.display().to_string());
                        if let Some(found) = setting(&text, section.as_deref(), key) {
                            value = Some((found, path));
                        }
                    }
                    Err(Found::Fails(_)) => {}
                    Err(found) => return found,
                }
            }
            if read.is_empty() {
                return if *unset {
                    Found::Holds(format!("{key} is not set: none of its files is here"))
                } else {
                    Found::Fails(format!("{} is missing", files.join(", ")))
                };
            }
            let Some((value, from)) = value.filter(|(value, _)| !value.is_empty()) else {
                return if *unset {
                    Found::Holds(format!("{key} is not set in {}", read.join(", ")))
                } else {
                    Found::Fails(format!("{key} is not set in {}", read.join(", ")))
                };
            };
            let said = format!("{key}={} in {}", printable(&value), from.display());
            if *unset {
                return Found::Fails(format!("{said}, which should be unset"));
            }
            if let Some(equals) = equals
                && &value != equals
            {
                return Found::Fails(format!("{said}, not {equals}"));
            }
            if let Some(contains) = contains
                && !value.contains(contains.as_str())
            {
                return Found::Fails(format!("{said}, without {contains}"));
            }
            if let Some(points_to) = points_to {
                let target = points_to.replace("{value}", &value);
                if !target.starts_with('/')
                    && !target.starts_with("~/")
                    && !target.starts_with("{grub}/")
                {
                    return Found::Fails(format!("{said}, which is not a full path"));
                }
                let target = expand(&target, system);
                if !system.exists(&target) {
                    return if system.can_look(&target) {
                        Found::Fails(format!("{said}, but {} is missing", target.display()))
                    } else {
                        Found::Unsure(format!(
                            "{said}; {} is in a folder closed to you",
                            target.display()
                        ))
                    };
                }
                return Found::Holds(format!("{said}, and {} is there", target.display()));
            }
            Found::Holds(said)
        }
        Claim::Service {
            unit,
            enabled,
            active,
            user,
        } => {
            if !system.exists(Path::new("/run/systemd/system")) {
                return Found::Unsure("systemd is not running here".to_string());
            }
            let ask = |question: &str| {
                let mut command = vec!["systemctl"];
                if *user {
                    command.push("--user");
                }
                command.extend([question, unit.as_str()]);
                system
                    .run(&command)
                    .map(|(yes, said)| (yes, said.trim().to_string()))
            };
            let mut said = Vec::new();
            for (wanted, question, word) in [
                (enabled, "is-enabled", "enabled"),
                (active, "is-active", "active"),
            ] {
                let Some(wanted) = wanted else {
                    continue;
                };
                let Some((yes, answer)) = ask(question) else {
                    return Found::Unsure("systemctl did not answer".to_string());
                };
                let answer = if answer.is_empty() {
                    "unknown".to_string()
                } else {
                    printable(&answer)
                };
                if yes != *wanted {
                    return Found::Fails(format!(
                        "{unit} is {answer}, not {}{word}",
                        if *wanted { "" } else { "un" }
                    ));
                }
                said.push(answer);
            }
            Found::Holds(format!("{unit} is {}", said.join(" and ")))
        }
    }
}

/// A check's name: the entry's, and the claim with this machine's GRUB folder.
fn named(name: &str, verify: &Verify, system: &dyn System) -> String {
    let grub = expand("{grub}", system).display().to_string();
    format!(
        "{name}: {}",
        describe(&verify.claim).replace("{grub}", &grub)
    )
}

/// A check's result as a run shows it; `NotHere` gives none.
fn check(name: String, found: Found) -> Option<Check> {
    Some(match found {
        Found::Holds(detail) => Check {
            name,
            passed: true,
            detail,
            ..Check::default()
        },
        Found::Fails(detail) => Check {
            name,
            passed: false,
            detail,
            ..Check::default()
        },
        Found::Unsure(detail) => Check {
            name,
            passed: true,
            detail,
            unsure: true,
            ..Check::default()
        },
        Found::NotHere => return None,
    })
}

/// The checks of a finished plan's script steps: what each installs, and
/// its claims; those for after a reboot are shown as waiting.
pub fn verify_scripts_with(
    plan: &Plan,
    holds: &dyn Fn(&crate::Probe) -> bool,
    on_path: &dyn Fn(&str) -> bool,
    system: &dyn System,
) -> Vec<Check> {
    let mut checks = Vec::new();
    for step in &plan.steps {
        let Step::Script { name, expect, .. } = step else {
            continue;
        };
        if !expect.probes.is_empty() || expect.binary.is_some() {
            let found = expect.probes.iter().find(|probe| holds(probe));
            checks.push(match (found, &expect.binary) {
                (Some(probe), _) => Check {
                    name: name.clone(),
                    passed: true,
                    detail: probe.describe(),
                    ..Check::default()
                },
                (None, Some(binary)) if on_path(binary) => Check {
                    name: name.clone(),
                    passed: true,
                    detail: format!("{binary} is on the PATH"),
                    ..Check::default()
                },
                (None, binary) => Check {
                    name: name.clone(),
                    passed: false,
                    detail: format!(
                        "not found: {}",
                        expect
                            .probes
                            .iter()
                            .map(|probe| probe.subject())
                            .chain(binary.iter().map(|binary| format!("{binary} on the PATH")))
                            .collect::<Vec<_>>()
                            .join(" or ")
                    ),
                    ..Check::default()
                },
            });
        }
        for verify in &expect.checks {
            if verify.after_reboot {
                checks.push(Check {
                    name: named(name, verify, system),
                    passed: true,
                    detail: "checked after the next reboot".to_string(),
                    later: true,
                    ..Check::default()
                });
                continue;
            }
            checks.extend(check(named(name, verify, system), evaluate(verify, system)));
        }
    }
    checks
}

/// Checks kept for after a reboot, from one run of one entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Waiting {
    pub entry: String,
    pub name: String,
    /// When the run finished, in Unix seconds.
    pub ran: u64,
    pub checks: Vec<Verify>,
}

/// The kept checks, oldest first; none when the file is missing or unreadable.
pub fn kept(system: &dyn System, directory: &Path) -> Vec<Waiting> {
    system
        .read(&directory.join(AFTER_REBOOT))
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Keep a finished run's checks for after the reboot, in place of any kept
/// from an earlier run of the same entry; those older than 30 days go.
pub fn keep(
    directory: &Path,
    plan: &Plan,
    finished: &crate::Finished,
    now: u64,
) -> std::io::Result<()> {
    let done: Vec<&str> = finished
        .steps
        .iter()
        .filter(|step| step.state == crate::StepState::Done)
        .map(|step| step.id.as_str())
        .collect();
    let new: Vec<Waiting> = plan
        .steps
        .iter()
        .filter_map(|step| match step {
            Step::Script {
                id, name, expect, ..
            } if done.contains(&id.as_str()) => {
                let checks: Vec<Verify> = expect
                    .checks
                    .iter()
                    .filter(|verify| verify.after_reboot)
                    .cloned()
                    .collect();
                (!checks.is_empty()).then(|| Waiting {
                    entry: id.clone(),
                    name: name.clone(),
                    ran: now,
                    checks,
                })
            }
            _ => None,
        })
        .collect();
    let path = directory.join(AFTER_REBOOT);
    let mut all = kept(&crate::doctor::Here, directory);
    let before = all.len();
    all.retain(|waiting| {
        now.saturating_sub(waiting.ran) < KEPT_FOR
            && !new.iter().any(|other| other.entry == waiting.entry)
    });
    if new.is_empty() && all.len() == before {
        return Ok(());
    }
    all.extend(new);
    std::fs::create_dir_all(directory)?;
    let text = serde_json::to_string_pretty(&all).map_err(std::io::Error::other)?;
    let scratch = directory.join(format!("{AFTER_REBOOT}.new"));
    std::fs::write(&scratch, text)?;
    std::fs::rename(scratch, path)
}

/// When the machine last started, in Unix seconds, from `/proc/stat`.
pub fn booted(system: &dyn System) -> Option<u64> {
    system
        .read(Path::new("/proc/stat"))?
        .lines()
        .find_map(|line| line.strip_prefix("btime ")?.trim().parse().ok())
}

/// The kept checks reviewed: those whose machine has started since their
/// run, checked now; the others still waiting.
pub struct Reviewed {
    pub waiting: Waiting,
    /// None until the machine has rebooted since the run.
    pub checks: Option<Vec<Check>>,
}

pub fn review(system: &dyn System, directory: &Path) -> Vec<Reviewed> {
    let started = booted(system);
    kept(system, directory)
        .into_iter()
        .map(|waiting| {
            let rebooted = started.is_some_and(|started| started > waiting.ran);
            let checks = rebooted.then(|| {
                waiting
                    .checks
                    .iter()
                    .filter_map(|verify| {
                        check(
                            named(&waiting.name, verify, system),
                            evaluate(verify, system),
                        )
                    })
                    .collect()
            });
            Reviewed { waiting, checks }
        })
        .collect()
}

/// An entry's checks now, whatever ran: what it installs and its claims,
/// those for after a reboot only once `rebooted` (since its last run).
/// Empty when it has none.
pub fn check_entry(
    entry: &helix_toolbox_core::Entry,
    platform: &helix_toolbox_platform::LinuxInfo,
    rebooted: bool,
    holds: &dyn Fn(&crate::Probe) -> bool,
    on_path: &dyn Fn(&str) -> bool,
    system: &dyn System,
) -> Vec<Check> {
    let mut expect = crate::expected(entry, platform);
    if let helix_toolbox_core::Action::Package { binary, .. } = &entry.action
        && let Some(identity) = entry.identity()
    {
        expect.probes = crate::own_probes(
            identity.packages,
            identity.aur,
            identity.flatpak,
            platform.manager,
        );
        expect.binary = binary.clone();
    }
    if rebooted {
        for verify in &mut expect.checks {
            verify.after_reboot = false;
        }
    }
    let plan = Plan {
        steps: vec![Step::Script {
            id: entry.id.clone(),
            name: entry.name.clone(),
            file: PathBuf::new(),
            program: String::new(),
            args: Vec::new(),
            interactive: false,
            expect,
        }],
        ..Plan::default()
    };
    verify_scripts_with(&plan, holds, on_path, system)
}

/// Whether the machine has started since the entry last ran successfully,
/// from a history listed newest first; true when it never ran here.
pub fn rebooted_since(system: &dyn System, history: &[history::Record], id: &str) -> bool {
    let last = history
        .iter()
        .find(|record| record.success && record.entries.iter().any(|entry| entry == id));
    match (last, booted(system)) {
        (None, _) => true,
        (Some(record), Some(started)) => started > record.unix_timestamp,
        (Some(_), None) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Expected, Finished, Outcome, Probe};
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        folders: HashSet<PathBuf>,
        /// Paths that exist but cannot be read.
        locked: HashSet<PathBuf>,
        /// Folders closed to the user.
        closed: Vec<PathBuf>,
        commands: HashMap<String, (bool, String)>,
    }

    impl Fake {
        fn file(mut self, path: &str, text: &str) -> Self {
            self.files.insert(PathBuf::from(path), text.to_string());
            self
        }
        fn folder(mut self, path: &str) -> Self {
            self.folders.insert(PathBuf::from(path));
            self
        }
        fn answer(mut self, command: &str, yes: bool, said: &str) -> Self {
            self.commands
                .insert(command.to_string(), (yes, said.to_string()));
            self
        }
        fn systemd(self) -> Self {
            self.folder("/run/systemd/system")
        }
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
                || self.folders.contains(path)
                || self.locked.contains(path)
        }
        fn list(&self, _: &Path) -> Vec<String> {
            Vec::new()
        }
        fn link(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn run(&self, command: &[&str]) -> Option<(bool, String)> {
            self.commands.get(&command.join(" ")).cloned()
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
            false
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
        fn can_look(&self, path: &Path) -> bool {
            !self.closed.iter().any(|closed| path.starts_with(closed))
        }
        fn is_dir(&self, path: &Path) -> bool {
            self.folders.contains(path)
        }
    }

    fn check(claim: Claim) -> Verify {
        Verify {
            claim,
            when_exists: None,
            after_reboot: false,
        }
    }

    fn theme(points_to: Option<&str>, equals: Option<&str>) -> Verify {
        check(Claim::Setting {
            files: vec![
                "/etc/sddm.conf.d/zz-commander-theme.conf".into(),
                "/etc/sddm.conf".into(),
            ],
            section: Some("Theme".into()),
            key: "Current".into(),
            equals: equals.map(str::to_string),
            contains: None,
            points_to: points_to.map(str::to_string),
            unset: false,
        })
    }

    #[test]
    fn settings_are_read_as_programs_read_them() {
        let ini = "# Current=commented\n[General]\nCurrent=general\n[Theme]\n Current = silent \n";
        assert_eq!(
            setting(ini, Some("Theme"), "Current").as_deref(),
            Some("silent")
        );
        assert_eq!(
            setting(ini, Some("General"), "Current").as_deref(),
            Some("general")
        );
        let grub = "GRUB_THEME=\"/boot/grub/themes/a/theme.txt\"\nGRUB_THEME='/boot/grub/themes/b/theme.txt'\n";
        assert_eq!(
            setting(grub, None, "GRUB_THEME").as_deref(),
            Some("/boot/grub/themes/b/theme.txt")
        );
        assert_eq!(
            setting("export LANG=en_US.UTF-8", None, "LANG").as_deref(),
            Some("en_US.UTF-8")
        );
        assert_eq!(setting(grub, None, "GRUB_TIMEOUT"), None);
    }

    #[test]
    fn a_setting_must_point_to_what_is_installed() {
        let points = Some("/usr/share/sddm/themes/{value}/metadata.desktop");
        let machine = Fake::default()
            .file(
                "/etc/sddm.conf.d/zz-commander-theme.conf",
                "[Theme]\nCurrent=silent\n",
            )
            .file("/usr/share/sddm/themes/silent/metadata.desktop", "");
        assert!(
            matches!(evaluate(&theme(points, None), &machine), Found::Holds(said) if said.contains("silent"))
        );
        assert!(matches!(
            evaluate(&theme(None, Some("silent")), &machine),
            Found::Holds(_)
        ));
        assert!(
            matches!(evaluate(&theme(None, Some("breeze")), &machine), Found::Fails(said) if said.contains("not breeze"))
        );
        // /etc/sddm.conf is read last and wins.
        let overridden = Fake::default()
            .file(
                "/etc/sddm.conf.d/zz-commander-theme.conf",
                "[Theme]\nCurrent=silent\n",
            )
            .file("/etc/sddm.conf", "[Theme]\nCurrent=breeze\n")
            .file("/usr/share/sddm/themes/silent/metadata.desktop", "");
        assert!(
            matches!(evaluate(&theme(points, None), &overridden), Found::Fails(said) if said.contains("breeze") && said.contains("missing"))
        );
        // A file only root can read is not a pass, nor a failure.
        let mut locked = Fake::default();
        locked.locked.insert(PathBuf::from("/etc/sddm.conf"));
        assert!(matches!(
            evaluate(&theme(points, None), &locked),
            Found::Unsure(_)
        ));
        let missing = Fake::default();
        assert!(
            matches!(evaluate(&theme(points, None), &missing), Found::Fails(said) if said.contains("missing"))
        );
    }

    #[test]
    fn grub_paths_and_closed_folders_are_told_apart() {
        let grub = check(Claim::Setting {
            files: vec!["/etc/default/grub".into()],
            section: None,
            key: "GRUB_THEME".into(),
            equals: None,
            contains: Some("/themes/commander-dracula-".into()),
            points_to: Some("{value}".into()),
            unset: false,
        });
        let machine = Fake::default()
            .file(
                "/etc/default/grub",
                "GRUB_THEME=\"/boot/grub/themes/commander-dracula-1/theme.txt\"\n",
            )
            .file("/boot/grub/themes/commander-dracula-1/theme.txt", "");
        assert!(matches!(evaluate(&grub, &machine), Found::Holds(_)));
        let mut closed = Fake::default().file(
            "/etc/default/grub",
            "GRUB_THEME=\"/boot/grub/themes/commander-dracula-1/theme.txt\"\n",
        );
        closed.closed.push(PathBuf::from("/boot"));
        assert!(
            matches!(evaluate(&grub, &closed), Found::Unsure(said) if said.contains("closed to you"))
        );
        let other = Fake::default().file(
            "/etc/default/grub",
            "GRUB_THEME=\"/usr/share/grub/themes/x/theme.txt\"\n",
        );
        assert!(matches!(evaluate(&grub, &other), Found::Fails(said) if said.contains("without")));
        let unset = check(Claim::Setting {
            files: vec!["/etc/default/grub".into()],
            section: None,
            key: "GRUB_THEME".into(),
            equals: None,
            contains: None,
            points_to: None,
            unset: true,
        });
        assert!(matches!(evaluate(&unset, &other), Found::Fails(_)));
        let cleared = Fake::default().file("/etc/default/grub", "GRUB_THEME=\"\"\n");
        assert!(matches!(evaluate(&unset, &cleared), Found::Holds(_)));
        // {grub} is GRUB's folder on this machine.
        let fedora = Fake::default()
            .folder("/boot/grub2")
            .file("/boot/grub2/grub.cfg", "");
        let config = check(Claim::Path {
            path: "{grub}/grub.cfg".into(),
            is: Some("file".into()),
            absent: false,
        });
        assert!(
            matches!(evaluate(&config, &fedora), Found::Holds(said) if said.contains("/boot/grub2/grub.cfg"))
        );
    }

    #[test]
    fn words_lines_and_paths() {
        let machine = Fake::default()
            .file("/proc/cmdline", "BOOT_IMAGE=/vmlinuz quiet splash\n")
            .file("/sys/module/apparmor/parameters/enabled", "Y\n")
            .folder("/home/me/.config/fish")
            .file("/home/me/.config/fish/config.fish", "");
        let word = |word: &str, absent| {
            check(Claim::Text {
                path: "/proc/cmdline".into(),
                word: Some(word.into()),
                line: None,
                absent,
            })
        };
        assert!(matches!(
            evaluate(&word("splash", false), &machine),
            Found::Holds(_)
        ));
        assert!(matches!(
            evaluate(&word("splash", true), &machine),
            Found::Fails(_)
        ));
        assert!(matches!(
            evaluate(&word("spla", false), &machine),
            Found::Fails(_)
        ));
        let line = check(Claim::Text {
            path: "/sys/module/apparmor/parameters/enabled".into(),
            word: None,
            line: Some("Y".into()),
            absent: false,
        });
        assert!(matches!(evaluate(&line, &machine), Found::Holds(_)));
        let folder = check(Claim::Path {
            path: "~/.config/fish".into(),
            is: Some("directory".into()),
            absent: false,
        });
        assert!(matches!(evaluate(&folder, &machine), Found::Holds(_)));
        let not_a_file = check(Claim::Path {
            path: "~/.config/fish".into(),
            is: Some("file".into()),
            absent: false,
        });
        assert!(matches!(evaluate(&not_a_file, &machine), Found::Fails(_)));
        let gone = check(Claim::Path {
            path: "/snap".into(),
            is: None,
            absent: true,
        });
        assert!(matches!(evaluate(&gone, &machine), Found::Holds(_)));
        let mut only_there = check(Claim::Path {
            path: "/etc/sddm.conf.d/x.conf".into(),
            is: None,
            absent: false,
        });
        only_there.when_exists = Some("/etc/sddm.conf".into());
        assert_eq!(evaluate(&only_there, &machine), Found::NotHere);
    }

    #[test]
    fn services_are_asked_of_systemd_and_containers_are_not_checked() {
        let timer = check(Claim::Service {
            unit: "fstrim.timer".into(),
            enabled: Some(true),
            active: Some(true),
            user: false,
        });
        assert!(
            matches!(evaluate(&timer, &Fake::default()), Found::Unsure(said) if said.contains("systemd"))
        );
        let running = Fake::default()
            .systemd()
            .answer("systemctl is-enabled fstrim.timer", true, "enabled\n")
            .answer("systemctl is-active fstrim.timer", true, "active\n");
        assert!(
            matches!(evaluate(&timer, &running), Found::Holds(said) if said == "fstrim.timer is enabled and active")
        );
        let stopped = Fake::default()
            .systemd()
            .answer("systemctl is-enabled fstrim.timer", true, "enabled\n")
            .answer("systemctl is-active fstrim.timer", false, "inactive\n");
        assert!(
            matches!(evaluate(&timer, &stopped), Found::Fails(said) if said.contains("inactive, not active"))
        );
        let off = check(Claim::Service {
            unit: "lightdm.service".into(),
            enabled: Some(false),
            active: None,
            user: false,
        });
        let disabled = Fake::default().systemd().answer(
            "systemctl is-enabled lightdm.service",
            false,
            "disabled\n",
        );
        assert!(matches!(evaluate(&off, &disabled), Found::Holds(_)));
        let mine = check(Claim::Service {
            unit: "pipewire.socket".into(),
            enabled: Some(true),
            active: None,
            user: true,
        });
        let user = Fake::default().systemd().answer(
            "systemctl --user is-enabled pipewire.socket",
            true,
            "enabled\n",
        );
        assert!(matches!(evaluate(&mine, &user), Found::Holds(_)));
    }

    fn script_plan(expect: Expected) -> Plan {
        Plan {
            steps: vec![Step::Script {
                id: "plymouth-on".into(),
                name: "Boot splash on".into(),
                file: PathBuf::from("/x.sh"),
                program: "sh".into(),
                args: Vec::new(),
                interactive: false,
                expect,
            }],
            ..Plan::default()
        }
    }

    #[test]
    fn a_script_is_checked_for_what_it_installs_and_what_it_claims() {
        let mut splash = check(Claim::Text {
            path: "/proc/cmdline".into(),
            word: Some("splash".into()),
            line: None,
            absent: false,
        });
        splash.after_reboot = true;
        let theme = check(Claim::Path {
            path: "/usr/share/plymouth/themes/spinner".into(),
            is: None,
            absent: false,
        });
        let plan = script_plan(Expected {
            probes: vec![
                Probe::package(helix_toolbox_core::PackageManager::Arch, "plymouth").unwrap(),
            ],
            binary: Some("plymouth".into()),
            checks: vec![splash, theme],
        });
        let machine = Fake::default().folder("/usr/share/plymouth/themes/spinner");
        let checks = verify_scripts_with(&plan, &|_| true, &|_| false, &machine);
        assert_eq!(checks.len(), 3);
        assert!(checks[0].passed && checks[0].detail.contains("plymouth"));
        assert!(checks[1].later && checks[1].passed);
        assert!(checks[2].passed && !checks[2].later);
        let finished = Finished {
            outcome: Outcome::Success,
            checks,
            steps: Vec::new(),
        };
        assert!(finished.verified());
        assert_eq!(
            finished.describe(),
            "exit 0, verified; 1 check after the next reboot"
        );
        // Nothing installed: it fails the run's verification.
        let missing = verify_scripts_with(&plan, &|_| false, &|_| false, &machine);
        assert!(!missing[0].passed);
        assert!(
            missing[0].detail.contains("plymouth on the PATH"),
            "{}",
            missing[0].detail
        );
        // Only unsure checks: the run is not verified, and says why.
        let unsure = Finished {
            outcome: Outcome::Success,
            checks: vec![Check {
                name: "x".into(),
                passed: true,
                detail: "systemd is not running here".into(),
                unsure: true,
                later: false,
            }],
            steps: Vec::new(),
        };
        assert!(!unsure.verified());
        assert_eq!(unsure.describe(), "exit 0; 1 check not possible here");
    }

    #[test]
    fn checks_for_after_the_reboot_are_kept_then_checked_once_it_started_again() {
        let directory = tempfile::tempdir().unwrap();
        let mut splash = check(Claim::Text {
            path: "/proc/cmdline".into(),
            word: Some("splash".into()),
            line: None,
            absent: false,
        });
        splash.after_reboot = true;
        let plan = script_plan(Expected {
            checks: vec![splash],
            ..Expected::default()
        });
        let finished = Finished {
            outcome: Outcome::Success,
            checks: Vec::new(),
            steps: vec![crate::StepResult {
                id: "plymouth-on".into(),
                name: "Boot splash on".into(),
                state: crate::StepState::Done,
            }],
        };
        keep(directory.path(), &plan, &finished, 1_000).unwrap();
        // Again later: one kept, the newer.
        keep(directory.path(), &plan, &finished, 2_000).unwrap();
        let text = std::fs::read_to_string(directory.path().join(AFTER_REBOOT)).unwrap();
        let machine = |booted: u64, cmdline: &str| {
            Fake::default()
                .file(
                    &directory.path().join(AFTER_REBOOT).display().to_string(),
                    &text,
                )
                .file("/proc/stat", &format!("cpu 1 2 3\nbtime {booted}\n"))
                .file("/proc/cmdline", cmdline)
        };
        let before = review(&machine(1_500, "quiet"), directory.path());
        assert_eq!(before.len(), 1);
        assert_eq!(before[0].waiting.ran, 2_000);
        assert!(before[0].checks.is_none(), "not rebooted since");
        let after = review(&machine(2_500, "quiet splash"), directory.path());
        let checks = after[0].checks.as_ref().unwrap();
        assert!(checks[0].passed && !checks[0].later);
        let wrong = review(&machine(2_500, "quiet"), directory.path());
        assert!(!wrong[0].checks.as_ref().unwrap()[0].passed);
        // A failed step keeps nothing, and old ones go.
        let failed = Finished {
            steps: vec![crate::StepResult {
                state: crate::StepState::Failed(1),
                ..finished.steps[0].clone()
            }],
            ..finished.clone()
        };
        keep(
            directory.path(),
            &script_plan(Expected::default()),
            &failed,
            2_000 + KEPT_FOR,
        )
        .unwrap();
        assert!(kept(&crate::doctor::Here, directory.path()).is_empty());
    }
}
