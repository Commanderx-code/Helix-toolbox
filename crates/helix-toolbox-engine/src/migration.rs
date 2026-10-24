//! The migration hub: what this machine has, written to a file to take to
//! another machine or distribution, and that file read on the other one.
//! Catalog entries translate, since each names its package for every
//! family; a Flatpak is the same everywhere; the other packages the user
//! installed are named alike only within a family, so on another one they
//! are listed for the user, with whether the repositories there have a
//! package of the same name. Exporting only reads; bringing a migration
//! over is a plan, reviewed like any other.

use crate::{
    EngineError, Machine, PackageState, Plan, Probe, Step, doctor::System, inventory::Inventory,
};
use helix_toolbox_core::{
    Action, Entry, PackageManager, PackageNames, printable, valid_flatpak_id, valid_id,
    valid_package_name,
};
use helix_toolbox_platform::LinuxInfo;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

/// The format of the files written now.
pub const VERSION: u32 = 1;

/// The id of the entry that installs Flatpak and adds Flathub.
const FLATPAK_ENTRY: &str = "flatpak-flathub";

/// What a machine had, as written to a migration file.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Migration {
    /// The file's format.
    pub version: u32,
    /// The distribution it was made on, as os-release names it.
    pub from: String,
    /// Its package family; none for one Helix Toolbox has no packages for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<PackageManager>,
    pub architecture: String,
    /// When it was made, in seconds since 1970.
    pub made: u64,
    /// Catalog entries installed there.
    #[serde(default)]
    pub entries: Vec<String>,
    /// Flatpak applications installed there that no entry covers.
    #[serde(default)]
    pub flatpaks: Vec<String>,
    /// The family's packages the user installed there that no entry covers.
    #[serde(default)]
    pub packages: Vec<String>,
}

/// Why a migration file cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, String),
    #[error("{0} is not a Helix Toolbox migration: {1}")]
    Format(PathBuf, String),
    #[error(
        "{0} was written by a newer Helix Toolbox (format {1}); update Helix Toolbox to read it"
    )]
    Newer(PathBuf, u32),
    #[error("cannot write {0}: {1}")]
    Write(PathBuf, String),
}

/// What kind of thing a migration names.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Entry,
    Flatpak,
    Package,
}

/// Where one thing a migration names stands on this machine.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// The plan installs it.
    Install,
    /// It is here already.
    Here,
    /// Helix Toolbox has no way to install it here.
    Unavailable,
    /// Left to the user: a package of another family, or one the
    /// repositories here may not have.
    Yours,
}

impl Standing {
    pub fn label(self) -> &'static str {
        match self {
            Standing::Install => "Install",
            Standing::Here => "Here",
            Standing::Unavailable => "Not here",
            Standing::Yours => "Yours",
        }
    }
}

/// One thing a migration names, and where it stands here.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Item {
    pub kind: Kind,
    /// The entry id, Flatpak id or package name.
    pub id: String,
    pub name: String,
    pub standing: Standing,
    /// Where it comes from here, or why it does not, as a phrase.
    pub detail: String,
}

/// A migration read on this machine.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Translation {
    pub migration: Migration,
    /// Whether this machine is of the family it was made on, so its
    /// packages are named the same.
    pub same_family: bool,
    pub items: Vec<Item>,
}

impl Translation {
    pub fn count(&self, standing: Standing) -> usize {
        self.items
            .iter()
            .filter(|item| item.standing == standing)
            .count()
    }
}

/// What a migration of this machine would hold, from what is installed:
/// the entries one of whose packages is, the Flatpaks no entry covers and
/// the packages the user chose (`chosen`) that no entry covers.
pub fn export(
    entries: &[&Entry],
    platform: &LinuxInfo,
    inventory: &Inventory,
    chosen: Option<&[String]>,
    made: u64,
) -> Migration {
    let mut found: Vec<String> = Vec::new();
    let mut covered: HashSet<String> = HashSet::new();
    for entry in entries {
        if found.contains(&entry.id) || matches!(entry.action, Action::Profile { .. }) {
            continue;
        }
        let Some(identity) = entry.identity() else {
            continue;
        };
        // Only its own packages count, not its command found elsewhere: an
        // install of another kind is not this entry's to bring over.
        let status = inventory.status(entry, platform, &|_| None);
        if !matches!(
            status,
            Some(
                crate::inventory::Status::Installed { .. }
                    | crate::inventory::Status::Update { .. }
            )
        ) {
            continue;
        }
        // Installed only as its Flatpak, it goes as that Flatpak, so the
        // other machine gets the same: with the entry, its native package
        // would come first.
        let native = crate::own_probes(identity.packages, identity.aur, None, platform.manager)
            .iter()
            .any(|probe| inventory.holds(probe));
        if !native {
            continue;
        }
        found.push(entry.id.clone());
        if let Some(manager) = platform.manager {
            covered.extend(identity.packages.for_manager(manager).map(String::from));
        }
        covered.extend(identity.aur.map(String::from));
    }
    let mut flatpaks: Vec<String> = inventory
        .flatpak_ids()
        .into_iter()
        .filter(|id| !covered.contains(id) && valid_flatpak_id(id))
        .collect();
    flatpaks.sort();
    let mut packages: Vec<String> = chosen
        .unwrap_or_default()
        .iter()
        .filter(|name| !covered.contains(*name) && valid_package_name(name))
        .cloned()
        .collect();
    packages.sort();
    packages.dedup();
    Migration {
        version: VERSION,
        from: printable(&platform.pretty_name),
        family: platform.manager,
        architecture: std::env::consts::ARCH.to_string(),
        made,
        entries: found,
        flatpaks,
        packages,
    }
}

/// A name for the file, from the day it was made.
pub fn file_name(migration: &Migration) -> String {
    format!(
        "helix-toolbox-migration-{}.toml",
        crate::date(migration.made)
    )
}

/// The migration as the file holds it.
pub fn to_text(migration: &Migration) -> String {
    format!(
        "# A Helix Toolbox migration: helix-toolbox migrate show <file>, then migrate apply <file>\n{}",
        toml::to_string(migration).unwrap_or_default()
    )
}

/// Write a migration file, beside it first, then renamed.
pub fn write(path: &Path, migration: &Migration) -> Result<(), MigrationError> {
    let failed = |error: String| MigrationError::Write(path.to_path_buf(), error);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| failed(error.to_string()))?;
    }
    let staging = path.with_extension("toml.new");
    std::fs::write(&staging, to_text(migration)).map_err(|error| failed(error.to_string()))?;
    std::fs::rename(&staging, path).map_err(|error| failed(error.to_string()))
}

/// Read a migration file. Everything it names must be a well-formed entry
/// id, Flatpak id or package name, since these go into commands.
pub fn read(path: &Path) -> Result<Migration, MigrationError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| MigrationError::Read(path.to_path_buf(), error.to_string()))?;
    parse(&text).map_err(|error| match error {
        Parsed::Newer(version) => MigrationError::Newer(path.to_path_buf(), version),
        Parsed::Format(why) => MigrationError::Format(path.to_path_buf(), why),
    })
}

enum Parsed {
    Newer(u32),
    Format(String),
}

fn parse(text: &str) -> Result<Migration, Parsed> {
    let migration: Migration =
        toml::from_str(text).map_err(|error| Parsed::Format(error.message().to_string()))?;
    if migration.version > VERSION {
        return Err(Parsed::Newer(migration.version));
    }
    let bad = |what: &str, name: &str| {
        Parsed::Format(format!("{what} {:?} is not valid", printable(name)))
    };
    if let Some(id) = migration.entries.iter().find(|id| !valid_id(id)) {
        return Err(bad("the entry", id));
    }
    if let Some(id) = migration.flatpaks.iter().find(|id| !valid_flatpak_id(id)) {
        return Err(bad("the Flatpak", id));
    }
    if let Some(name) = migration
        .packages
        .iter()
        .find(|name| !valid_package_name(name))
    {
        return Err(bad("the package", name));
    }
    Ok(Migration {
        from: printable(&migration.from),
        architecture: printable(&migration.architecture),
        ..migration
    })
}

/// How the repositories here call a family's package manager.
fn manager_name(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Debian => "apt",
        PackageManager::Void => "xbps",
        manager => manager.executable(),
    }
}

/// Where everything a migration names stands on this machine. `entries`
/// is this catalog; `available` the package names this family's
/// repositories offer, from the metadata on the machine, when it was read.
pub fn translate(
    migration: Migration,
    entries: &[&Entry],
    platform: &LinuxInfo,
    machine: Machine,
    available: Option<&HashSet<String>>,
) -> Translation {
    let same_family = migration.family.is_some() && migration.family == platform.manager;
    let find = |id: &str| entries.iter().copied().find(|entry| entry.id == id);
    let mut items: Vec<Item> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut add_entry = |items: &mut Vec<Item>, id: &str, detail_for: Option<&str>| {
        if !seen.insert(id.to_string()) {
            return;
        }
        let Some(entry) = find(id) else {
            items.push(Item {
                kind: Kind::Entry,
                id: id.to_string(),
                name: id.to_string(),
                standing: Standing::Unavailable,
                detail: "not in the catalog here".to_string(),
            });
            return;
        };
        let (standing, detail) = entry_standing(entry, platform, machine, &find);
        items.push(Item {
            kind: Kind::Entry,
            id: entry.id.clone(),
            name: entry.name.clone(),
            standing,
            detail: match detail_for {
                Some(name) => format!("{detail}; for {name} there"),
                None => detail,
            },
        });
    };
    for id in &migration.entries {
        add_entry(&mut items, id, None);
    }
    // A package another catalog had no entry for may have one in this
    // catalog, named for the family the migration was made on.
    let by_package: HashMap<&str, &Entry> = match migration.family {
        Some(family) => entries
            .iter()
            .filter_map(|entry| {
                let identity = entry.identity()?;
                let mut names: Vec<&str> =
                    identity.packages.for_manager(family).into_iter().collect();
                if family == PackageManager::Arch {
                    names.extend(identity.aur);
                }
                Some(names.into_iter().map(move |name| (name, *entry)))
            })
            .flatten()
            .collect(),
        None => HashMap::new(),
    };
    let by_flatpak: HashMap<&str, &Entry> = entries
        .iter()
        .filter_map(|entry| Some((entry.identity()?.flatpak?, *entry)))
        .collect();
    let mut flatpak_items: Vec<Item> = Vec::new();
    for id in &migration.flatpaks {
        // The application may be here as its entry's native package.
        let native = by_flatpak.get(id.as_str()).and_then(|entry| {
            match crate::package_state(entry, platform, machine)? {
                PackageState::Installed(found) | PackageState::OnPath(found) => {
                    Some(format!("{} is here: {found}", entry.name))
                }
                PackageState::Absent { .. } => None,
            }
        });
        if let Some(detail) = native {
            flatpak_items.push(Item {
                kind: Kind::Flatpak,
                id: id.clone(),
                name: id.clone(),
                standing: Standing::Here,
                detail,
            });
            continue;
        }
        let here = (machine.holds)(&Probe::Flatpak(id.clone()));
        flatpak_items.push(Item {
            kind: Kind::Flatpak,
            id: id.clone(),
            name: id.clone(),
            standing: if here {
                Standing::Here
            } else {
                Standing::Install
            },
            detail: if here {
                "installed".to_string()
            } else {
                "Flathub".to_string()
            },
        });
    }
    // Flatpaks need Flatpak: when it is missing, its entry goes first, and
    // without one they cannot be installed here.
    if flatpak_items
        .iter()
        .any(|item| item.standing == Standing::Install)
        && !(machine.on_path)("flatpak")
    {
        if find(FLATPAK_ENTRY).is_some() {
            add_entry(&mut items, FLATPAK_ENTRY, Some("the Flatpaks"));
        } else {
            for item in &mut flatpak_items {
                if item.standing == Standing::Install {
                    item.standing = Standing::Unavailable;
                    item.detail =
                        "Flatpak is not installed here, and no entry here installs it".to_string();
                }
            }
        }
    }
    items.extend(flatpak_items);
    for name in &migration.packages {
        if let Some(entry) = by_package.get(name.as_str()) {
            add_entry(&mut items, &entry.id.clone(), Some(name));
            continue;
        }
        items.push(package_item(
            name,
            &migration,
            platform,
            machine,
            available,
            same_family,
        ));
    }
    Translation {
        migration,
        same_family,
        items,
    }
}

fn entry_standing<'a>(
    entry: &'a Entry,
    platform: &LinuxInfo,
    machine: Machine,
    find: &dyn Fn(&str) -> Option<&'a Entry>,
) -> (Standing, String) {
    if !entry.needs.runs_on(std::env::consts::ARCH) {
        return (
            Standing::Unavailable,
            format!("not made for {}", std::env::consts::ARCH),
        );
    }
    if let Some(PackageState::Installed(found) | PackageState::OnPath(found)) =
        crate::package_state(entry, platform, machine)
    {
        return (Standing::Here, found);
    }
    match crate::plan_with([entry], platform, find, machine) {
        Err(EngineError::UnsupportedEntry(_)) => (
            Standing::Unavailable,
            format!("no package for {}", platform.pretty_name),
        ),
        Err(error) => (Standing::Unavailable, error.to_string()),
        Ok(planned) => match planned.steps.first() {
            Some(Step::Package { sources, .. }) => (
                Standing::Install,
                sources
                    .first()
                    .map(|source| source.label.clone())
                    .unwrap_or_default(),
            ),
            Some(Step::Script { .. }) => (Standing::Install, "its script".to_string()),
            Some(_) => (Standing::Install, "its command".to_string()),
            None => (
                Standing::Unavailable,
                planned
                    .blocked
                    .first()
                    .or(planned.notes.first())
                    .cloned()
                    .unwrap_or_else(|| "nothing to run here".to_string()),
            ),
        },
    }
}

fn package_item(
    name: &str,
    migration: &Migration,
    platform: &LinuxInfo,
    machine: Machine,
    available: Option<&HashSet<String>>,
    same_family: bool,
) -> Item {
    let item = |standing, detail: String| Item {
        kind: Kind::Package,
        id: name.to_string(),
        name: name.to_string(),
        standing,
        detail,
    };
    let Some(manager) = platform.manager else {
        return item(
            Standing::Unavailable,
            format!(
                "Helix Toolbox installs no packages on {}",
                platform.pretty_name
            ),
        );
    };
    let probe = (manager != PackageManager::Solus).then(|| Probe::Package {
        manager,
        name: name.to_string(),
    });
    if probe.as_ref().is_some_and(|probe| (machine.holds)(probe)) {
        return item(
            Standing::Here,
            if same_family {
                "installed".to_string()
            } else {
                "installed under the same name".to_string()
            },
        );
    }
    let offered = available.map(|names| names.contains(name));
    let repositories = format!("{}'s repositories here", manager_name(manager));
    match (same_family, offered) {
        (true, Some(true)) => item(Standing::Install, manager_name(manager).to_string()),
        (true, Some(false)) => item(
            Standing::Yours,
            match manager {
                PackageManager::Arch => {
                    format!("not in {repositories}: from the AUR or a file, perhaps")
                }
                _ => format!("not in {repositories}: from another repository or a file, perhaps"),
            },
        ),
        (true, None) => item(
            Standing::Yours,
            format!("not checked against {repositories}"),
        ),
        (false, Some(true)) => item(
            Standing::Yours,
            format!("{repositories} have a package of this name; check it is the same"),
        ),
        (false, _) => item(
            Standing::Yours,
            format!(
                "named as on {}; look for what {} calls it",
                migration.from, platform.pretty_name
            ),
        ),
    }
}

/// The plan that installs what a translation marks Install: the entries,
/// then the Flatpaks from Flathub, then the packages through this family's
/// package manager. A failed install does not stop the others.
pub fn plan(
    translation: &Translation,
    entries: &[&Entry],
    platform: &LinuxInfo,
    machine: Machine,
) -> Result<Plan, EngineError> {
    let installs = translation
        .items
        .iter()
        .filter(|item| item.standing == Standing::Install);
    let mut made: Vec<Entry> = Vec::new();
    let mut ids: Vec<String> = Vec::new();
    for item in installs {
        match item.kind {
            Kind::Entry => ids.push(item.id.clone()),
            Kind::Flatpak => {
                made.push(package_entry(
                    format!("flatpak:{}", item.id),
                    &item.id,
                    PackageNames::default(),
                    Some(item.id.clone()),
                ));
                ids.push(format!("flatpak:{}", item.id));
            }
            Kind::Package => {
                let Some(manager) = platform.manager else {
                    continue;
                };
                let mut names = PackageNames::default();
                *names.for_manager_mut(manager) = Some(item.id.clone());
                made.push(package_entry(
                    format!("package:{}", item.id),
                    &item.id,
                    names,
                    None,
                ));
                ids.push(format!("package:{}", item.id));
            }
        }
    }
    let lookup = |id: &str| -> Option<&Entry> {
        entries
            .iter()
            .copied()
            .find(|entry| entry.id == id)
            .or_else(|| made.iter().find(|entry| entry.id == id))
    };
    let queued: Vec<&Entry> = ids.iter().filter_map(|id| lookup(id)).collect();
    let mut planned = crate::plan_with(queued, platform, &lookup, machine)?;
    planned.keep_going = true;
    Ok(planned)
}

/// An entry for a Flatpak or package the catalog has none for.
fn package_entry(id: String, name: &str, packages: PackageNames, flatpak: Option<String>) -> Entry {
    Entry {
        id,
        name: name.to_string(),
        description: String::new(),
        flags: "I".to_string(),
        multi_select: true,
        writes: Vec::new(),
        action: Action::Package {
            binary: None,
            packages,
            aur: None,
            flatpak,
            conflicts: Vec::new(),
            removable: false,
            release: None,
        },
        detect: None,
        needs: Default::default(),
        effects: Default::default(),
    }
}

/// The packages the user installed themselves, as the family's package
/// manager records them; None where it cannot be told.
pub fn chosen(system: &dyn System, manager: Option<PackageManager>) -> Option<Vec<String>> {
    let output = |command: &[&str]| output(system, command);
    match manager? {
        PackageManager::Arch => output(&["pacman", "-Qqe"]).map(|text| names(&text)),
        PackageManager::Debian => output(&["apt-mark", "showmanual"]).map(|text| names(&text)),
        PackageManager::Fedora => output(&[
            "dnf",
            "-C",
            "-q",
            "repoquery",
            "--userinstalled",
            "--qf",
            "%{name}\n",
        ])
        .map(|text| names(&text)),
        PackageManager::Suse => {
            // zypper records the packages it installed as dependencies; the
            // others were asked for.
            let installed = output(&["rpm", "-qa", "--qf", "%{NAME}\n"])?;
            let automatic = system.read(Path::new("/var/lib/zypp/AutoInstalled"))?;
            Some(asked_for(&installed, &automatic))
        }
        PackageManager::Alpine => system
            .read(Path::new("/etc/apk/world"))
            .map(|text| apk_world(&text)),
        PackageManager::Void => output(&["xbps-query", "-m"]).map(|text| xbps_names(&text)),
        PackageManager::Solus => None,
    }
}

/// The package names this family's repositories offer, from the metadata
/// already on the machine (no network); None where it is not read.
pub fn available(system: &dyn System, manager: Option<PackageManager>) -> Option<HashSet<String>> {
    let output = |command: &[&str]| output(system, command);
    let text = match manager? {
        PackageManager::Arch => output(&["pacman", "-Slq"])?,
        PackageManager::Debian => output(&["apt-cache", "pkgnames"])?,
        PackageManager::Fedora => output(&["dnf", "-C", "-q", "repoquery", "--qf", "%{name}\n"])?,
        PackageManager::Suse => {
            return output(&[
                "zypper",
                "--no-refresh",
                "-q",
                "-x",
                "search",
                "-t",
                "package",
            ])
            .map(|text| zypper_names(&text))
            .filter(|names| !names.is_empty());
        }
        _ => return None,
    };
    let found: HashSet<String> = names(&text).into_iter().collect();
    // An empty list means the metadata is missing, not an empty repository.
    (!found.is_empty()).then_some(found)
}

/// A migration file read on the machine `system` answers for, its
/// packages read once.
pub fn read_here<S>(
    system: &S,
    path: &Path,
    entries: &[&Entry],
    platform: &LinuxInfo,
) -> Result<Translation, MigrationError>
where
    S: System + Clone + Send + Sync + 'static,
{
    let migration = read(path)?;
    let inventory = Inventory::gather(system, platform);
    let holds = |probe: &Probe| inventory.holds(probe);
    let on_path = |tool: &str| system.on_path(tool);
    let machine = Machine {
        holds: &holds,
        on_path: &on_path,
        ..crate::Machine::blind()
    };
    Ok(translate(
        migration,
        entries,
        platform,
        machine,
        available(system, platform.manager).as_ref(),
    ))
}

/// `plan` on the machine `system` answers for, its packages read once.
pub fn plan_here<S>(
    system: &S,
    translation: &Translation,
    entries: &[&Entry],
    platform: &LinuxInfo,
) -> Result<Plan, EngineError>
where
    S: System + Clone + Send + Sync + 'static,
{
    let inventory = Inventory::gather(system, platform);
    let holds = |probe: &Probe| inventory.holds(probe);
    let on_path = |tool: &str| system.on_path(tool);
    let machine = Machine {
        holds: &holds,
        on_path: &on_path,
        ..crate::Machine::blind()
    };
    plan(translation, entries, platform, machine)
}

/// What the machine `system` answers for has, its packages read once.
pub fn gather<S>(system: &S, entries: &[&Entry], platform: &LinuxInfo, made: u64) -> Migration
where
    S: System + Clone + Send + Sync + 'static,
{
    let inventory = Inventory::gather(system, platform);
    let chosen = chosen(system, platform.manager);
    export(entries, platform, &inventory, chosen.as_deref(), made)
}

/// A read-only query's standard output; None when it failed or could not run.
fn output(system: &dyn System, command: &[&str]) -> Option<String> {
    system
        .run(command)
        .filter(|(success, _)| *success)
        .map(|(_, text)| text)
}

/// One name a line.
fn names(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|name| valid_package_name(name))
        .map(String::from)
        .collect()
}

/// rpm's names less those zypper installed as dependencies.
fn asked_for(installed: &str, automatic: &str) -> Vec<String> {
    let automatic: HashSet<&str> = automatic.lines().map(str::trim).collect();
    let mut found: Vec<String> = names(installed)
        .into_iter()
        .filter(|name| !automatic.contains(name.as_str()) && !name.starts_with("gpg-pubkey"))
        .collect();
    found.sort();
    found.dedup();
    found
}

/// /etc/apk/world: names, with a version or repository after some.
fn apk_world(text: &str) -> Vec<String> {
    text.split_whitespace()
        .filter_map(|word| word.split(['<', '>', '=', '~', '@']).next())
        .filter(|name| valid_package_name(name))
        .map(String::from)
        .collect()
}

/// `xbps-query -m`: `name-version_revision` a line.
fn xbps_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().rsplit_once('-'))
        .map(|(name, _)| name)
        .filter(|name| valid_package_name(name))
        .map(String::from)
        .collect()
}

/// The `name="…"` of each solvable in zypper's XML search results.
fn zypper_names(text: &str) -> HashSet<String> {
    text.split("<solvable ")
        .skip(1)
        .filter_map(|solvable| {
            let start = solvable
                .find(" name=\"")
                .map(|at| at + 7)
                .or_else(|| solvable.strip_prefix("name=\"").map(|_| 6))?;
            let rest = &solvable[start..];
            Some(rest[..rest.find('"')?].to_string())
        })
        .filter(|name| valid_package_name(name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn arch() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=arch\nPRETTY_NAME=\"Arch Linux\"\n").unwrap()
    }

    fn fedora() -> LinuxInfo {
        LinuxInfo::from_os_release("ID=fedora\nPRETTY_NAME=\"Fedora Linux 43\"\n").unwrap()
    }

    fn package(id: &str, arch: Option<&str>, fedora: Option<&str>, flatpak: Option<&str>) -> Entry {
        let names = PackageNames {
            arch: arch.map(String::from),
            fedora: fedora.map(String::from),
            ..Default::default()
        };
        let mut entry = package_entry(
            id.to_string(),
            &id.to_uppercase(),
            names,
            flatpak.map(String::from),
        );
        entry.flags = "I".to_string();
        entry
    }

    fn catalog() -> Vec<Entry> {
        vec![
            package(
                "firefox",
                Some("firefox"),
                Some("firefox"),
                Some("org.mozilla.firefox"),
            ),
            package("yay-only", Some("yay"), None, None),
            package("gimp", Some("gimp"), Some("gimp"), Some("org.gimp.GIMP")),
            package("ripgrep", Some("ripgrep"), Some("ripgrep"), None),
        ]
    }

    #[test]
    fn an_export_names_entries_then_what_no_entry_covers() {
        let entries = catalog();
        let refs: Vec<&Entry> = entries.iter().collect();
        let native: HashMap<String, String> =
            [("firefox", "130-1"), ("yay", "12-1"), ("htop", "3-1")]
                .into_iter()
                .map(|(name, version)| (name.to_string(), version.to_string()))
                .collect();
        let flatpaks: HashMap<String, String> =
            [("org.gimp.GIMP", "2.10"), ("com.spotify.Client", "1")]
                .into_iter()
                .map(|(id, version)| (id.to_string(), version.to_string()))
                .collect();
        let inventory = Inventory::from_parts(
            Some(PackageManager::Arch),
            Some(native),
            HashMap::new(),
            flatpaks,
            HashSet::new(),
        );
        let chosen = ["firefox", "htop", "yay", "base", "bad name"].map(String::from);
        let exported = export(&refs, &arch(), &inventory, Some(&chosen), 1_767_225_600);
        // GIMP is here only as its Flatpak, so it goes as that Flatpak.
        assert_eq!(exported.entries, ["firefox", "yay-only"]);
        assert_eq!(exported.flatpaks, ["com.spotify.Client", "org.gimp.GIMP"]);
        assert_eq!(exported.packages, ["base", "htop"]);
        assert_eq!(exported.family, Some(PackageManager::Arch));
        assert_eq!(
            file_name(&exported),
            "helix-toolbox-migration-2026-01-01.toml"
        );
        // It reads back as written.
        let text = to_text(&exported);
        assert!(text.starts_with("# A Helix Toolbox migration"));
        assert_eq!(parse(&text).ok(), Some(exported));
    }

    #[test]
    fn a_migration_file_must_name_only_well_formed_things() {
        let good = "version = 1\nfrom = \"Arch\"\narchitecture = \"x86_64\"\nmade = 0\npackages = [\"htop\"]\n";
        assert!(parse(good).is_ok());
        for bad in [
            "version = 1\nfrom = \"\"\narchitecture = \"\"\nmade = 0\npackages = [\"-rf\"]\n",
            "version = 1\nfrom = \"\"\narchitecture = \"\"\nmade = 0\nflatpaks = [\"x;rm\"]\n",
            "version = 1\nfrom = \"\"\narchitecture = \"\"\nmade = 0\nentries = [\"A B\"]\n",
            "version = 1\nfrom = \"\"\narchitecture = \"\"\nmade = 0\nextra = 1\n",
        ] {
            assert!(matches!(parse(bad), Err(Parsed::Format(_))), "{bad}");
        }
        assert!(matches!(
            parse("version = 9\nfrom = \"\"\narchitecture = \"\"\nmade = 0\n"),
            Err(Parsed::Newer(9))
        ));
    }

    #[test]
    fn on_another_family_entries_translate_and_packages_are_left_to_the_user() {
        let entries = catalog();
        let refs: Vec<&Entry> = entries.iter().collect();
        let migration = Migration {
            version: VERSION,
            from: "Arch Linux".into(),
            family: Some(PackageManager::Arch),
            architecture: std::env::consts::ARCH.into(),
            made: 0,
            entries: vec!["firefox".into(), "yay-only".into(), "gone".into()],
            flatpaks: vec![
                "com.spotify.Client".into(),
                "org.gimp.GIMP".into(),
                "org.mozilla.firefox".into(),
            ],
            packages: vec!["ripgrep".into(), "htop".into(), "python-requests".into()],
        };
        let holds =
            |probe: &Probe| matches!(probe, Probe::Package { name, .. } if name == "firefox");
        let on_path = |_: &str| true;
        let machine = Machine {
            holds: &holds,
            on_path: &on_path,
            ..crate::Machine::blind()
        };
        let available: HashSet<String> = ["htop".to_string()].into();
        let translated = translate(migration, &refs, &fedora(), machine, Some(&available));
        assert!(!translated.same_family);
        let standing: Vec<(&str, Standing)> = translated
            .items
            .iter()
            .map(|item| (item.id.as_str(), item.standing))
            .collect();
        assert_eq!(
            standing,
            [
                ("firefox", Standing::Here),
                ("yay-only", Standing::Unavailable),
                ("gone", Standing::Unavailable),
                ("com.spotify.Client", Standing::Install),
                ("org.gimp.GIMP", Standing::Install),
                // Here as its entry's native package.
                ("org.mozilla.firefox", Standing::Here),
                // A package this catalog has an entry for, by its Arch name.
                ("ripgrep", Standing::Install),
                ("htop", Standing::Yours),
                ("python-requests", Standing::Yours),
            ]
        );
        assert_eq!(translated.items[1].detail, "no package for Fedora Linux 43");
        assert!(
            translated.items[7]
                .detail
                .contains("have a package of this name")
        );
        assert!(
            translated.items[8]
                .detail
                .contains("named as on Arch Linux")
        );
        assert_eq!(
            translated.items[5].detail,
            "FIREFOX is here: dnf lists firefox"
        );
        let planned = plan(&translated, &refs, &fedora(), machine).unwrap();
        let ids: Vec<&str> = planned.steps.iter().map(|step| step.id()).collect();
        assert_eq!(
            ids,
            [
                "flatpak:com.spotify.Client",
                "flatpak:org.gimp.GIMP",
                "ripgrep"
            ]
        );
        assert!(planned.keep_going);
    }

    #[test]
    fn within_a_family_packages_the_repositories_have_are_installed() {
        let entries = catalog();
        let refs: Vec<&Entry> = entries.iter().collect();
        let migration = Migration {
            version: VERSION,
            from: "Garuda Linux".into(),
            family: Some(PackageManager::Arch),
            architecture: std::env::consts::ARCH.into(),
            made: 0,
            flatpaks: vec!["com.spotify.Client".into()],
            packages: vec!["htop".into(), "an-aur-thing".into(), "nano".into()],
            ..Migration::default()
        };
        let holds = |probe: &Probe| matches!(probe, Probe::Package { name, .. } if name == "nano");
        // No Flatpak here yet, so the entry that sets it up goes first.
        let on_path = |tool: &str| tool != "flatpak";
        let machine = Machine {
            holds: &holds,
            on_path: &on_path,
            ..crate::Machine::blind()
        };
        let mut with_flatpak = entries.clone();
        let mut setup = package(FLATPAK_ENTRY, Some("flatpak"), Some("flatpak"), None);
        setup.name = "Flatpak / Flathub".into();
        with_flatpak.push(setup);
        let refs_with: Vec<&Entry> = with_flatpak.iter().collect();
        let available: HashSet<String> = ["htop".to_string(), "nano".to_string()].into();
        let translated = translate(
            migration.clone(),
            &refs_with,
            &arch(),
            machine,
            Some(&available),
        );
        assert!(translated.same_family);
        let standing: Vec<(&str, Standing)> = translated
            .items
            .iter()
            .map(|item| (item.id.as_str(), item.standing))
            .collect();
        assert_eq!(
            standing,
            [
                (FLATPAK_ENTRY, Standing::Install),
                ("com.spotify.Client", Standing::Install),
                ("htop", Standing::Install),
                ("an-aur-thing", Standing::Yours),
                ("nano", Standing::Here),
            ]
        );
        assert!(translated.items[3].detail.contains("the AUR"));
        let planned = plan(&translated, &refs_with, &arch(), machine).unwrap();
        let ids: Vec<&str> = planned.steps.iter().map(|step| step.id()).collect();
        assert_eq!(
            ids,
            [FLATPAK_ENTRY, "flatpak:com.spotify.Client", "package:htop"]
        );
        // Without the repositories' list nothing is assumed.
        let unread = translate(migration, &refs, &arch(), machine, None);
        assert!(
            unread
                .items
                .iter()
                .filter(|item| item.kind == Kind::Package && item.id != "nano")
                .all(|item| item.standing == Standing::Yours)
        );
        // Nor, with no Flatpak and no entry for it, are the Flatpaks.
        assert_eq!(unread.items[0].id, "com.spotify.Client");
        assert_eq!(unread.items[0].standing, Standing::Unavailable);
    }

    #[test]
    fn the_package_lists_are_read_from_each_family() {
        assert_eq!(names("htop\n\nbad name\nnano\n"), ["htop", "nano"]);
        assert_eq!(
            asked_for("bash\nhtop\nglibc\ngpg-pubkey\n", "glibc\nbash\n"),
            ["htop"]
        );
        assert_eq!(
            apk_world("alpine-base\nhtop>=3\nnano@edge\n"),
            ["alpine-base", "htop", "nano"]
        );
        assert_eq!(
            xbps_names("htop-3.3.0_1\nnetwork-manager-applet-1.36_1\n"),
            ["htop", "network-manager-applet"]
        );
        let xml = "<?xml version='1.0'?><stream><search-result><solvable-list><solvable status=\"not-installed\" name=\"htop\" summary=\"x\" kind=\"package\"/><solvable name=\"nano\" kind=\"package\"/></solvable-list></search-result></stream>";
        assert_eq!(
            zypper_names(xml),
            ["htop".to_string(), "nano".to_string()].into()
        );
    }

    #[test]
    fn the_machine_is_asked_through_its_system() {
        let mut fake = crate::doctor::Fake::default();
        let mut answer = |command: &str, success: bool, text: &str| {
            fake.commands.insert(command.into(), (success, text.into()));
        };
        answer("pacman -Q", true, "firefox 130-1\nhtop 3-1\n");
        answer("pacman -Qqe", true, "firefox\nhtop\n");
        answer("pacman -Slq", true, "firefox\nhtop\nnano\n");
        answer("rpm -qa --qf %{NAME}\n", true, "bash\nhtop\n");
        // A failed query says nothing, even with output.
        answer("apt-mark showmanual", false, "htop\n");
        fake.files
            .insert("/var/lib/zypp/AutoInstalled".into(), "bash\n".into());
        fake.files
            .insert("/etc/apk/world".into(), "alpine-base\nhtop>=3\n".into());
        assert_eq!(
            chosen(&fake, Some(PackageManager::Arch)),
            Some(vec!["firefox".to_string(), "htop".to_string()])
        );
        assert_eq!(
            chosen(&fake, Some(PackageManager::Suse)),
            Some(vec!["htop".to_string()])
        );
        assert_eq!(
            chosen(&fake, Some(PackageManager::Alpine)),
            Some(vec!["alpine-base".to_string(), "htop".to_string()])
        );
        assert_eq!(chosen(&fake, Some(PackageManager::Debian)), None);
        assert_eq!(chosen(&fake, Some(PackageManager::Fedora)), None);
        assert_eq!(
            available(&fake, Some(PackageManager::Arch)).map(|names| names.len()),
            Some(3)
        );
        assert_eq!(available(&fake, Some(PackageManager::Debian)), None);
        let entries = catalog();
        let refs: Vec<&Entry> = entries.iter().collect();
        let found = gather(&fake, &refs, &arch(), 0);
        assert_eq!(found.entries, ["firefox"]);
        assert_eq!(found.packages, ["htop"]);
    }
}
