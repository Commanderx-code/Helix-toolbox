//! The system change preview: what a plan is expected to change on this
//! machine, step by step: files (side by side where the new contents can be
//! worked out without writing anything), packages, services and when it
//! takes effect. Every change says how it is known: worked out here,
//! declared by the entry, or impossible to tell before the step runs. It
//! only reads files and asks package managers read-only questions.

use crate::{Plan, Step, doctor::System};
use helixforge_core::{Entry, PackageManager, printable};
use helixforge_platform::LinuxInfo;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// How a change is known.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Certainty {
    /// Worked out on this machine: a package manager's own answer, a file
    /// rendered by the installer's own code, a backup's contents.
    Known,
    /// Declared by the entry, or seen in its script.
    Expected,
    /// Cannot be told before the step runs.
    Unknown,
}

impl Certainty {
    pub fn label(self) -> &'static str {
        match self {
            Certainty::Known => "known",
            Certainty::Expected => "expected",
            Certainty::Unknown => "not known in advance",
        }
    }

    pub fn mark(self) -> &'static str {
        match self {
            Certainty::Known => "✓",
            Certainty::Expected => "~",
            Certainty::Unknown => "?",
        }
    }
}

/// What a change touches.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Area {
    File,
    Package,
    Service,
    Boot,
    Other,
}

/// One expected change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Change {
    pub certainty: Certainty,
    pub area: Area,
    pub text: String,
    /// The file's contents now and afterwards, when both are known.
    pub diff: Option<Diff>,
}

/// One step's changes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct StepPreview {
    pub id: String,
    pub name: String,
    pub changes: Vec<Change>,
}

/// A side-by-side comparison of a file now and after the step.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diff {
    pub path: String,
    /// Rows pairing the two sides: a line now, a line after, or both.
    pub rows: Vec<Row>,
    /// Whether anything differs.
    pub changed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Row {
    pub before: Option<String>,
    pub after: Option<String>,
    pub tag: Tag,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Tag {
    Same,
    Changed,
    Removed,
    Added,
}

/// What the preview asks the machine; tests answer instead.
pub struct Look<'a> {
    pub system: &'a dyn System,
    /// Run one of the installers' `configure.py` renderers, which only
    /// print: the new contents of a file, or why not.
    pub render: &'a dyn Fn(&[String]) -> Result<String, String>,
}

/// Files larger than this, or with lines past this count, get no diff.
const DIFF_BYTES: usize = 1 << 20;
const DIFF_LINES: usize = 4_000;

/// Preview a plan's changes.
pub fn preview<'a>(
    plan: &Plan,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
    platform: &LinuxInfo,
    look: &Look,
) -> Vec<StepPreview> {
    plan.steps
        .iter()
        .map(|step| StepPreview {
            id: step.id().to_string(),
            name: step.name().to_string(),
            changes: step_changes(step, lookup(step.id()), platform, look),
        })
        .collect()
}

fn change(certainty: Certainty, area: Area, text: impl Into<String>) -> Change {
    Change {
        certainty,
        area,
        text: printable(&text.into()),
        diff: None,
    }
}

fn step_changes(
    step: &Step,
    entry: Option<&Entry>,
    platform: &LinuxInfo,
    look: &Look,
) -> Vec<Change> {
    let mut changes = Vec::new();
    match step {
        Step::Package {
            sources, removal, ..
        } => {
            let Some(first) = sources.first() else {
                return changes;
            };
            let names: Vec<String> = package_names(first);
            if *removal {
                changes.push(change(
                    Certainty::Known,
                    Area::Package,
                    format!("removes {} through {}", names.join(", "), first.label),
                ));
                changes.extend(simulate(platform.manager, &names, true, look.system));
            } else {
                changes.push(change(
                    Certainty::Known,
                    Area::Package,
                    format!("installs {} from {}", names.join(", "), first.label),
                ));
                if first.label != "Flathub" && !first.label.starts_with("the AUR") {
                    changes.extend(simulate(platform.manager, &names, false, look.system));
                }
                for other in &sources[1..] {
                    changes.push(change(
                        Certainty::Expected,
                        Area::Package,
                        format!("if that fails or leaves it missing, from {}", other.label),
                    ));
                }
                if let Some(entry) = entry
                    && let Some(identity) = entry.identity()
                    && !identity.conflicts.is_empty()
                {
                    changes.push(change(
                        Certainty::Expected,
                        Area::Package,
                        format!(
                            "replaces {} where installed; the package manager asks first",
                            identity.conflicts.join(", ")
                        ),
                    ));
                }
                if platform.manager == Some(PackageManager::Debian) {
                    changes.push(change(
                        Certainty::Unknown,
                        Area::Service,
                        "Debian and Ubuntu packages start the services they install; which, if any, is up to the package",
                    ));
                }
            }
        }
        Step::Script { file, .. } => {
            if let Some(entry) = entry {
                changes.extend(declared(entry, look));
                changes.extend(writes(entry, look.system));
            }
            // Declared services say more than a look at the script.
            if entry.is_none_or(|entry| entry.effects.services.is_empty()) {
                changes.extend(seen_in(file));
            }
            changes.push(change(
                Certainty::Unknown,
                Area::Other,
                if entry.is_some_and(|entry| !entry.effects.is_empty()) {
                    "anything else the script does as it runs, and the answers it asks you for"
                } else {
                    "what the script changes: it decides as it runs, so HelixForge cannot show it in advance"
                },
            ));
        }
        Step::Command { line, .. } => {
            changes.push(change(
                Certainty::Known,
                Area::Other,
                format!("runs {line}"),
            ));
            changes.push(change(
                Certainty::Unknown,
                Area::Other,
                "what the command changes",
            ));
        }
        Step::Nix { action, .. } => match action {
            helixforge_core::NixAction::ProfileInstall { installable } => {
                changes.push(change(
                    Certainty::Known,
                    Area::Package,
                    format!("adds {installable} to your Nix profile; nothing outside the Nix store and your profile changes"),
                ));
            }
            helixforge_core::NixAction::HomeManagerSwitch { flake } => {
                changes.push(change(
                    Certainty::Known,
                    Area::Other,
                    format!("switches Home Manager to {flake}"),
                ));
                changes.push(change(
                    Certainty::Unknown,
                    Area::File,
                    "which files and packages change: the flake decides; home-manager build, then nvd diff, shows it",
                ));
            }
        },
        Step::Native { .. } => changes.push(change(
            Certainty::Known,
            Area::Other,
            "only reads the system and prints what it finds",
        )),
        Step::Shell {
            display,
            changes: made,
            ..
        } => {
            if made.is_empty() {
                changes.push(change(
                    Certainty::Known,
                    Area::Other,
                    format!("runs {display}"),
                ));
            }
            for made in made {
                let mut found = change(
                    Certainty::Known,
                    Area::File,
                    format!("{}: {}", made.path.display(), made.text),
                );
                if let Some(from) = &made.from {
                    let before = read_text(look.system, &made.path).unwrap_or_default();
                    match read_text(look.system, from) {
                        Some(after) => {
                            found.diff = diff(&made.path.display().to_string(), &before, &after)
                        }
                        None => {
                            found
                                .text
                                .push_str(" (its contents cannot be read here to compare)");
                        }
                    }
                }
                changes.push(found);
            }
        }
        Step::Update {
            source, targets, ..
        } => {
            if source.label == "pacman" && targets.len() > 1 || step.id() == "system-update" {
                match look.system.run(&["pacman", "-Qu"]) {
                    Some((true, text)) if !text.trim().is_empty() => {
                        let lines: Vec<&str> = text.lines().collect();
                        changes.push(change(
                            Certainty::Known,
                            Area::Package,
                            format!(
                                "updates {} packages from the repository metadata on this machine: {}",
                                lines.len(),
                                lines.join("; ")
                            ),
                        ));
                    }
                    _ => changes.push(change(
                        Certainty::Unknown,
                        Area::Package,
                        "which packages: pacman -Syu reads the repositories again first",
                    )),
                }
                changes.push(change(
                    Certainty::Unknown,
                    Area::Package,
                    "packages newer than the metadata on this machine, which pacman -Syu downloads first",
                ));
            } else {
                for target in targets {
                    changes.push(change(
                        Certainty::Known,
                        Area::Package,
                        format!(
                            "updates {}{} through {}",
                            target.name,
                            target
                                .from
                                .as_deref()
                                .map(|from| format!(" from {from}"))
                                .unwrap_or_default(),
                            source.label
                        ),
                    ));
                }
            }
        }
    }
    changes
}

/// The package names a source installs: its commands' last words.
fn package_names(source: &crate::Source) -> Vec<String> {
    source
        .commands
        .last()
        .map(|command| {
            command
                .iter()
                .rev()
                .take_while(|word| {
                    !word.starts_with('-')
                        && !matches!(
                            word.as_str(),
                            "install" | "add" | "remove" | "del" | "-S" | "-R" | "sudo"
                        )
                })
                .cloned()
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect()
        })
        .unwrap_or_default()
}

/// What the package manager says it would do, asked without root.
fn simulate(
    manager: Option<PackageManager>,
    names: &[String],
    removal: bool,
    system: &dyn System,
) -> Vec<Change> {
    let names: Vec<&str> = names
        .iter()
        .map(String::as_str)
        .filter(|name| helixforge_core::valid_package_name(name))
        .collect();
    if names.is_empty() {
        return Vec::new();
    }
    let mut changes = Vec::new();
    match manager {
        Some(PackageManager::Arch) => {
            let mut command = vec![
                "pacman",
                if removal { "-Rp" } else { "-Sp" },
                "--print-format",
                "%n %v",
            ];
            command.extend(&names);
            match system.run(&command) {
                Some((true, text)) => {
                    let all: Vec<&str> = text
                        .lines()
                        .map(str::trim)
                        .filter(|line| !line.is_empty())
                        .collect();
                    let others: Vec<&str> = all
                        .iter()
                        .copied()
                        .filter(|line| {
                            !names
                                .iter()
                                .any(|name| line.split(' ').next() == Some(name))
                        })
                        .collect();
                    changes.push(change(
                        Certainty::Known,
                        Area::Package,
                        if others.is_empty() {
                            format!(
                                "pacman lists nothing else to {}",
                                if removal { "remove" } else { "install" }
                            )
                        } else {
                            format!(
                                "pacman would also {}: {}",
                                if removal { "remove" } else { "install" },
                                others.join(", ")
                            )
                        },
                    ));
                }
                _ => changes.push(change(
                    Certainty::Unknown,
                    Area::Package,
                    "which dependencies: pacman could not say from the metadata on this machine",
                )),
            }
        }
        Some(PackageManager::Debian) => {
            let mut command = vec![
                "apt-get",
                "-s",
                "-q",
                if removal { "remove" } else { "install" },
            ];
            command.extend(&names);
            match system.run(&command) {
                Some((true, text)) => {
                    let pick = |prefix: &str| -> Vec<String> {
                        text.lines()
                            .filter_map(|line| line.strip_prefix(prefix))
                            .filter_map(|rest| rest.split_whitespace().next())
                            .filter(|name| !names.contains(name))
                            .map(String::from)
                            .collect()
                    };
                    let installs = pick("Inst ");
                    let removes = pick("Remv ");
                    if !installs.is_empty() {
                        changes.push(change(
                            Certainty::Known,
                            Area::Package,
                            format!("apt would also install: {}", installs.join(", ")),
                        ));
                    }
                    if !removes.is_empty() {
                        changes.push(change(
                            Certainty::Known,
                            Area::Package,
                            format!("apt would remove: {}", removes.join(", ")),
                        ));
                    }
                    if installs.is_empty() && removes.is_empty() {
                        changes.push(change(
                            Certainty::Known,
                            Area::Package,
                            "apt lists nothing else to change",
                        ));
                    }
                }
                _ => changes.push(change(
                    Certainty::Unknown,
                    Area::Package,
                    "which dependencies: apt could not simulate it here",
                )),
            }
        }
        Some(other) => changes.push(change(
            Certainty::Unknown,
            Area::Package,
            format!(
                "which dependencies: {} lists them only with root, as it starts",
                other.executable()
            ),
        )),
        None => {}
    }
    changes
}

/// The entry's declared effects, with new contents rendered where it can.
fn declared(entry: &Entry, look: &Look) -> Vec<Change> {
    let effects = &entry.effects;
    let mut changes = Vec::new();
    for package in &effects.packages {
        changes.push(change(
            Certainty::Expected,
            Area::Package,
            format!("installs {package}"),
        ));
    }
    for service in &effects.services {
        changes.push(change(Certainty::Expected, Area::Service, service.clone()));
    }
    let grub = if look.system.exists(Path::new("/boot/grub2")) {
        "/boot/grub2"
    } else {
        "/boot/grub"
    };
    let expand = |text: &str| {
        text.replace("{grub}", grub)
            .replace("{time}", "<date and time of the run>")
    };
    for file in &effects.files {
        let path = expand(&file.path);
        let when = file
            .when
            .as_deref()
            .map(|when| format!(" (when {when})"))
            .unwrap_or_default();
        let shown = PathBuf::from(&path);
        let exists = look.system.exists(&shown);
        let mut found = change(
            Certainty::Expected,
            Area::File,
            format!("{path}: {}{when}", file.change),
        );
        if !file.render.is_empty() {
            if !exists {
                found
                    .text
                    .push_str("; it is not on this machine, so this does not apply");
            } else {
                let arguments: Vec<String> = file.render.iter().map(|word| expand(word)).collect();
                match (read_text(look.system, &shown), (look.render)(&arguments)) {
                    (Some(before), Ok(after)) => {
                        found.certainty = Certainty::Known;
                        found.diff = diff(&path, &before, &after);
                        if found.diff.as_ref().is_some_and(|diff| !diff.changed) {
                            found
                                .text
                                .push_str("; it already says this, so nothing changes");
                        }
                    }
                    (_, Err(why)) => found.text.push_str(&printable(&format!(
                        "; its new contents could not be worked out: {why}"
                    ))),
                    (None, _) => found.text.push_str("; it cannot be read here to compare"),
                }
            }
        } else if file.path.contains("{time}") || !exists {
            found.text.push_str(" (created)");
        }
        changes.push(found);
    }
    if let Some(when) = &effects.takes_effect {
        changes.push(change(
            Certainty::Expected,
            Area::Boot,
            format!("takes effect: {when}"),
        ));
    }
    for unknown in &effects.unknown {
        changes.push(change(Certainty::Unknown, Area::Other, unknown.clone()));
    }
    changes
}

/// The user paths an entry says it writes.
fn writes(entry: &Entry, system: &dyn System) -> Vec<Change> {
    let (_, _, home) = system.user();
    entry
        .writes
        .iter()
        .map(|written| {
            let path = match written.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(written),
            };
            let what = if system.is_link(&path) {
                "replaced; it is a link now, which the installer refuses to overwrite if Home Manager manages it"
            } else if system.exists(&path) {
                "replaced, keeping the current one as a backup"
            } else {
                "created"
            };
            let mut found = change(Certainty::Expected, Area::File, format!("{written}: {what}"));
            if system.exists(&path) {
                found.text.push_str("; the new contents come from the entry's pinned source, read as it runs");
            }
            found
        })
        .collect()
}

/// Services and system paths the script names, which it may change.
fn seen_in(file: &Path) -> Vec<Change> {
    let Ok(text) = std::fs::read_to_string(file) else {
        return Vec::new();
    };
    let mut services: Vec<String> = Vec::new();
    for line in text.lines().filter(|line| {
        let line = line.trim_start();
        // Comments and messages name commands without running them.
        !line.starts_with('#') && !line.starts_with("printf") && !line.starts_with("echo")
    }) {
        let Some(at) = line.find("systemctl ") else {
            continue;
        };
        let words: Vec<&str> = line[at + "systemctl ".len()..].split_whitespace().collect();
        let Some(verb) = words.iter().find(|word| !word.starts_with('-')) else {
            continue;
        };
        if !matches!(
            *verb,
            "enable" | "disable" | "start" | "stop" | "restart" | "reload" | "mask"
        ) {
            continue;
        }
        let unit = words
            .iter()
            .skip_while(|word| *word != verb)
            .skip(1)
            .find(|word| !word.starts_with('-'))
            .map(|word| word.trim_matches(|c: char| "\"';.,)".contains(c)))
            .unwrap_or("a unit");
        let unit = if unit.starts_with('$') {
            "a unit it chooses"
        } else {
            unit
        };
        let text = format!("may {verb} {unit}");
        if !services.contains(&text) {
            services.push(text);
        }
    }
    services
        .into_iter()
        .map(|text| {
            change(
                Certainty::Expected,
                Area::Service,
                format!("{text} (seen in its script)"),
            )
        })
        .collect()
}

fn read_text(system: &dyn System, path: &Path) -> Option<String> {
    let text = system.read(path)?;
    (text.len() <= DIFF_BYTES).then_some(text)
}

/// Compare a file's lines now and after, side by side. None when either is
/// too long to compare line by line.
pub fn diff(path: &str, before: &str, after: &str) -> Option<Diff> {
    let old: Vec<&str> = before.lines().collect();
    let new: Vec<&str> = after.lines().collect();
    if old.len() > DIFF_LINES || new.len() > DIFF_LINES {
        return None;
    }
    // Longest common subsequence, then walk it.
    let (n, m) = (old.len(), new.len());
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] {
                table[i + 1][j + 1] + 1
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }
    let mut rows = Vec::new();
    let (mut i, mut j) = (0, 0);
    let mut removed: Vec<&str> = Vec::new();
    let mut added: Vec<&str> = Vec::new();
    let flush = |rows: &mut Vec<Row>, removed: &mut Vec<&str>, added: &mut Vec<&str>| {
        let pairs = removed.len().max(added.len());
        for index in 0..pairs {
            let before = removed.get(index).map(|line| printable(line));
            let after = added.get(index).map(|line| printable(line));
            let tag = match (&before, &after) {
                (Some(_), Some(_)) => Tag::Changed,
                (Some(_), None) => Tag::Removed,
                _ => Tag::Added,
            };
            rows.push(Row { before, after, tag });
        }
        removed.clear();
        added.clear();
    };
    while i < n || j < m {
        if i < n && j < m && old[i] == new[j] {
            flush(&mut rows, &mut removed, &mut added);
            rows.push(Row {
                before: Some(printable(old[i])),
                after: Some(printable(new[j])),
                tag: Tag::Same,
            });
            i += 1;
            j += 1;
        } else if j < m && (i == n || table[i][j + 1] >= table[i + 1][j]) {
            added.push(new[j]);
            j += 1;
        } else {
            removed.push(old[i]);
            i += 1;
        }
    }
    flush(&mut rows, &mut removed, &mut added);
    let changed = rows.iter().any(|row| row.tag != Tag::Same);
    Some(Diff {
        path: printable(path),
        rows,
        changed,
    })
}

impl Diff {
    /// The rows that differ, each with up to `context` unchanged rows around
    /// it; None between runs marks the rows left out.
    pub fn hunks(&self, context: usize) -> Vec<Option<&Row>> {
        let keep: Vec<bool> = (0..self.rows.len())
            .map(|index| {
                let low = index.saturating_sub(context);
                let high = (index + context + 1).min(self.rows.len());
                self.rows[low..high].iter().any(|row| row.tag != Tag::Same)
            })
            .collect();
        let mut shown = Vec::new();
        let mut gap = false;
        for (index, row) in self.rows.iter().enumerate() {
            if keep[index] {
                if gap && !shown.is_empty() {
                    shown.push(None);
                }
                gap = false;
                shown.push(Some(row));
            } else {
                gap = true;
            }
        }
        shown
    }

    /// The differing rows as a unified diff, for the command line.
    pub fn unified(&self) -> String {
        let mut text = format!("--- {} (now)\n+++ {} (after)\n", self.path, self.path);
        for row in self.hunks(2) {
            match row {
                None => text.push_str("@@\n"),
                Some(row) => match row.tag {
                    Tag::Same => {
                        text.push_str(&format!(" {}\n", row.before.as_deref().unwrap_or("")))
                    }
                    _ => {
                        if let Some(before) = &row.before {
                            text.push_str(&format!("-{before}\n"));
                        }
                        if let Some(after) = &row.after {
                            text.push_str(&format!("+{after}\n"));
                        }
                    }
                },
            }
        }
        text
    }
}

/// Run a renderer from the installers' folder: `python3 configure.py ARGS`,
/// which only prints. `scripts` is the unpacked scripts folder.
pub fn render_with(scripts: &Path, arguments: &[String]) -> Result<String, String> {
    if !arguments
        .first()
        .is_some_and(|action| helixforge_core::RENDERERS.contains(&action.as_str()))
    {
        return Err("not a renderer".to_string());
    }
    let output = std::process::Command::new("python3")
        .arg("-B")
        .arg("./configure.py")
        .args(arguments)
        .current_dir(scripts.join("installers/commander"))
        .env("LC_ALL", "C.UTF-8")
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("cannot run python3: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helixforge_catalog::Catalog;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        commands: HashMap<String, String>,
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
        fn on_path(&self, _: &str) -> bool {
            false
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    fn arch() -> LinuxInfo {
        LinuxInfo {
            id: "arch".into(),
            pretty_name: "Arch Linux".into(),
            id_like: vec![],
            manager: Some(PackageManager::Arch),
        }
    }

    #[test]
    fn diffs_pair_changed_lines_and_keep_context() {
        let found = diff("f", "a\nb\nc\nd\ne\nf\ng\n", "a\nB\nc\nd\ne\nf\ng\nh\n").unwrap();
        assert!(found.changed);
        let tags: Vec<Tag> = found.rows.iter().map(|row| row.tag).collect();
        assert_eq!(tags[1], Tag::Changed);
        assert_eq!(*tags.last().unwrap(), Tag::Added);
        let shown = found.hunks(1);
        assert!(
            shown.contains(&None),
            "rows between the changes are left out"
        );
        let unified = found.unified();
        assert!(unified.contains("-b\n+B\n"), "{unified}");
        assert!(unified.contains("+h\n"));
        assert!(!diff("f", "same\n", "same\n").unwrap().changed);
        // Escape sequences in a file never reach the terminal.
        let evil = diff("f", "", "\u{1b}[2J\n").unwrap();
        assert!(!evil.rows[0].after.as_ref().unwrap().contains('\u{1b}'));
    }

    #[test]
    fn a_grub_theme_shows_its_new_settings_and_what_cannot_be_known() {
        let catalog = Catalog::builtin(false).unwrap();
        let entry = catalog.find("grub-dracula").unwrap();
        let mut fake = Fake::default();
        fake.files.insert(
            PathBuf::from("/etc/default/grub"),
            "GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet\"\n".into(),
        );
        let render = |arguments: &[String]| -> Result<String, String> {
            assert_eq!(arguments[0], "grub");
            assert_eq!(arguments[1], "/etc/default/grub");
            assert!(arguments[2].starts_with("/boot/grub/themes/commander-dracula-"));
            Ok(format!(
                "GRUB_TIMEOUT=5\nGRUB_CMDLINE_LINUX_DEFAULT=\"quiet\"\nGRUB_THEME=\"{}\"\n",
                arguments[2]
            ))
        };
        let look = Look {
            system: &fake,
            render: &render,
        };
        let planned = crate::plan_with(
            [entry],
            &arch(),
            &|id| catalog.find(id),
            crate::Machine {
                holds: &|_| false,
                on_path: &|_| false,
            },
        )
        .unwrap();
        let found = preview(&planned, &|id| catalog.find(id), &arch(), &look);
        assert_eq!(found.len(), 1);
        let changes = &found[0].changes;
        let grub = changes
            .iter()
            .find(|change| change.text.starts_with("/etc/default/grub"))
            .unwrap();
        assert_eq!(grub.certainty, Certainty::Known);
        let diff = grub.diff.as_ref().unwrap();
        assert!(diff.changed);
        assert!(diff.unified().contains("+GRUB_THEME="));
        assert!(changes.iter().any(|change| change.area == Area::Boot));
        assert!(
            changes
                .iter()
                .any(|change| change.certainty == Certainty::Unknown
                    && change.text.contains("grub-mkconfig"))
        );
        assert!(
            changes
                .iter()
                .any(|change| change.text.contains("<date and time of the run>"))
        );
    }

    #[test]
    fn packages_list_what_the_package_manager_would_add() {
        let catalog = Catalog::builtin(false).unwrap();
        let mut fake = Fake::default();
        fake.commands.insert(
            "pacman -Sp --print-format %n %v neovim".into(),
            "luajit 2.1-1\nneovim 0.11.0-1\n".into(),
        );
        let render = |_: &[String]| Err("none".to_string());
        let look = Look {
            system: &fake,
            render: &render,
        };
        let planned = crate::plan_with(
            [catalog.find("neovim").unwrap()],
            &arch(),
            &|id| catalog.find(id),
            crate::Machine {
                holds: &|_| false,
                on_path: &|_| false,
            },
        )
        .unwrap();
        let found = preview(&planned, &|id| catalog.find(id), &arch(), &look);
        let texts: Vec<&str> = found[0]
            .changes
            .iter()
            .map(|change| change.text.as_str())
            .collect();
        assert!(
            texts[0].starts_with("installs neovim from pacman"),
            "{texts:?}"
        );
        assert!(
            texts
                .iter()
                .any(|text| text.contains("also install: luajit 2.1-1")),
            "{texts:?}"
        );
    }

    #[test]
    fn a_restore_shows_the_file_it_puts_back() {
        let mut fake = Fake::default();
        fake.files
            .insert(PathBuf::from("/home/me/.config/x.conf"), "new\n".into());
        fake.files
            .insert(PathBuf::from("/home/me/.config/x.conf.bak"), "old\n".into());
        let backup = crate::backups::Backup {
            original: PathBuf::from("/home/me/.config/x.conf"),
            backup: PathBuf::from("/home/me/.config/x.conf.bak"),
            taken: None,
            system: false,
            directory: false,
        };
        let render = |_: &[String]| Err("none".to_string());
        let look = Look {
            system: &fake,
            render: &render,
        };
        let found = preview(
            &crate::backups::plan_restore(&backup),
            &|_| None,
            &arch(),
            &look,
        );
        let diff = found[0].changes[0].diff.as_ref().unwrap();
        assert_eq!(diff.rows[0].before.as_deref(), Some("new"));
        assert_eq!(diff.rows[0].after.as_deref(), Some("old"));
    }
}
