//! The catalog on disk: `catalog/tabs.toml` names the tabs in order; each tab
//! is a directory with `tab.toml`, and inside it entries are `<id>.toml` files
//! and groups are directories with `group.toml`. A script entry names a script
//! under `scripts/installers/` by id, or keeps one beside its TOML file; the
//! shared helpers live in `scripts/shared/`. Both trees are embedded into the
//! binary and unpacked to a temporary directory when the application starts,
//! so scripts are real files when they run. Each unpacked copy holds a lock
//! for as long as its process lives; copies left behind by a process that was
//! killed are removed the next time HelixForge starts.

use helixforge_core::{
    ARCHITECTURES, Action, Check, Detect, Effects, Entry, FileEffect, GPU_VENDORS, Group, Item,
    NativeAction, Needs, NixAction, PackageNames, Precondition, Tab, valid_binary_name,
    valid_flatpak_id, valid_id, valid_package_name, valid_script_segment,
};
use helixforge_platform::binary_exists;
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs::{self, File},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
};
use thiserror::Error;

static EMBEDDED_CATALOG: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/catalog");
static EMBEDDED_SCRIPTS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/scripts");

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("{0}: {1}")]
    Io(PathBuf, #[source] std::io::Error),
    #[error("{0}: {1}")]
    Toml(PathBuf, #[source] toml::de::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug)]
pub struct Catalog {
    pub tabs: Vec<Tab>,
    /// The directory holding `catalog/` and `scripts/`.
    pub root: PathBuf,
    /// Keeps the unpacked trees alive while the catalog is used.
    _unpacked: Option<Unpacked>,
}

/// Prefix of the temporary directories the embedded trees are unpacked to.
const UNPACK_PREFIX: &str = "helixforge-";
/// The file inside an unpacked copy that its process keeps locked.
const LOCK_FILE: &str = ".helixforge-lock";

/// An unpacked copy of the embedded trees, removed when dropped.
#[derive(Debug)]
struct Unpacked {
    // Dropped first, so the directory is removed while the lock is still held.
    directory: tempfile::TempDir,
    _lock: File,
}

impl Unpacked {
    /// Make a locked, empty directory under `parent`. The lock file is locked
    /// before it gets its name, so a sweep never sees it unlocked.
    fn create(parent: &Path) -> std::io::Result<Self> {
        let directory = tempfile::Builder::new()
            .prefix(UNPACK_PREFIX)
            .tempdir_in(parent)?;
        let staging = directory.path().join(".helixforge-lock.new");
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&staging)?;
        lock.try_lock()
            .map_err(|error| std::io::Error::other(error.to_string()))?;
        fs::rename(&staging, directory.path().join(LOCK_FILE))?;
        Ok(Self {
            directory,
            _lock: lock,
        })
    }

    fn path(&self) -> &Path {
        self.directory.path()
    }
}

/// Remove unpacked copies under `parent` whose process has ended: they belong
/// to this user, carry a lock file, and nobody holds the lock. Copies without
/// a lock file, other users' copies and anything that is not a plain directory
/// are left alone. Returns how many were removed.
pub fn sweep_stale(parent: &Path) -> usize {
    let Ok(listing) = fs::read_dir(parent) else {
        return 0;
    };
    let uid = fs::metadata("/proc/self").map(|meta| meta.uid()).ok();
    let mut removed = 0;
    for item in listing.flatten() {
        if !item
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(UNPACK_PREFIX))
        {
            continue;
        }
        let path = item.path();
        let Ok(meta) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_dir() || Some(meta.uid()) != uid {
            continue;
        }
        // The directory is private to its owner, so the lock file is ours.
        let Ok(lock) = File::open(path.join(LOCK_FILE)) else {
            continue;
        };
        if lock.try_lock().is_ok() && fs::remove_dir_all(&path).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Write an embedded tree into `target`. include_dir writes a tree's own
/// files into `target` without creating it, and creates it only on the way
/// to a subdirectory, so whether that comes first depends on the order the
/// build machine's file system listed them: make it first.
fn unpack(tree: &Dir, target: &Path) -> std::io::Result<()> {
    fs::create_dir_all(target)?;
    tree.extract(target)
}

impl Catalog {
    /// Unpack the embedded catalog and read it. With `validate`, entries whose
    /// preconditions fail on this machine are left out.
    pub fn builtin(validate: bool) -> Result<Self, CatalogError> {
        let parent = std::env::temp_dir();
        sweep_stale(&parent);
        let directory = Unpacked::create(&parent)
            .map_err(|error| CatalogError::Io(PathBuf::from("temporary directory"), error))?;
        for (tree, name) in [
            (&EMBEDDED_CATALOG, "catalog"),
            (&EMBEDDED_SCRIPTS, "scripts"),
        ] {
            let target = directory.path().join(name);
            unpack(tree, &target).map_err(|error| CatalogError::Io(target, error))?;
        }
        let tabs = load_tabs(directory.path(), validate)?;
        Ok(Self {
            tabs,
            root: directory.path().to_path_buf(),
            _unpacked: Some(directory),
        })
    }

    /// Read a catalog from a directory holding `catalog/` and `scripts/`.
    pub fn from_directory(root: &Path, validate: bool) -> Result<Self, CatalogError> {
        Ok(Self {
            tabs: load_tabs(root, validate)?,
            root: root.to_path_buf(),
            _unpacked: None,
        })
    }

    pub fn find(&self, id: &str) -> Option<&Entry> {
        self.entries().into_iter().find(|entry| entry.id == id)
    }

    pub fn entries(&self) -> Vec<&Entry> {
        self.tabs.iter().flat_map(Tab::entries).collect()
    }

    /// The folder holding `catalog/` and `scripts/`.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The shared shell helpers that scripts source through `HELIXFORGE_LIB`.
    pub fn lib_dir(&self) -> PathBuf {
        self.root.join("scripts/shared")
    }
}

#[derive(Deserialize)]
struct TabsFile {
    tabs: Vec<String>,
}

#[derive(Deserialize)]
struct TabFile {
    name: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct GroupFile {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_true")]
    multi_select: bool,
    #[serde(default)]
    order: Option<i64>,
    #[serde(default)]
    preconditions: Vec<Precondition>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryFile {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    flags: String,
    #[serde(default = "default_true")]
    multi_select: bool,
    #[serde(default)]
    order: Option<i64>,
    #[serde(default)]
    preconditions: Vec<Precondition>,
    #[serde(default)]
    writes: Vec<String>,
    #[serde(default)]
    package: Option<PackageFile>,
    #[serde(default)]
    script: Option<ScriptFile>,
    #[serde(default)]
    command: Option<CommandFile>,
    #[serde(default)]
    nix: Option<NixFile>,
    #[serde(default)]
    native: Option<NativeFile>,
    #[serde(default)]
    profile: Option<ProfileFile>,
    /// What a script or command entry installs, for its status.
    #[serde(default)]
    detect: Option<DetectFile>,
    #[serde(default)]
    needs: Option<NeedsFile>,
    /// What a script is expected to change, for the change preview.
    #[serde(default)]
    effects: Option<EffectsFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EffectsFile {
    #[serde(default)]
    files: Vec<FileEffectFile>,
    #[serde(default)]
    packages: Vec<String>,
    #[serde(default)]
    services: Vec<String>,
    #[serde(default)]
    takes_effect: Option<String>,
    #[serde(default)]
    unknown: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileEffectFile {
    path: String,
    change: String,
    #[serde(default)]
    render: Vec<String>,
    #[serde(default)]
    when: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NeedsFile {
    #[serde(default)]
    gpu: Vec<String>,
    #[serde(default)]
    repositories: Vec<String>,
    #[serde(default)]
    space_mb: Option<u64>,
    #[serde(default)]
    after: Vec<String>,
    #[serde(default)]
    architectures: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DetectFile {
    #[serde(default)]
    binary: Option<String>,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    debian: Option<String>,
    #[serde(default)]
    fedora: Option<String>,
    #[serde(default)]
    suse: Option<String>,
    #[serde(default)]
    alpine: Option<String>,
    #[serde(default)]
    void: Option<String>,
    #[serde(default)]
    solus: Option<String>,
    #[serde(default)]
    aur: Option<String>,
    #[serde(default)]
    flatpak: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFile {
    #[serde(default)]
    binary: Option<String>,
    #[serde(default)]
    conflicts: Vec<String>,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    debian: Option<String>,
    #[serde(default)]
    fedora: Option<String>,
    #[serde(default)]
    suse: Option<String>,
    #[serde(default)]
    alpine: Option<String>,
    #[serde(default)]
    void: Option<String>,
    #[serde(default)]
    solus: Option<String>,
    /// The Arch package from the AUR, through paru or yay, when `arch` is not given.
    #[serde(default)]
    aur: Option<String>,
    /// The Flathub application id, for families without a package.
    #[serde(default)]
    flatpak: Option<String>,
    #[serde(default)]
    removable: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptFile {
    /// A script under `scripts/installers/`, without the `.sh`.
    #[serde(default)]
    id: Option<String>,
    /// A script beside the entry's TOML file.
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default = "default_true")]
    interactive: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandFile {
    line: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NixFile {
    #[serde(default)]
    profile_install: Option<String>,
    #[serde(default)]
    home_manager_switch: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeFile {
    action: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    entries: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, CatalogError> {
    let text =
        fs::read_to_string(path).map_err(|error| CatalogError::Io(path.to_path_buf(), error))?;
    toml::from_str(&text).map_err(|error| CatalogError::Toml(path.to_path_buf(), error))
}

fn load_tabs(root: &Path, validate: bool) -> Result<Vec<Tab>, CatalogError> {
    let catalog = root.join("catalog");
    let installers = root.join("scripts/installers");
    let index: TabsFile = read(&catalog.join("tabs.toml"))?;
    let mut ids = HashSet::new();
    let mut tabs = Vec::new();
    for id in index.tabs {
        if !valid_id(&id) {
            return Err(CatalogError::Invalid(format!(
                "tabs.toml names an invalid tab id: {id}"
            )));
        }
        let directory = catalog.join(&id);
        let tab: TabFile = read(&directory.join("tab.toml"))?;
        let items = load_items(&directory, &installers, validate, true, &mut ids)?;
        tabs.push(Tab {
            id,
            name: tab.name,
            items,
        });
    }
    if tabs.is_empty() {
        return Err(CatalogError::Invalid("tabs.toml names no tabs".into()));
    }
    check_profiles(&tabs)?;
    Ok(tabs)
}

/// Profiles may only name entries that exist, and may not include themselves
/// directly or through other profiles.
fn check_profiles(tabs: &[Tab]) -> Result<(), CatalogError> {
    let entries: Vec<&Entry> = tabs.iter().flat_map(Tab::entries).collect();
    fn visit(
        entry: &Entry,
        entries: &[&Entry],
        trail: &mut Vec<String>,
    ) -> Result<(), CatalogError> {
        if let Action::Profile { entries: members } = &entry.action {
            if trail.contains(&entry.id) {
                return Err(CatalogError::Invalid(format!(
                    "profile {} includes itself",
                    entry.id
                )));
            }
            trail.push(entry.id.clone());
            for member in members {
                let Some(found) = entries.iter().find(|candidate| &candidate.id == member) else {
                    return Err(CatalogError::Invalid(format!(
                        "profile {} names an unknown entry: {member}",
                        entry.id
                    )));
                };
                visit(found, entries, trail)?;
            }
            trail.pop();
        }
        Ok(())
    }
    for entry in &entries {
        visit(entry, &entries, &mut Vec::new())?;
    }
    Ok(())
}

/// Groups are subdirectories with a group.toml; entries are the other TOML
/// files. Both sort by their `order`, then by name.
fn load_items(
    directory: &Path,
    installers: &Path,
    validate: bool,
    parent_multi_select: bool,
    ids: &mut HashSet<String>,
) -> Result<Vec<Item>, CatalogError> {
    // (0 for a group or 1 for an entry, order, name, item)
    let mut found: Vec<(u8, i64, String, Item)> = Vec::new();
    let listing = fs::read_dir(directory)
        .map_err(|error| CatalogError::Io(directory.to_path_buf(), error))?;
    for child in listing {
        let child = child.map_err(|error| CatalogError::Io(directory.to_path_buf(), error))?;
        let path = child.path();
        let file_name = child.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            let group_file = path.join("group.toml");
            if !group_file.is_file() {
                continue;
            }
            if !valid_id(&file_name) {
                return Err(CatalogError::Invalid(format!(
                    "invalid group id: {}",
                    path.display()
                )));
            }
            let group: GroupFile = read(&group_file)?;
            if validate && !group.preconditions.iter().all(holds) {
                continue;
            }
            let multi_select = parent_multi_select && group.multi_select;
            let items = load_items(&path, installers, validate, multi_select, ids)?;
            if validate && items.is_empty() {
                continue;
            }
            found.push((
                0,
                group.order.unwrap_or(i64::MAX),
                group.name.clone(),
                Item::Group(Group {
                    id: file_name,
                    name: group.name,
                    description: group.description,
                    multi_select,
                    items,
                }),
            ));
        } else if let Some(stem) = file_name.strip_suffix(".toml") {
            if stem == "tab" || stem == "group" {
                continue;
            }
            if !valid_id(stem) {
                return Err(CatalogError::Invalid(format!(
                    "invalid entry id: {}",
                    path.display()
                )));
            }
            let entry: EntryFile = read(&path)?;
            if validate && !entry.preconditions.iter().all(holds) {
                continue;
            }
            let Some(action) = action_of(&entry, &path, installers, validate)? else {
                continue;
            };
            let detect = detect_of(&entry, &path)?;
            let needs = needs_of(&entry, &path)?;
            // For other processors, like a failed precondition.
            if validate && !needs.runs_on(std::env::consts::ARCH) {
                continue;
            }
            let effects = effects_of(&entry, &path)?;
            if !ids.insert(stem.to_string()) {
                return Err(CatalogError::Invalid(format!("duplicate entry id: {stem}")));
            }
            found.push((
                1,
                entry.order.unwrap_or(i64::MAX),
                entry.name.clone(),
                Item::Entry(Entry {
                    id: stem.to_string(),
                    name: entry.name,
                    description: entry.description,
                    flags: entry.flags,
                    multi_select: parent_multi_select && entry.multi_select,
                    writes: entry.writes,
                    action,
                    detect,
                    needs,
                    effects,
                }),
            ));
        }
    }
    // Groups first, then entries; each by order, then name.
    found.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    Ok(found.into_iter().map(|(_, _, _, item)| item).collect())
}

/// The entry's `effects` table, checked: plain text, absolute or home
/// paths, and renderers that only print. A renderer's second argument is the
/// file it reads, which must be the file the effect names.
fn effects_of(entry: &EntryFile, path: &Path) -> Result<Effects, CatalogError> {
    let Some(effects) = &entry.effects else {
        return Ok(Effects::default());
    };
    let invalid = |why: &str| {
        Err(CatalogError::Invalid(format!(
            "{} has an invalid effects section: {why}",
            path.display()
        )))
    };
    if entry.script.is_none() {
        return invalid("only script entries declare effects");
    }
    let plain =
        |text: &str| !text.is_empty() && text.len() <= 400 && !text.chars().any(char::is_control);
    let texts = effects
        .packages
        .iter()
        .chain(&effects.services)
        .chain(&effects.unknown)
        .chain(effects.takes_effect.iter());
    if !texts.into_iter().all(|text| plain(text)) {
        return invalid("a text is empty, too long or has control characters");
    }
    let mut files = Vec::new();
    for file in &effects.files {
        let located = file.path.starts_with('/')
            || file.path.starts_with("~/")
            || file.path.starts_with("{grub}/");
        if !located || file.path.contains("..") || !plain(&file.path) || !plain(&file.change) {
            return invalid("a file needs an absolute or ~/ path and a change");
        }
        if file.when.as_deref().is_some_and(|when| !plain(when)) {
            return invalid("a when is empty or has control characters");
        }
        if !file.render.is_empty() {
            let known = helixforge_core::RENDERERS.contains(&file.render[0].as_str());
            let reads = file.render.get(1) == Some(&file.path);
            let safe = file.render.iter().all(|word| {
                word.len() <= 256 && !word.chars().any(char::is_control) && !word.starts_with('-')
            });
            if !known || !reads || !safe {
                return invalid("render names a renderer that only prints, then the file it reads");
            }
        }
        files.push(FileEffect {
            path: file.path.clone(),
            change: file.change.clone(),
            render: file.render.clone(),
            when: file.when.clone(),
        });
    }
    Ok(Effects {
        files,
        packages: effects.packages.clone(),
        services: effects.services.clone(),
        takes_effect: effects.takes_effect.clone(),
        unknown: effects.unknown.clone(),
    })
}

/// The entry's `needs` table, checked.
fn needs_of(entry: &EntryFile, path: &Path) -> Result<Needs, CatalogError> {
    let Some(needs) = &entry.needs else {
        return Ok(Needs::default());
    };
    let valid = needs
        .gpu
        .iter()
        .all(|vendor| GPU_VENDORS.contains(&vendor.as_str()))
        && needs
            .repositories
            .iter()
            .all(|name| valid_package_name(name))
        && needs.after.iter().all(|id| valid_id(id))
        && needs
            .architectures
            .iter()
            .all(|name| ARCHITECTURES.contains(&name.as_str()));
    if !valid {
        return Err(CatalogError::Invalid(format!(
            "{} has an invalid needs section",
            path.display()
        )));
    }
    Ok(Needs {
        gpu: needs.gpu.clone(),
        repositories: needs.repositories.clone(),
        space_mb: needs.space_mb,
        after: needs.after.clone(),
        architectures: needs.architectures.clone(),
    })
}

/// The entry's `detect` table, checked; a package entry has its own.
fn detect_of(entry: &EntryFile, path: &Path) -> Result<Option<Detect>, CatalogError> {
    let Some(detect) = &entry.detect else {
        return Ok(None);
    };
    let packages = PackageNames {
        arch: detect.arch.clone(),
        debian: detect.debian.clone(),
        fedora: detect.fedora.clone(),
        suse: detect.suse.clone(),
        alpine: detect.alpine.clone(),
        void: detect.void.clone(),
        solus: detect.solus.clone(),
    };
    let valid = entry.package.is_none()
        && packages
            .all()
            .iter()
            .filter_map(|(_, name)| *name)
            .chain(detect.aur.as_deref())
            .all(valid_package_name)
        && detect.binary.as_deref().is_none_or(valid_binary_name)
        && detect.flatpak.as_deref().is_none_or(valid_flatpak_id)
        && (detect.binary.is_some()
            || !packages.is_empty()
            || detect.aur.is_some()
            || detect.flatpak.is_some());
    if !valid {
        return Err(CatalogError::Invalid(format!(
            "{} has an invalid detect section",
            path.display()
        )));
    }
    Ok(Some(Detect {
        binary: detect.binary.clone(),
        packages,
        aur: detect.aur.clone(),
        flatpak: detect.flatpak.clone(),
    }))
}

/// The entry's action; `None` when what it needs (a script's interpreter,
/// Nix) is missing and validation is on, so the entry is hidden like one
/// with failed preconditions.
fn action_of(
    entry: &EntryFile,
    path: &Path,
    installers: &Path,
    validate: bool,
) -> Result<Option<Action>, CatalogError> {
    let kinds = usize::from(entry.package.is_some())
        + usize::from(entry.script.is_some())
        + usize::from(entry.command.is_some())
        + usize::from(entry.nix.is_some())
        + usize::from(entry.native.is_some())
        + usize::from(entry.profile.is_some());
    if kinds != 1 {
        return Err(CatalogError::Invalid(format!(
            "{} must have exactly one of [package], [script], [command], [nix], [native] or [profile]",
            path.display()
        )));
    }
    if let Some(package) = &entry.package {
        let packages = PackageNames {
            arch: package.arch.clone(),
            debian: package.debian.clone(),
            fedora: package.fedora.clone(),
            suse: package.suse.clone(),
            alpine: package.alpine.clone(),
            void: package.void.clone(),
            solus: package.solus.clone(),
        };
        let names_valid = packages
            .all()
            .iter()
            .filter_map(|(_, name)| *name)
            .chain(package.aur.as_deref())
            .chain(package.conflicts.iter().map(String::as_str))
            .all(valid_package_name);
        let has_source = !packages.is_empty() || package.aur.is_some() || package.flatpak.is_some();
        if package
            .binary
            .as_deref()
            .is_some_and(|binary| !valid_binary_name(binary))
            || !names_valid
            || !has_source
            || package
                .flatpak
                .as_deref()
                .is_some_and(|id| !valid_flatpak_id(id))
        {
            return Err(CatalogError::Invalid(format!(
                "{} has an invalid package section",
                path.display()
            )));
        }
        return Ok(Some(Action::Package {
            binary: package.binary.clone(),
            packages,
            aur: package.aur.clone(),
            flatpak: package.flatpak.clone(),
            conflicts: package.conflicts.clone(),
            removable: package.removable,
        }));
    }
    if let Some(script) = &entry.script {
        let file = match (&script.id, &script.path) {
            (Some(id), None) => {
                // A script id may name a subdirectory of installers/, one valid id per segment.
                if id.is_empty() || !id.split('/').all(valid_script_segment) {
                    return Err(CatalogError::Invalid(format!(
                        "{} names an invalid script id: {id}",
                        path.display()
                    )));
                }
                installers.join(format!("{id}.sh"))
            }
            (None, Some(relative)) => {
                if relative.is_absolute()
                    || relative.components().any(|part| part.as_os_str() == "..")
                {
                    return Err(CatalogError::Invalid(format!(
                        "{} names a script outside its directory",
                        path.display()
                    )));
                }
                path.parent().unwrap_or(Path::new(".")).join(relative)
            }
            _ => {
                return Err(CatalogError::Invalid(format!(
                    "{} needs either a script id or a path",
                    path.display()
                )));
            }
        };
        if !file.is_file() {
            return Err(CatalogError::Invalid(format!(
                "{} names a missing script: {}",
                path.display(),
                file.display()
            )));
        }
        if validate && !interpreter_available(&file) {
            return Ok(None);
        }
        return Ok(Some(Action::Script {
            file,
            interactive: script.interactive,
        }));
    }
    if let Some(command) = &entry.command {
        if command.line.trim().is_empty() {
            return Err(CatalogError::Invalid(format!(
                "{} has an empty command",
                path.display()
            )));
        }
        return Ok(Some(Action::Command {
            line: command.line.clone(),
        }));
    }
    if let Some(nix) = &entry.nix {
        let action = match (&nix.profile_install, &nix.home_manager_switch) {
            (Some(installable), None) if !installable.trim().is_empty() => {
                NixAction::ProfileInstall {
                    installable: installable.clone(),
                }
            }
            (None, Some(flake)) if !flake.trim().is_empty() => NixAction::HomeManagerSwitch {
                flake: flake.clone(),
            },
            _ => {
                return Err(CatalogError::Invalid(format!(
                    "{} needs one Nix action",
                    path.display()
                )));
            }
        };
        if validate && !binary_exists("nix") {
            return Ok(None);
        }
        return Ok(Some(Action::Nix { action }));
    }
    if let Some(native) = &entry.native {
        let Some(action) = NativeAction::parse(&native.action) else {
            return Err(CatalogError::Invalid(format!(
                "{} names an unknown native action: {}",
                path.display(),
                native.action
            )));
        };
        return Ok(Some(Action::Native { action }));
    }
    let profile = entry.profile.as_ref().expect("checked above");
    if profile.entries.is_empty() || profile.entries.iter().any(|id| !valid_id(id)) {
        return Err(CatalogError::Invalid(format!(
            "{} needs a list of entry ids",
            path.display()
        )));
    }
    Ok(Some(Action::Profile {
        entries: profile.entries.clone(),
    }))
}

/// The interpreter named by a shebang, or `/bin/sh -e` without one; scripts
/// are run as `interpreter [args] script`.
pub fn interpreter(file: &Path) -> (String, Vec<String>) {
    let fallback = || ("/bin/sh".to_string(), vec!["-e".to_string()]);
    let Ok(text) = fs::read_to_string(file) else {
        return fallback();
    };
    let Some(first) = text.lines().next() else {
        return fallback();
    };
    let Some(rest) = first.strip_prefix("#!") else {
        return fallback();
    };
    let mut parts = rest.split_whitespace();
    match parts.next() {
        Some(program) => (program.to_string(), parts.map(str::to_string).collect()),
        None => fallback(),
    }
}

fn interpreter_available(file: &Path) -> bool {
    let (program, _) = interpreter(file);
    let path = Path::new(&program);
    if path.is_absolute() {
        return path.is_file();
    }
    binary_exists(&program)
}

fn holds(condition: &Precondition) -> bool {
    match &condition.check {
        Check::Environment(name) => std::env::var(name)
            .is_ok_and(|value| condition.values.contains(&value) == condition.matches),
        Check::ContainingFile(path) => fs::read_to_string(path).is_ok_and(|text| {
            condition
                .values
                .iter()
                .all(|value| text.contains(value) == condition.matches)
        }),
        Check::CommandExists => condition
            .values
            .iter()
            .all(|command| binary_exists(command) == condition.matches),
        Check::FileExists => condition
            .values
            .iter()
            .all(|path| Path::new(path).is_file() == condition.matches),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_tree_with_files_at_its_top_unpacks_into_a_new_directory() {
        let parent = tempfile::tempdir().unwrap();
        let target = parent.path().join("scripts");
        unpack(&EMBEDDED_SCRIPTS, &target).unwrap();
        // Files at the top of scripts/, beside its folders.
        assert!(target.join("verify.sh").is_file());
        assert!(target.join("package.sh").is_file());
        assert!(target.join("installers").is_dir());
    }

    use super::*;

    #[test]
    fn a_sweep_removes_copies_whose_process_ended_and_keeps_live_ones() {
        let parent = tempfile::tempdir().unwrap();
        let live = Unpacked::create(parent.path()).unwrap();
        fs::write(live.path().join("data"), "x").unwrap();
        // A copy whose process was killed: the lock file stays, nobody holds it.
        let dead = Unpacked::create(parent.path()).unwrap();
        let dead_path = dead.path().to_path_buf();
        let kept = dead.directory.keep();
        drop(dead._lock);
        assert!(kept.join(LOCK_FILE).exists());
        // Something else with the prefix and no lock file is not ours to remove.
        let foreign = parent.path().join("helixforge-foreign");
        fs::create_dir(&foreign).unwrap();
        // Nor is a plain file with the prefix.
        fs::write(parent.path().join("helixforge-file"), "x").unwrap();
        assert_eq!(sweep_stale(parent.path()), 1);
        assert!(!dead_path.exists());
        assert!(live.path().join("data").exists());
        assert!(foreign.exists());
        assert!(parent.path().join("helixforge-file").exists());
        let live_path = live.path().to_path_buf();
        drop(live);
        assert!(!live_path.exists());
    }

    #[test]
    fn the_builtin_catalog_unpacks_into_a_locked_copy_that_is_removed_after_use() {
        let catalog = Catalog::builtin(false).unwrap();
        let root = catalog.root.clone();
        assert!(root.join(LOCK_FILE).exists());
        assert!(root.join("scripts/shared").is_dir());
        // A sweep from another process must leave a copy in use alone.
        assert!(
            File::open(root.join(LOCK_FILE))
                .unwrap()
                .try_lock()
                .is_err()
        );
        drop(catalog);
        assert!(!root.exists());
    }

    fn write(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn sample() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write(root, "catalog/tabs.toml", "tabs = [\"apps\"]\n");
        write(root, "catalog/apps/tab.toml", "name = \"Applications\"\n");
        write(
            root,
            "catalog/apps/tools/group.toml",
            "name = \"Tools\"\nmulti_select = false\norder = 1\n",
        );
        write(
            root,
            "catalog/apps/tools/git.toml",
            "name = \"Git\"\ndescription = \"Version control.\"\nflags = \"I\"\n\n[package]\nbinary = \"git\"\narch = \"git\"\ndebian = \"git\"\n",
        );
        write(
            root,
            "catalog/apps/tools/hello.toml",
            "name = \"Hello\"\norder = 1\n\n[script]\npath = \"hello.sh\"\n",
        );
        write(
            root,
            "catalog/apps/tools/hello.sh",
            "#!/bin/sh\necho hello\n",
        );
        write(
            root,
            "catalog/apps/tools/shared.toml",
            "name = \"Shared\"\norder = 2\n\n[script]\nid = \"shared\"\ninteractive = false\n",
        );
        write(
            root,
            "scripts/installers/shared.sh",
            "#!/bin/sh\necho shared\n",
        );
        write(
            root,
            "catalog/apps/tools/hidden.toml",
            "name = \"Hidden\"\n\n[[preconditions]]\nmatches = true\ncheck = \"command_exists\"\nvalues = [\"no-such-command-helixforge\"]\n\n[command]\nline = \"true\"\n",
        );
        // For another processor than this one.
        let other = if std::env::consts::ARCH == "x86_64" {
            "aarch64"
        } else {
            "x86_64"
        };
        write(
            root,
            "catalog/apps/tools/elsewhere.toml",
            &format!(
                "name = \"Elsewhere\"\n\n[command]\nline = \"true\"\n\n[needs]\narchitectures = [\"{other}\"]\n"
            ),
        );
        write(
            root,
            "catalog/apps/tools/exotic.toml",
            "name = \"Exotic\"\n\n[script]\npath = \"exotic.sh\"\n",
        );
        write(
            root,
            "catalog/apps/tools/exotic.sh",
            "#!/no/such/interpreter\n",
        );
        write(
            root,
            "catalog/apps/extras/group.toml",
            "name = \"Extras\"\norder = 9\n",
        );
        write(
            root,
            "catalog/apps/extras/later.toml",
            "name = \"Later\"\n\n[command]\nline = \"echo later\"\n",
        );
        write(
            root,
            "catalog/apps/zed.toml",
            "name = \"Zed\"\norder = 5\n\n[command]\nline = \"echo zed\"\n",
        );
        write(
            root,
            "catalog/apps/only-here.toml",
            "name = \"Only here\"\n\n[[preconditions]]\nmatches = true\ncheck = { containing_file = \"/etc/os-release\" }\nvalues = [\"ID=\"]\n\n[command]\nline = \"true\"\n",
        );
        write(
            root,
            "catalog/apps/report.toml",
            "name = \"Report\"\norder = 6\n\n[native]\naction = \"system-report\"\n",
        );
        write(
            root,
            "catalog/apps/basics.toml",
            "name = \"Basics\"\norder = 7\n\n[profile]\nentries = [\"git\", \"hello\"]\n",
        );
        directory
    }

    #[test]
    fn the_tree_is_ordered_and_filtered() {
        let directory = sample();
        let catalog = Catalog::from_directory(directory.path(), true).unwrap();
        let tab = &catalog.tabs[0];
        let names: Vec<&str> = tab.items.iter().map(Item::name).collect();
        assert_eq!(
            names,
            ["Tools", "Extras", "Zed", "Report", "Basics", "Only here"],
            "groups before entries, whatever their order"
        );
        let Item::Group(tools) = &tab.items[0] else {
            panic!("expected a group");
        };
        let names: Vec<&str> = tools.items.iter().map(Item::name).collect();
        assert_eq!(
            names,
            ["Hello", "Shared", "Git"],
            "order before name; hidden, elsewhere and exotic left out"
        );
        let Item::Entry(shared) = &tools.items[1] else {
            panic!("expected an entry");
        };
        assert!(matches!(
            &shared.action,
            Action::Script { file, interactive: false } if file.ends_with("scripts/installers/shared.sh")
        ));
        let Item::Entry(git) = &tools.items[2] else {
            panic!("expected an entry");
        };
        assert!(!git.multi_select, "the group turns multi-select off");
        assert_eq!(git.flags, "I");
        assert_eq!(
            catalog.find("hello").map(|entry| entry.name.as_str()),
            Some("Hello")
        );
        assert_eq!(catalog.entries().len(), 8);
        assert!(matches!(
            catalog.find("report").unwrap().action,
            Action::Native {
                action: NativeAction::SystemReport
            }
        ));
        assert!(matches!(
            &catalog.find("basics").unwrap().action,
            Action::Profile { entries } if entries == &["git", "hello"]
        ));
        assert!(catalog.lib_dir().ends_with("scripts/shared"));
    }

    #[test]
    fn unvalidated_loading_keeps_everything() {
        let directory = sample();
        let catalog = Catalog::from_directory(directory.path(), false).unwrap();
        assert_eq!(catalog.entries().len(), 11);
        assert!(
            !catalog
                .find("elsewhere")
                .unwrap()
                .needs
                .architectures
                .is_empty()
        );
    }

    #[test]
    fn script_entries_say_what_they_install() {
        let directory = sample();
        write(
            directory.path(),
            "catalog/apps/steamish.toml",
            "name = \"Steamish\"\n[command]\nline = \"true\"\n\n[detect]\nbinary = \"steam\"\narch = \"steam\"\nflatpak = \"com.valvesoftware.Steam\"\n",
        );
        let catalog = Catalog::from_directory(directory.path(), false).unwrap();
        let entry = catalog.find("steamish").unwrap();
        let identity = entry.identity().unwrap();
        assert_eq!(identity.binary, Some("steam"));
        assert_eq!(identity.packages.arch.as_deref(), Some("steam"));
        assert_eq!(identity.flatpak, Some("com.valvesoftware.Steam"));
        assert!(identity.conflicts.is_empty());
        assert!(catalog.find("zed").unwrap().identity().is_none());
        // Every built-in entry that says what it installs loads.
        let builtin = Catalog::builtin(false).unwrap();
        assert!(builtin.find("steam").unwrap().identity().is_some());
    }

    #[test]
    fn broken_entries_are_errors() {
        for (name, text) in [
            (
                "two.toml",
                "name = \"Two\"\n[command]\nline = \"true\"\n[script]\npath = \"x.sh\"\n",
            ),
            (
                "escape.toml",
                "name = \"Escape\"\n[script]\npath = \"../x.sh\"\n",
            ),
            (
                "missing.toml",
                "name = \"Missing\"\n[script]\npath = \"nope.sh\"\n",
            ),
            (
                "flag.toml",
                "name = \"Flag\"\n[package]\nbinary = \"x\"\narch = \"--overwrite\"\n",
            ),
            (
                "aurflag.toml",
                "name = \"Aur\"\n[package]\nbinary = \"x\"\naur = \"-x\"\n",
            ),
            (
                "sparc.toml",
                "name = \"Sparc\"\n[command]\nline = \"true\"\n[needs]\narchitectures = [\"sparc\"]\n",
            ),
            (
                "flat.toml",
                "name = \"Flat\"\n[package]\nbinary = \"x\"\nflatpak = \"not an id\"\n",
            ),
            (
                "nothing.toml",
                "name = \"Nothing\"\n[package]\nbinary = \"x\"\nremovable = true\n",
            ),
            (
                "detectflag.toml",
                "name = \"Detect\"\n[command]\nline = \"true\"\n[detect]\narch = \"--overwrite\"\n",
            ),
            (
                "detectempty.toml",
                "name = \"Empty\"\n[command]\nline = \"true\"\n[detect]\n",
            ),
            (
                "detectpackage.toml",
                "name = \"Both\"\n[package]\narch = \"x\"\n[detect]\narch = \"x\"\n",
            ),
            (
                "unknown.toml",
                "name = \"Unknown\"\nbogus = 1\n[command]\nline = \"true\"\n",
            ),
            (
                "native.toml",
                "name = \"Native\"\n[native]\naction = \"format-disk\"\n",
            ),
            ("nix.toml", "name = \"Nix\"\n[nix]\n"),
            (
                "updir.toml",
                "name = \"Updir\"\n[script]\nid = \"../etc/x\"\n",
            ),
            (
                "both.toml",
                "name = \"Both\"\n[script]\nid = \"a\"\npath = \"b.sh\"\n",
            ),
            (
                "ghost.toml",
                "name = \"Ghost\"\n[profile]\nentries = [\"nope\"]\n",
            ),
            (
                "loop.toml",
                "name = \"Loop\"\n[profile]\nentries = [\"loop\"]\n",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            write(directory.path(), "catalog/tabs.toml", "tabs = [\"t\"]\n");
            write(directory.path(), "catalog/t/tab.toml", "name = \"T\"\n");
            write(directory.path(), &format!("catalog/t/{name}"), text);
            assert!(
                Catalog::from_directory(directory.path(), false).is_err(),
                "{name} was accepted"
            );
        }
    }

    #[test]
    fn shebangs_pick_the_interpreter() {
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("s.sh");
        fs::write(&script, "#!/usr/bin/env bash\necho\n").unwrap();
        assert_eq!(
            interpreter(&script),
            ("/usr/bin/env".to_string(), vec!["bash".to_string()])
        );
        fs::write(&script, "echo\n").unwrap();
        assert_eq!(
            interpreter(&script),
            ("/bin/sh".to_string(), vec!["-e".to_string()])
        );
    }

    #[test]
    fn the_embedded_catalog_loads() {
        let catalog = Catalog::builtin(false).unwrap();
        assert_eq!(catalog.tabs.len(), 6);
        assert!(catalog.find("git").is_some());
        assert!(catalog.find("system-report").is_some());
        // The installers from the previous toolbox and the general catalog.
        assert!(catalog.find("dotfiles-nvim").is_some());
        assert!(catalog.find("discord").is_some());
        assert!(catalog.find("neovim-titus").is_some());
        assert!(catalog.entries().len() >= 230);
        assert!(catalog.lib_dir().join("common-script.sh").is_file());
    }
}
