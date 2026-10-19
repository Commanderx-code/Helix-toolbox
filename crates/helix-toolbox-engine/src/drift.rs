//! HELIX Drift: how a machine has moved away from a saved baseline or a
//! workstation profile. A baseline records what was installed, the
//! configuration files catalog entries write (by checksum, never their
//! contents), a few machine settings and the entries Helix Toolbox ran there.
//! Comparing names what is missing, changed or new, and which entry would
//! put a missing part back; nothing here changes the machine.

use crate::{
    doctor::System,
    history,
    inventory::{Inventory, Status},
};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, printable, valid_id};
use helix_toolbox_platform::LinuxInfo;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Component, Path, PathBuf},
};

/// The baseline file format this version writes.
pub const FORMAT: u32 = 1;

/// Limits on what a shared baseline may ask a comparison to do.
const MAX_FILE: u64 = 4 << 20;
const MAX_RECORDS: usize = 10_000;
const MAX_FILES: usize = 1_000;

/// A saved state of a machine.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Baseline {
    #[serde(default = "format")]
    pub format: u32,
    pub name: String,
    /// When it was taken, in Unix seconds.
    #[serde(default)]
    pub taken: u64,
    /// The distribution it was taken on.
    #[serde(default)]
    pub system: String,
    /// Installed entries.
    #[serde(default, rename = "entry", skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<Recorded>,
    /// Entries Helix Toolbox ran successfully there that leave no package.
    #[serde(default, rename = "applied", skip_serializing_if = "Vec::is_empty")]
    pub applied: Vec<Applied>,
    /// Configuration files and folders, by checksum.
    #[serde(default, rename = "file", skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileRecord>,
    /// Machine settings: the login shell, display manager, repositories.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, String>,
}

fn format() -> u32 {
    FORMAT
}

/// An installed entry: its id, and where from and which version.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Recorded {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// An entry that ran, and when (Unix seconds).
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct Applied {
    pub id: String,
    #[serde(default)]
    pub when: u64,
}

/// One configuration path as it was.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileRecord {
    /// With `~` for the home folder.
    pub path: String,
    pub kind: FileKind,
    /// The file's checksum, or the folder's (names, kinds and contents).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// A link's target.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// The entry that writes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FileKind {
    #[default]
    File,
    Directory,
    Link,
    Missing,
}

impl FileKind {
    fn label(self) -> &'static str {
        match self {
            FileKind::File => "a file",
            FileKind::Directory => "a folder",
            FileKind::Link => "a link",
            FileKind::Missing => "missing",
        }
    }
}

/// What kind of difference.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// An application the baseline had, or the profile names, is not installed.
    Missing,
    /// A configuration path is gone.
    FileMissing,
    /// An entry ran there and has not run here.
    NotApplied,
    /// A configuration path is something else now: a file became a link.
    FileReplaced,
    /// A configuration file or folder has other contents.
    FileChanged,
    /// A machine setting differs.
    Setting,
    /// Installed here and not in the baseline.
    Added,
    /// Installed in both, at another version or from another source.
    Version,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Missing | Kind::FileMissing => "Missing",
            Kind::NotApplied => "Not run",
            Kind::FileReplaced => "Replaced",
            Kind::FileChanged => "Changed",
            Kind::Setting => "Setting",
            Kind::Added => "Added",
            Kind::Version => "Version",
        }
    }

    /// What the difference means, for its details.
    pub fn explain(self) -> &'static str {
        match self {
            Kind::Missing => {
                "The application was installed on the baseline machine, or the profile names it, and is not installed here."
            }
            Kind::FileMissing => "The configuration file or folder was there, and is not here.",
            Kind::NotApplied => {
                "Helix Toolbox ran this entry successfully on the baseline machine (or the profile names it), and its history here has no successful run. It leaves no package to look for. Where the files it writes, or the settings its installer renders, show it in place, it counts as set up; otherwise it may still have been set up another way, or before Helix Toolbox 1.1 recorded runs."
            }
            Kind::FileReplaced => {
                "The path is another kind of thing now: a file became a link or a folder. A link into the Nix store means Home Manager manages it."
            }
            Kind::FileChanged => {
                "The file or folder has other contents than when the baseline was saved. Only checksums are kept, so Helix Toolbox cannot show what changed."
            }
            Kind::Setting => {
                "A machine setting differs: the login shell, the display manager, the Arch repositories, Flatpak's remotes or the desktop."
            }
            Kind::Added => "Installed here, and not on the baseline machine.",
            Kind::Version => "Installed in both, at another version or from another source.",
        }
    }

    /// Whether this is part of the setup that is not here.
    pub fn missing(self) -> bool {
        matches!(self, Kind::Missing | Kind::FileMissing | Kind::NotApplied)
    }
}

/// One difference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Difference {
    pub kind: Kind,
    /// The entry's name, the path or the setting.
    pub subject: String,
    pub was: Option<String>,
    pub now: Option<String>,
    /// The entry that would put it back, or installs it.
    pub entry: Option<String>,
    /// One sentence about it.
    pub note: String,
}

/// What a comparison found.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Report {
    /// What it was compared with.
    pub against: String,
    /// Differences, most important first.
    pub differences: Vec<Difference>,
    /// How many things were the same.
    pub matched: usize,
    /// Entry ids the baseline or profile names that this catalog lacks.
    pub unknown: Vec<String>,
}

impl Report {
    /// The entries that would put back what is missing, in order, once each.
    pub fn restore(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for difference in &self.differences {
            if let Some(id) = &difference.entry
                && difference.kind.missing()
                && !ids.contains(id)
            {
                ids.push(id.clone());
            }
        }
        ids
    }

    /// How many differences of each label, for a summary line.
    pub fn counts(&self) -> Vec<(&'static str, usize)> {
        let mut counts: Vec<(&'static str, usize)> = Vec::new();
        for difference in &self.differences {
            let label = difference.kind.label();
            match counts.iter_mut().find(|(name, _)| *name == label) {
                Some((_, count)) => *count += 1,
                None => counts.push((label, 1)),
            }
        }
        counts
    }
}

/// Why a baseline cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, String),
    #[error("{0} is not a Helix Toolbox baseline: {1}")]
    Format(PathBuf, String),
    #[error(
        "{0} was written by a later Helix Toolbox (format {1}); update Helix Toolbox to read it"
    )]
    Later(PathBuf, u32),
    #[error("the baseline contains control characters")]
    Control,
    #[error("the baseline records more than a baseline can")]
    TooLarge,
    #[error("the baseline names {0}, which is not an entry id")]
    Id(String),
    #[error("the baseline names a path Helix Toolbox does not compare: {0}")]
    Path(String),
    #[error("the name {0:?} gives no usable file name; use letters or digits")]
    Name(String),
    #[error("cannot write {0}: {1}")]
    Write(PathBuf, String),
}

/// Everything a capture or a comparison reads; tests supply their own.
pub struct Sources<'a> {
    pub catalog: &'a Catalog,
    pub platform: &'a LinuxInfo,
    pub inventory: &'a Inventory,
    pub on_path: &'a dyn Fn(&str) -> Option<PathBuf>,
    pub system: &'a dyn System,
    pub home: &'a Path,
    /// The runs recorded here, newest first.
    pub history: &'a [history::Record],
    /// The machine's settings, as a capture records them.
    pub settings: &'a dyn Fn() -> BTreeMap<String, String>,
    /// The installers' renderers (`configure.py`), which only print: how an
    /// entry that ran before its runs were recorded is seen to be in place.
    pub render: &'a dyn Fn(&[String]) -> Result<String, String>,
}

/// `$XDG_CONFIG_HOME/helix-toolbox/baselines`, or under `~/.config`.
pub fn dir() -> Option<PathBuf> {
    crate::profiles::dir().and_then(|profiles| Some(profiles.parent()?.join("baselines")))
}

/// Record this machine as a baseline. `extra` adds paths of the user's own
/// to the configuration files the catalog's entries write.
pub fn capture(name: &str, taken: u64, sources: &Sources, extra: &[String]) -> Baseline {
    let mut baseline = Baseline {
        format: FORMAT,
        name: name.trim().to_string(),
        taken,
        system: sources.platform.pretty_name.clone(),
        settings: (sources.settings)(),
        ..Baseline::default()
    };
    for entry in sources.catalog.entries() {
        if baseline.entries.iter().any(|known| known.id == entry.id) {
            continue;
        }
        if let Some(status) = sources
            .inventory
            .status(entry, sources.platform, sources.on_path)
            && let Some((source, version)) = installed(&status)
        {
            baseline.entries.push(Recorded {
                id: entry.id.clone(),
                source: Some(source),
                version,
            });
        }
    }
    baseline.applied = applied_here(sources);
    let mut paths: Vec<(String, Option<String>)> = Vec::new();
    for entry in sources.catalog.entries() {
        for written in &entry.writes {
            if !paths.iter().any(|(path, _)| path == written) {
                paths.push((written.clone(), Some(entry.id.clone())));
            }
        }
    }
    for path in extra {
        let shown = tilde(sources.home, path);
        if !paths.iter().any(|(known, _)| *known == shown) {
            paths.push((shown, None));
        }
    }
    for (path, entry) in paths {
        let Some(full) = expand(sources.home, &path) else {
            continue;
        };
        let (kind, sha256, target) = look(&full);
        // Paths no entry has written are left out, unless the user named them.
        if kind == FileKind::Missing && entry.is_some() {
            continue;
        }
        baseline.files.push(FileRecord {
            path,
            kind,
            sha256,
            target,
            entry,
        });
    }
    baseline
}

/// The installed state of a status as (source, version), or None when it
/// is not installed.
fn installed(status: &Status) -> Option<(String, Option<String>)> {
    match status {
        Status::Installed {
            source, version, ..
        }
        | Status::Update {
            source, version, ..
        } => Some((source.clone(), version.clone())),
        Status::Managed { by, version } => Some((by.clone(), version.clone())),
        Status::Elsewhere { .. } => Some(("another install".to_string(), None)),
        Status::Absent { .. } => None,
    }
}

/// Entries without a package that Helix Toolbox ran successfully here, newest
/// run first: configurations and tweaks. Reports and profiles are left out.
fn applied_here(sources: &Sources) -> Vec<Applied> {
    let mut applied: Vec<Applied> = Vec::new();
    for record in sources.history.iter().filter(|record| record.success) {
        for id in &record.entries {
            if applied.iter().any(|known| known.id == *id) {
                continue;
            }
            if sources
                .catalog
                .find(id)
                .is_some_and(|entry| leaves_no_package(sources.catalog, entry))
            {
                applied.push(Applied {
                    id: id.clone(),
                    when: record.unix_timestamp,
                });
            }
        }
    }
    applied
}

/// Whether what an entry sets up is here, though no run of it is recorded:
/// it ran before Helix Toolbox recorded runs (v1.1.0), or was set up by hand.
/// The files it writes are all there, or the installer's own renderer finds
/// its settings in place. `None` when nothing tells.
fn set_up_here(entry: &Entry, sources: &Sources) -> Option<bool> {
    if let Some(rendered) = crate::preview::rendered_in_place(entry, sources.system, sources.render)
    {
        return Some(rendered);
    }
    if entry.writes.is_empty() {
        return None;
    }
    Some(entry.writes.iter().all(|path| {
        expand(sources.home, path).is_some_and(|full| look(&full).0 != FileKind::Missing)
    }))
}

/// Whether an entry changes the machine without a package to show for it.
fn leaves_no_package(catalog: &Catalog, entry: &Entry) -> bool {
    let report = catalog
        .tabs
        .iter()
        .any(|tab| tab.id == "doctor" && tab.entries().iter().any(|other| other.id == entry.id));
    entry.identity().is_none()
        && !report
        && matches!(entry.action, Action::Script { .. } | Action::Command { .. })
}

/// Compare the machine with a baseline.
pub fn compare(baseline: &Baseline, sources: &Sources) -> Report {
    let mut report = Report {
        against: printable(&baseline.name),
        ..Report::default()
    };
    let name = |id: &str| {
        sources
            .catalog
            .find(id)
            .map(|entry| entry.name.clone())
            .unwrap_or_else(|| printable(id))
    };
    // Applications.
    for recorded in &baseline.entries {
        let Some(entry) = sources.catalog.find(&recorded.id) else {
            report.unknown.push(printable(&recorded.id));
            continue;
        };
        let was = shown(recorded.source.as_deref(), recorded.version.as_deref());
        let status = sources
            .inventory
            .status(entry, sources.platform, sources.on_path);
        match status.as_ref().and_then(installed) {
            None => report.differences.push(Difference {
                kind: Kind::Missing,
                subject: entry.name.clone(),
                was: Some(was.clone()),
                now: None,
                entry: Some(entry.id.clone()),
                note: format!("was installed ({was}); not here"),
            }),
            Some((source, version)) => {
                let now = shown(Some(&source), version.as_deref());
                let moved = recorded.source.as_deref().is_some_and(|was| was != source);
                let changed = match (&recorded.version, &version) {
                    (Some(was), Some(now)) => was != now,
                    _ => false,
                };
                if moved || changed {
                    report.differences.push(Difference {
                        kind: Kind::Version,
                        subject: entry.name.clone(),
                        was: Some(was.clone()),
                        now: Some(now.clone()),
                        entry: None,
                        note: format!("{was} there, {now} here"),
                    });
                } else {
                    report.matched += 1;
                }
            }
        }
    }
    for entry in sources.catalog.entries() {
        if baseline.entries.iter().any(|known| known.id == entry.id)
            || report.differences.iter().any(|known| {
                known.kind == Kind::Added && known.entry.as_deref() == Some(entry.id.as_str())
            })
        {
            continue;
        }
        if let Some(status) = sources
            .inventory
            .status(entry, sources.platform, sources.on_path)
            && let Some((source, version)) = installed(&status)
        {
            let now = shown(Some(&source), version.as_deref());
            report.differences.push(Difference {
                kind: Kind::Added,
                subject: entry.name.clone(),
                was: None,
                now: Some(now.clone()),
                entry: Some(entry.id.clone()),
                note: format!("installed here ({now}); not in the baseline"),
            });
        }
    }
    // Entries that ran there.
    let here = applied_here(sources);
    for applied in &baseline.applied {
        if sources.catalog.find(&applied.id).is_none() {
            report.unknown.push(printable(&applied.id));
            continue;
        }
        if here.iter().any(|known| known.id == applied.id) {
            report.matched += 1;
            continue;
        }
        let evidence = sources
            .catalog
            .find(&applied.id)
            .and_then(|entry| set_up_here(entry, sources));
        if evidence == Some(true) {
            report.matched += 1;
            continue;
        }
        report.differences.push(Difference {
            kind: Kind::NotApplied,
            subject: name(&applied.id),
            was: Some("ran".to_string()),
            now: None,
            entry: Some(applied.id.clone()),
            note: not_run_note("ran on the baseline machine", evidence),
        });
    }
    // Configuration files.
    for record in &baseline.files {
        let Some(full) = expand(sources.home, &record.path) else {
            continue;
        };
        let (kind, sha256, target) = look(&full);
        let path = printable(&record.path);
        let writer = record
            .entry
            .as_deref()
            .filter(|id| sources.catalog.find(id).is_some())
            .map(String::from);
        let by = writer
            .as_deref()
            .map(|id| format!(" ({} writes it)", name(id)))
            .unwrap_or_default();
        let difference = |kind, note: String| Difference {
            kind,
            subject: path.clone(),
            was: Some(record.kind.label().to_string()),
            now: None,
            entry: writer.clone(),
            note,
        };
        if kind == record.kind {
            let same = match kind {
                FileKind::Link => same_target(record.target.as_deref(), target.as_deref()),
                FileKind::File | FileKind::Directory => {
                    record.sha256.is_none() || sha256.is_none() || record.sha256 == sha256
                }
                FileKind::Missing => true,
            };
            if same {
                report.matched += 1;
                continue;
            }
            let note = if kind == FileKind::Link {
                format!(
                    "the link points somewhere else: {} → {}",
                    printable(record.target.as_deref().unwrap_or("?")),
                    printable(target.as_deref().unwrap_or("?"))
                )
            } else {
                format!("other contents{by}")
            };
            report.differences.push(Difference {
                now: Some(kind.label().to_string()),
                ..difference(Kind::FileChanged, note)
            });
        } else if kind == FileKind::Missing {
            report.differences.push(difference(
                Kind::FileMissing,
                format!("was {}; gone{by}", record.kind.label()),
            ));
        } else {
            let now = match (&kind, &target) {
                (FileKind::Link, Some(target)) if target.starts_with("/nix/store/") => {
                    "a link into the Nix store (Home Manager)".to_string()
                }
                (FileKind::Link, Some(target)) => format!("a link to {}", printable(target)),
                _ => kind.label().to_string(),
            };
            report.differences.push(Difference {
                now: Some(now.clone()),
                ..difference(
                    Kind::FileReplaced,
                    format!("was {}, now {now}", record.kind.label()),
                )
            });
        }
    }
    // Settings, where both sides know them.
    let settings = (sources.settings)();
    for (key, was) in &baseline.settings {
        let label = setting_label(key);
        match settings.get(key) {
            Some(now) if now == was => report.matched += 1,
            Some(now) => report.differences.push(Difference {
                kind: Kind::Setting,
                subject: label.clone(),
                was: Some(printable(was)),
                now: Some(printable(now)),
                entry: None,
                note: format!("{} there, {} here", printable(was), printable(now)),
            }),
            None if key == "desktop" => {}
            None => report.differences.push(Difference {
                kind: Kind::Setting,
                subject: label.clone(),
                was: Some(printable(was)),
                now: None,
                entry: None,
                note: format!("{} there; not set here", printable(was)),
            }),
        }
    }
    report.differences.sort_by_key(|difference| difference.kind);
    report.unknown.sort();
    report.unknown.dedup();
    report
}

/// Compare the machine with a profile: whether each entry it names is
/// installed, its files are there, or Helix Toolbox has run it here.
pub fn compare_profile(name: &str, ids: &[String], sources: &Sources) -> Report {
    let mut report = Report {
        against: printable(name),
        ..Report::default()
    };
    let mut seen: Vec<String> = Vec::new();
    let mut queue: Vec<String> = ids.to_vec();
    queue.reverse();
    let here = applied_here(sources);
    while let Some(id) = queue.pop() {
        if seen.contains(&id) {
            continue;
        }
        seen.push(id.clone());
        let Some(entry) = sources.catalog.find(&id) else {
            report.unknown.push(printable(&id));
            continue;
        };
        if let Action::Profile { entries } = &entry.action {
            queue.extend(entries.iter().rev().cloned());
            continue;
        }
        if entry.identity().is_some() {
            let status = sources
                .inventory
                .status(entry, sources.platform, sources.on_path);
            if status.as_ref().and_then(installed).is_some() {
                report.matched += 1;
            } else {
                report.differences.push(Difference {
                    kind: Kind::Missing,
                    subject: entry.name.clone(),
                    was: None,
                    now: None,
                    entry: Some(entry.id.clone()),
                    note: "not installed".to_string(),
                });
            }
            continue;
        }
        if !entry.writes.is_empty() {
            let missing: Vec<&String> = entry
                .writes
                .iter()
                .filter(|path| {
                    expand(sources.home, path).is_none_or(|full| look(&full).0 == FileKind::Missing)
                })
                .collect();
            if missing.is_empty() {
                report.matched += 1;
            } else {
                for path in missing {
                    report.differences.push(Difference {
                        kind: Kind::FileMissing,
                        subject: printable(path),
                        was: None,
                        now: None,
                        entry: Some(entry.id.clone()),
                        note: format!("not here ({} writes it)", entry.name),
                    });
                }
            }
            continue;
        }
        let evidence = set_up_here(entry, sources);
        if here.iter().any(|known| known.id == entry.id) || evidence == Some(true) {
            report.matched += 1;
        } else {
            report.differences.push(Difference {
                kind: Kind::NotApplied,
                subject: entry.name.clone(),
                was: None,
                now: None,
                entry: Some(entry.id.clone()),
                note: not_run_note("the profile names it", evidence),
            });
        }
    }
    report.differences.sort_by_key(|difference| difference.kind);
    report.unknown.sort();
    report.unknown.dedup();
    report
}

/// Why an entry counts as not run here.
fn not_run_note(there: &str, evidence: Option<bool>) -> String {
    match evidence {
        Some(_) => format!("{there}; no run of it here, and what it sets is not in place"),
        None => format!(
            "{there}; no run of it recorded here, and nothing to check it by (runs before Helix Toolbox 1.1 were not recorded)"
        ),
    }
}

/// The settings a baseline records, read from this machine.
pub fn settings_here(system: &dyn System) -> BTreeMap<String, String> {
    let mut settings = BTreeMap::new();
    let (uid, _, _) = system.user();
    if let Some(passwd) = system.read(Path::new("/etc/passwd")) {
        let shell = passwd.lines().find_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            (fields.len() >= 7 && fields[2].parse::<u32>().ok() == Some(uid))
                .then(|| fields[6].to_string())
        });
        if let Some(shell) = shell.filter(|shell| !shell.is_empty()) {
            settings.insert("shell".to_string(), shell);
        }
    }
    if let Some(target) = system.link(Path::new("/etc/systemd/system/display-manager.service"))
        && let Some(stem) = target.file_stem()
    {
        settings.insert(
            "display-manager".to_string(),
            stem.to_string_lossy().into_owned(),
        );
    }
    if let Some(repositories) = system
        .read(Path::new("/etc/pacman.conf"))
        .map(|text| crate::preflight::repositories_in(&text))
        .filter(|repositories| !repositories.is_empty())
    {
        settings.insert("repositories".to_string(), repositories.join(", "));
    }
    if system.on_path("flatpak")
        && let Some((true, text)) = system.run(&["flatpak", "remotes", "--columns=name"])
    {
        let mut remotes: Vec<&str> = text
            .lines()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        remotes.sort_unstable();
        remotes.dedup();
        if !remotes.is_empty() {
            settings.insert("flatpak-remotes".to_string(), remotes.join(", "));
        }
    }
    if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP")
        && !desktop.is_empty()
    {
        settings.insert("desktop".to_string(), desktop);
    }
    settings
}

fn setting_label(key: &str) -> String {
    match key {
        "shell" => "Login shell".to_string(),
        "display-manager" => "Display manager".to_string(),
        "repositories" => "Arch repositories".to_string(),
        "flatpak-remotes" => "Flatpak remotes".to_string(),
        "desktop" => "Desktop".to_string(),
        other => printable(other),
    }
}

fn shown(source: Option<&str>, version: Option<&str>) -> String {
    match (source, version) {
        (Some(source), Some(version)) => printable(&format!("{source} {version}")),
        (Some(source), None) => printable(source),
        (None, Some(version)) => printable(version),
        (None, None) => "installed".to_string(),
    }
}

/// Links into the Nix store count as the same: Home Manager points them at
/// a new store path on every switch.
fn same_target(was: Option<&str>, now: Option<&str>) -> bool {
    match (was, now) {
        (Some(was), Some(now)) => {
            was == now || (was.starts_with("/nix/store/") && now.starts_with("/nix/store/"))
        }
        (was, now) => was == now,
    }
}

/// A path with `~` for the home folder.
pub(crate) fn tilde(home: &Path, path: &str) -> String {
    match Path::new(path).strip_prefix(home) {
        Ok(rest) if !home.as_os_str().is_empty() => {
            if rest.as_os_str().is_empty() {
                "~".to_string()
            } else {
                format!("~/{}", rest.display())
            }
        }
        _ => path.to_string(),
    }
}

/// `~` expanded; None for a path Helix Toolbox does not compare: relative, or
/// one that climbs with `..`.
fn expand(home: &Path, path: &str) -> Option<PathBuf> {
    let full = match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if path == "~" => home.to_path_buf(),
        None => PathBuf::from(path),
    };
    let plain = full.is_absolute()
        && full
            .components()
            .all(|component| !matches!(component, Component::ParentDir | Component::CurDir));
    plain.then_some(full)
}

/// Whether a path is one a baseline may name.
pub fn valid_path(path: &str) -> bool {
    !path.chars().any(char::is_control)
        && path.len() <= 4096
        && (path.starts_with("~/") || path.starts_with('/'))
        && expand(Path::new("/home/user"), path).is_some()
}

/// A folder tree larger than this is compared by its presence only.
const TREE_FILES: usize = 5_000;
const TREE_BYTES: u64 = 256 << 20;
const TREE_DEPTH: usize = 64;

/// What a path is now: its kind, its checksum and a link's target.
fn look(path: &Path) -> (FileKind, Option<String>, Option<String>) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return (FileKind::Missing, None, None);
    };
    if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(path)
            .ok()
            .map(|target| target.to_string_lossy().into_owned());
        return (FileKind::Link, None, target);
    }
    if metadata.is_dir() {
        let mut hasher = Sha256::new();
        let mut budget = (TREE_FILES, TREE_BYTES);
        let sum =
            tree(path, Path::new(""), &mut hasher, &mut budget).then(|| hex(&hasher.finalize()));
        return (FileKind::Directory, sum, None);
    }
    // A device, a pipe or a socket is compared by its presence only: reading
    // one could block or never end.
    let sum = metadata
        .is_file()
        .then(|| file_sum(path, TREE_BYTES))
        .flatten();
    (FileKind::File, sum, None)
}

/// A regular file's checksum, reading at most `limit` bytes; None for
/// anything else, or a file larger than that. The file is opened without
/// following a link or waiting on a pipe, and checked once open, so a path
/// swapped after it was looked at is not read either.
fn file_sum(path: &Path, limit: u64) -> Option<String> {
    use std::{io::Read, os::unix::fs::OpenOptionsExt};
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    if !metadata.is_file() || metadata.len() > limit {
        return None;
    }
    let mut hasher = Sha256::new();
    let read = std::io::copy(&mut file.take(limit + 1), &mut hasher).ok()?;
    (read <= limit).then(|| hex(&hasher.finalize()))
}

/// Hash a folder's names, kinds and contents in a fixed order; false when it
/// is too large or unreadable to compare.
fn tree(root: &Path, relative: &Path, hasher: &mut Sha256, budget: &mut (usize, u64)) -> bool {
    if relative.components().count() > TREE_DEPTH {
        return false;
    }
    let Ok(listing) = std::fs::read_dir(root.join(relative)) else {
        return false;
    };
    let mut names: Vec<std::ffi::OsString> =
        listing.flatten().map(|item| item.file_name()).collect();
    names.sort();
    for name in names {
        if budget.0 == 0 {
            return false;
        }
        budget.0 -= 1;
        let child = relative.join(&name);
        let full = root.join(&child);
        let Ok(metadata) = std::fs::symlink_metadata(&full) else {
            return false;
        };
        hasher.update(child.to_string_lossy().as_bytes());
        hasher.update([0]);
        if metadata.file_type().is_symlink() {
            hasher.update(b"link ");
            if let Ok(target) = std::fs::read_link(&full) {
                hasher.update(target.to_string_lossy().as_bytes());
            }
        } else if metadata.is_dir() {
            hasher.update(b"dir");
            if !tree(root, &child, hasher, budget) {
                return false;
            }
        } else if metadata.is_file() {
            if metadata.len() > budget.1 {
                return false;
            }
            hasher.update(b"file ");
            match file_sum(&full, budget.1) {
                Some(sum) => hasher.update(sum.as_bytes()),
                None => return false,
            }
            budget.1 -= metadata.len();
        } else {
            // Devices, pipes and sockets count by name only.
            hasher.update(b"special");
        }
        hasher.update([0]);
    }
    true
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Read and check a baseline file.
pub fn read(path: &Path) -> Result<Baseline, BaselineError> {
    use std::io::Read;
    let failed = |error: String| BaselineError::Read(path.to_path_buf(), error);
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| file.take(MAX_FILE + 1).read_to_string(&mut text))
        .map_err(|error| failed(error.to_string()))?;
    if text.len() as u64 > MAX_FILE {
        return Err(failed("larger than a baseline can be".to_string()));
    }
    let baseline: Baseline = toml::from_str(&text)
        .map_err(|error| BaselineError::Format(path.to_path_buf(), printable(error.message())))?;
    if baseline.format > FORMAT {
        return Err(BaselineError::Later(path.to_path_buf(), baseline.format));
    }
    check(&baseline)?;
    Ok(baseline)
}

/// Plain text throughout, ids that are ids and paths Helix Toolbox compares: a
/// shared file could otherwise carry escape sequences into the terminal.
pub fn check(baseline: &Baseline) -> Result<(), BaselineError> {
    let control = |text: &str| text.chars().any(char::is_control);
    let mut texts: Vec<&str> = vec![&baseline.name, &baseline.system];
    for recorded in &baseline.entries {
        texts.extend(recorded.source.as_deref());
        texts.extend(recorded.version.as_deref());
    }
    for record in &baseline.files {
        texts.extend(record.sha256.as_deref());
        texts.extend(record.target.as_deref());
    }
    for (key, value) in &baseline.settings {
        texts.push(key);
        texts.push(value);
    }
    if texts.into_iter().any(control) {
        return Err(BaselineError::Control);
    }
    if baseline.entries.len() > MAX_RECORDS
        || baseline.applied.len() > MAX_RECORDS
        || baseline.files.len() > MAX_FILES
        || baseline.settings.len() > MAX_RECORDS
    {
        return Err(BaselineError::TooLarge);
    }
    let ids = baseline
        .entries
        .iter()
        .map(|recorded| &recorded.id)
        .chain(baseline.applied.iter().map(|applied| &applied.id))
        .chain(
            baseline
                .files
                .iter()
                .filter_map(|record| record.entry.as_ref()),
        );
    for id in ids {
        if !valid_id(id) {
            return Err(BaselineError::Id(printable(id)));
        }
    }
    if let Some(record) = baseline
        .files
        .iter()
        .find(|record| !valid_path(&record.path))
    {
        return Err(BaselineError::Path(printable(&record.path)));
    }
    Ok(())
}

/// Write a baseline to a file, whole or not at all.
pub fn write(path: &Path, baseline: &Baseline) -> Result<(), BaselineError> {
    let failed = |error: String| BaselineError::Write(path.to_path_buf(), error);
    check(baseline)?;
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| failed(error.to_string()))?;
    }
    let text = toml::to_string(baseline).map_err(|error| failed(error.to_string()))?;
    let staging = path.with_extension("toml.new");
    std::fs::write(
        &staging,
        format!("# A Helix Toolbox baseline: helix-toolbox drift check <file>\n{text}"),
    )
    .map_err(|error| failed(error.to_string()))?;
    std::fs::rename(&staging, path).map_err(|error| failed(error.to_string()))
}

/// Save a baseline in `directory` under its name's slug, replacing one of
/// the same name. Returns the file.
pub fn save(directory: &Path, baseline: &Baseline) -> Result<PathBuf, BaselineError> {
    let slug = crate::profiles::slug(&baseline.name)
        .ok_or_else(|| BaselineError::Name(baseline.name.clone()))?;
    let path = directory.join(format!("{slug}.toml"));
    write(&path, baseline)?;
    Ok(path)
}

/// The baselines in `directory`, newest first, each with its slug; files
/// that cannot be used are returned as problems.
pub fn load_all(directory: &Path) -> (Vec<(String, Baseline)>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let Ok(listing) = std::fs::read_dir(directory) else {
        return (found, problems);
    };
    for path in listing.flatten().map(|item| item.path()) {
        if path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        let Some(slug) = path
            .file_stem()
            .and_then(|stem| crate::profiles::slug(&stem.to_string_lossy()))
        else {
            continue;
        };
        match read(&path) {
            Ok(baseline) => found.push((slug, baseline)),
            Err(error) => problems.push(error.to_string()),
        }
    }
    found.sort_by(|a, b| b.1.taken.cmp(&a.1.taken).then_with(|| a.0.cmp(&b.0)));
    problems.sort();
    (found, problems)
}

/// Find a saved baseline by its name or slug.
pub fn find(directory: &Path, name: &str) -> Option<(String, Baseline)> {
    let wanted = crate::profiles::slug(name)?;
    load_all(directory)
        .0
        .into_iter()
        .find(|(slug, _)| *slug == wanted)
}

/// Of the entries that would restore what is missing, those this machine
/// can plan, and those it cannot.
pub fn available(
    ids: &[String],
    catalog: &Catalog,
    platform: &LinuxInfo,
) -> (Vec<String>, Vec<String>) {
    let nothing = crate::Machine {
        holds: &|_| false,
        on_path: &|_| false,
    };
    ids.iter().cloned().partition(|id| {
        catalog.find(id).is_some_and(|entry| {
            crate::plan_with([entry], platform, &|id| catalog.find(id), nothing).is_ok()
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doctor::System;
    use helix_toolbox_core::PackageManager;
    use std::collections::{HashMap, HashSet};

    /// A machine whose settings and files come from a temporary home.
    struct Fake {
        home: PathBuf,
        passwd: String,
        files: HashMap<PathBuf, String>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            if path == Path::new("/etc/passwd") {
                return Some(self.passwd.clone());
            }
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path)
        }
        fn list(&self, _: &Path) -> Vec<String> {
            Vec::new()
        }
        fn link(&self, path: &Path) -> Option<PathBuf> {
            (path == Path::new("/etc/systemd/system/display-manager.service"))
                .then(|| PathBuf::from("/usr/lib/systemd/system/sddm.service"))
        }
        fn run(&self, _: &[&str]) -> Option<(bool, String)> {
            None
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
            (1000, 1000, self.home.clone())
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

    fn inventory(packages: &[(&str, &str)]) -> Inventory {
        Inventory::from_parts(
            Some(PackageManager::Arch),
            Some(
                packages
                    .iter()
                    .map(|(name, version)| (name.to_string(), version.to_string()))
                    .collect(),
            ),
            HashMap::new(),
            HashMap::new(),
            HashSet::new(),
        )
    }

    fn record(entries: &[&str], success: bool, when: u64) -> history::Record {
        history::Record {
            unix_timestamp: when,
            entries: entries.iter().map(|id| id.to_string()).collect(),
            names: Vec::new(),
            success,
            result: String::new(),
            log: None,
        }
    }

    fn sources<'a>(
        catalog: &'a Catalog,
        platform: &'a LinuxInfo,
        inventory: &'a Inventory,
        fake: &'a Fake,
        history: &'a [history::Record],
        settings: &'a dyn Fn() -> BTreeMap<String, String>,
    ) -> Sources<'a> {
        Sources {
            catalog,
            platform,
            inventory,
            on_path: &|_| None,
            system: fake,
            home: &fake.home,
            history,
            settings,
            render: &|_| Err("no renderer in these tests".to_string()),
        }
    }

    #[test]
    fn a_fresh_machine_shows_what_the_old_one_had() {
        let catalog = Catalog::builtin(false).unwrap();
        let platform = arch();
        let old_home = tempfile::tempdir().unwrap();
        let ghostty = old_home.path().join(".config/ghostty");
        std::fs::create_dir_all(ghostty.join("themes")).unwrap();
        std::fs::write(ghostty.join("config"), "font-size = 12\n").unwrap();
        std::fs::write(old_home.path().join("notes.txt"), "mine\n").unwrap();
        let old = Fake {
            home: old_home.path().to_path_buf(),
            passwd: "me:x:1000:1000::/home/me:/usr/bin/fish\n".into(),
            files: HashMap::new(),
        };
        let had = inventory(&[("git", "2.47.0-1"), ("ripgrep", "14.1.1-1")]);
        let ran = [
            record(&["pacman-config"], true, 50),
            record(&["pacman-config", "neovim"], false, 45),
            record(&["git"], true, 40),
        ];
        let old_settings = || settings_here(&old);
        let extra = [old_home.path().join("notes.txt").display().to_string()];
        let baseline = capture(
            "Old laptop",
            100,
            &sources(&catalog, &platform, &had, &old, &ran, &old_settings),
            &extra,
        );
        let ids: Vec<&str> = baseline
            .entries
            .iter()
            .map(|known| known.id.as_str())
            .collect();
        assert_eq!(ids, ["git", "ripgrep"]);
        assert_eq!(baseline.entries[0].version.as_deref(), Some("2.47.0-1"));
        assert_eq!(baseline.settings["shell"], "/usr/bin/fish");
        assert_eq!(baseline.settings["display-manager"], "sddm");
        let paths: Vec<&str> = baseline
            .files
            .iter()
            .map(|record| record.path.as_str())
            .collect();
        assert!(paths.contains(&"~/.config/ghostty/config"), "{paths:?}");
        assert!(paths.contains(&"~/notes.txt"), "{paths:?}");
        assert!(
            !paths.iter().any(|path| path.contains("kitty")),
            "{paths:?}"
        );
        assert_eq!(baseline.applied.len(), 1);
        assert_eq!(baseline.applied[0].id, "pacman-config");

        // Saved and read back unchanged.
        let directory = tempfile::tempdir().unwrap();
        let file = save(directory.path(), &baseline).unwrap();
        assert_eq!(file, directory.path().join("old-laptop.toml"));
        assert_eq!(read(&file).unwrap(), baseline);
        assert_eq!(find(directory.path(), "old laptop").unwrap().1, baseline);

        // A fresh machine: only git, another shell, no configuration.
        let new_home = tempfile::tempdir().unwrap();
        let new = Fake {
            home: new_home.path().to_path_buf(),
            passwd: "me:x:1000:1000::/home/me:/bin/bash\n".into(),
            files: HashMap::new(),
        };
        let has = inventory(&[("git", "2.48.0-1"), ("neovim", "0.11.0-1")]);
        let new_settings = || settings_here(&new);
        let report = compare(
            &baseline,
            &sources(&catalog, &platform, &has, &new, &[], &new_settings),
        );
        let by_kind = |kind: Kind| -> Vec<&str> {
            report
                .differences
                .iter()
                .filter(|difference| difference.kind == kind)
                .map(|difference| difference.subject.as_str())
                .collect()
        };
        assert_eq!(by_kind(Kind::Missing), ["Ripgrep"]);
        assert!(by_kind(Kind::FileMissing).contains(&"~/.config/ghostty/config"));
        assert_eq!(by_kind(Kind::Setting), ["Login shell"]);
        assert_eq!(by_kind(Kind::Added), ["Neovim"]);
        assert_eq!(by_kind(Kind::Version), ["Git"]);
        // Most important first, and what restores the missing parts.
        assert_eq!(report.differences[0].kind, Kind::Missing);
        let restore = report.restore();
        assert_eq!(restore[0], "ripgrep");
        assert!(
            restore.contains(&"dotfiles-ghostty".to_string()),
            "{restore:?}"
        );
        assert!(!restore.contains(&"neovim".to_string()));
    }

    #[test]
    fn changed_and_replaced_files_are_told_apart() {
        let catalog = Catalog::builtin(false).unwrap();
        let platform = arch();
        let home = tempfile::tempdir().unwrap();
        let config = home.path().join(".config/kitty");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(config.join("kitty.conf"), "a\n").unwrap();
        let folder = home.path().join("tree");
        std::fs::create_dir_all(folder.join("inner")).unwrap();
        std::fs::write(folder.join("inner/file"), "x").unwrap();
        let fake = Fake {
            home: home.path().to_path_buf(),
            passwd: String::new(),
            files: HashMap::new(),
        };
        let empty = inventory(&[]);
        let none = BTreeMap::new;
        let at = |path: &Path| path.display().to_string();
        let baseline = capture(
            "Before",
            1,
            &sources(&catalog, &platform, &empty, &fake, &[], &none),
            &[
                at(&config.join("kitty.conf")),
                at(&folder),
                at(&home.path().join("gone")),
            ],
        );
        let compare_now = || {
            compare(
                &baseline,
                &sources(&catalog, &platform, &empty, &fake, &[], &none),
            )
        };
        let unchanged = compare_now();
        assert!(unchanged.differences.is_empty(), "{unchanged:?}");
        // A path recorded as missing that is still missing matches.
        assert_eq!(unchanged.matched, 3);

        std::fs::write(folder.join("inner/file"), "y").unwrap();
        std::fs::remove_file(config.join("kitty.conf")).unwrap();
        std::os::unix::fs::symlink(
            "/nix/store/abc-home-manager-files/kitty.conf",
            config.join("kitty.conf"),
        )
        .unwrap();
        let report = compare_now();
        let kinds: Vec<(Kind, &str)> = report
            .differences
            .iter()
            .map(|difference| (difference.kind, difference.subject.as_str()))
            .collect();
        assert_eq!(
            kinds,
            [
                (Kind::FileReplaced, "~/.config/kitty/kitty.conf"),
                (Kind::FileChanged, "~/tree"),
            ]
        );
        assert!(report.differences[0].note.contains("Home Manager"));
    }

    #[test]
    fn a_profile_is_checked_entry_by_entry() {
        let catalog = Catalog::builtin(false).unwrap();
        let platform = arch();
        let home = tempfile::tempdir().unwrap();
        let fake = Fake {
            home: home.path().to_path_buf(),
            passwd: String::new(),
            files: HashMap::new(),
        };
        let has = inventory(&[("git", "1")]);
        let none = BTreeMap::new;
        let report = compare_profile(
            "Developer basics",
            &[
                "developer-basics".to_string(),
                "dotfiles-ghostty".to_string(),
                "nope".to_string(),
            ],
            &sources(&catalog, &platform, &has, &fake, &[], &none),
        );
        assert_eq!(report.matched, 1);
        let missing: Vec<&str> = report
            .differences
            .iter()
            .filter(|difference| difference.kind == Kind::Missing)
            .map(|difference| difference.subject.as_str())
            .collect();
        assert_eq!(missing, ["Ripgrep", "fzf", "Neovim"]);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.kind == Kind::FileMissing
                    && difference.entry.as_deref() == Some("dotfiles-ghostty"))
        );
        assert_eq!(report.unknown, ["nope"]);
    }

    #[test]
    fn devices_pipes_and_huge_files_are_never_read() {
        // /dev/zero would never end; a pipe would wait for a writer.
        let (kind, sum, _) = look(Path::new("/dev/zero"));
        assert_eq!((kind, sum), (FileKind::File, None));
        let directory = tempfile::tempdir().unwrap();
        let pipe = directory.path().join("pipe");
        nix::unistd::mkfifo(&pipe, nix::sys::stat::Mode::S_IRWXU).unwrap();
        assert_eq!(look(&pipe).1, None);
        let start = std::time::Instant::now();
        let (kind, sum, _) = look(directory.path());
        assert_eq!(kind, FileKind::Directory);
        assert!(sum.is_some());
        assert!(start.elapsed() < std::time::Duration::from_secs(5));
        let big = directory.path().join("big");
        std::fs::write(&big, vec![0u8; 64]).unwrap();
        assert!(file_sum(&big, 63).is_none());
        assert!(file_sum(&big, 64).is_some());
    }

    #[test]
    fn shared_baselines_are_checked() {
        let base = Baseline {
            name: "x".into(),
            ..Baseline::default()
        };
        assert!(check(&base).is_ok());
        let evil = Baseline {
            name: "Evil\u{1b}[2J".into(),
            ..base.clone()
        };
        assert!(matches!(check(&evil), Err(BaselineError::Control)));
        for path in ["relative", "~/../etc/shadow", "/etc/../root", "~user/x"] {
            let bad = Baseline {
                files: vec![FileRecord {
                    path: path.into(),
                    ..FileRecord::default()
                }],
                ..base.clone()
            };
            assert!(check(&bad).is_err(), "{path}");
        }
        let bad_id = Baseline {
            entries: vec![Recorded {
                id: "Not An Id".into(),
                ..Recorded::default()
            }],
            ..base.clone()
        };
        assert!(check(&bad_id).is_err());
        let directory = tempfile::tempdir().unwrap();
        let later = directory.path().join("later.toml");
        std::fs::write(&later, "format = 9\nname = \"x\"\nunknown = 1\n").unwrap();
        assert!(matches!(read(&later), Err(BaselineError::Later(_, 9))));
        let huge = Baseline {
            files: (0..1001)
                .map(|index| FileRecord {
                    path: format!("~/f{index}"),
                    ..FileRecord::default()
                })
                .collect(),
            ..base.clone()
        };
        assert!(matches!(check(&huge), Err(BaselineError::TooLarge)));
        // Whatever is wrong, no error repeats an escape sequence it was given.
        for bad in [
            Baseline {
                name: "\u{1b}]52;c;x\u{7}".into(),
                ..huge.clone()
            },
            Baseline {
                entries: vec![Recorded {
                    id: "x\u{1b}[2J".into(),
                    ..Recorded::default()
                }],
                ..base.clone()
            },
        ] {
            let message = check(&bad).unwrap_err().to_string();
            assert!(!message.chars().any(char::is_control), "{message:?}");
        }
        let fine = directory.path().join("fine.toml");
        // Fields a later version adds are ignored.
        std::fs::write(&fine, "name = \"x\"\nunknown = 1\n").unwrap();
        assert_eq!(read(&fine).unwrap().name, "x");
    }

    #[test]
    fn an_entry_whose_settings_are_in_place_counts_without_a_recorded_run() {
        let catalog = Catalog::builtin(false).unwrap();
        let platform = arch();
        let home = tempfile::tempdir().unwrap();
        let grub =
            "GRUB_TIMEOUT=5\nGRUB_THEME=\"/boot/grub/themes/commander-dracula-1080p/theme.txt\"\n";
        let fake = Fake {
            home: home.path().to_path_buf(),
            passwd: String::new(),
            files: HashMap::from([(PathBuf::from("/etc/default/grub"), grub.to_string())]),
        };
        let empty = inventory(&[]);
        let none = BTreeMap::new;
        let ids = ["grub-dracula".to_string(), "printer-driverless".to_string()];
        let report_with = |render: &dyn Fn(&[String]) -> Result<String, String>| {
            let sources = Sources {
                render,
                ..sources(&catalog, &platform, &empty, &fake, &[], &none)
            };
            compare_profile("Mine", &ids, &sources)
        };
        // The installer's renderer leaves the file as it is: the theme is in
        // place, run from an older Helix Toolbox or by hand.
        let rendered_as_is = |arguments: &[String]| {
            assert_eq!(arguments[0], "grub");
            Ok(grub.to_string())
        };
        let report = report_with(&rendered_as_is);
        assert!(
            !report
                .differences
                .iter()
                .any(|difference| difference.entry.as_deref() == Some("grub-dracula")),
            "{report:?}"
        );
        assert_eq!(report.matched, 1);
        // Nothing tells for an entry with nothing to render or write.
        let other = report
            .differences
            .iter()
            .find(|difference| difference.entry.as_deref() == Some("printer-driverless"))
            .unwrap();
        assert_eq!(other.kind, Kind::NotApplied);
        assert!(
            other
                .note
                .contains("runs before Helix Toolbox 1.1 were not recorded"),
            "{}",
            other.note
        );
        // A renderer that would change the file: not in place.
        let rendered_otherwise =
            |_: &[String]| Ok("GRUB_TIMEOUT=5\nGRUB_THEME=\"elsewhere\"\n".to_string());
        let report = report_with(&rendered_otherwise);
        let dracula = report
            .differences
            .iter()
            .find(|difference| difference.entry.as_deref() == Some("grub-dracula"))
            .unwrap();
        assert!(
            dracula.note.contains("what it sets is not in place"),
            "{}",
            dracula.note
        );
    }
}
