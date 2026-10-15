//! The catalog on disk: `catalog/tabs.toml` names the tabs in order; each tab
//! is a directory with `tab.toml`, and inside it entries are `<id>.toml` files
//! and groups are directories with `group.toml`. A script entry keeps its
//! script beside its TOML file. The whole tree is embedded into the binary and
//! unpacked to a temporary directory when the application starts, so scripts
//! are real files when they run.

use helixforge_core::{
    Action, Check, Entry, Group, Item, PackageNames, Precondition, Tab, valid_id,
    valid_package_name,
};
use helixforge_platform::binary_exists;
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};
use thiserror::Error;

static EMBEDDED: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../catalog");

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
    /// The directory the catalog was read from; `lib/` inside it holds the
    /// shared shell helpers.
    pub root: PathBuf,
    /// Keeps the unpacked tree alive while the catalog is used.
    _unpacked: Option<tempfile::TempDir>,
}

impl Catalog {
    /// Unpack the embedded catalog and read it. With `validate`, entries whose
    /// preconditions fail on this machine are left out.
    pub fn builtin(validate: bool) -> Result<Self, CatalogError> {
        let directory = tempfile::Builder::new()
            .prefix("helixforge-")
            .tempdir()
            .map_err(|error| CatalogError::Io(PathBuf::from("temporary directory"), error))?;
        EMBEDDED
            .extract(directory.path())
            .map_err(|error| CatalogError::Io(directory.path().to_path_buf(), error))?;
        let tabs = load_tabs(directory.path(), validate)?;
        Ok(Self {
            tabs,
            root: directory.path().to_path_buf(),
            _unpacked: Some(directory),
        })
    }

    /// Read a catalog from a directory on disk.
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

    /// The shared shell helpers that scripts source through `HELIXFORGE_LIB`.
    pub fn lib_dir(&self) -> PathBuf {
        self.root.join("lib")
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
    package: Option<PackageFile>,
    #[serde(default)]
    script: Option<ScriptFile>,
    #[serde(default)]
    command: Option<CommandFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageFile {
    binary: String,
    #[serde(default)]
    arch: Option<String>,
    #[serde(default)]
    debian: Option<String>,
    #[serde(default)]
    fedora: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ScriptFile {
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CommandFile {
    line: String,
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
    let index: TabsFile = read(&root.join("tabs.toml"))?;
    let mut ids = HashSet::new();
    let mut tabs = Vec::new();
    for id in index.tabs {
        if !valid_id(&id) {
            return Err(CatalogError::Invalid(format!(
                "tabs.toml names an invalid tab id: {id}"
            )));
        }
        let directory = root.join(&id);
        let tab: TabFile = read(&directory.join("tab.toml"))?;
        let items = load_items(&directory, validate, true, &mut ids)?;
        tabs.push(Tab {
            id,
            name: tab.name,
            items,
        });
    }
    if tabs.is_empty() {
        return Err(CatalogError::Invalid("tabs.toml names no tabs".into()));
    }
    Ok(tabs)
}

/// Groups are subdirectories with a group.toml; entries are the other TOML
/// files. Both sort by their `order`, then by name.
fn load_items(
    directory: &Path,
    validate: bool,
    parent_multi_select: bool,
    ids: &mut HashSet<String>,
) -> Result<Vec<Item>, CatalogError> {
    let mut found: Vec<(i64, String, Item)> = Vec::new();
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
            let items = load_items(&path, validate, multi_select, ids)?;
            if validate && items.is_empty() {
                continue;
            }
            found.push((
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
            let action = action_of(&entry, &path, validate)?;
            let Some(action) = action else {
                continue;
            };
            if !ids.insert(stem.to_string()) {
                return Err(CatalogError::Invalid(format!("duplicate entry id: {stem}")));
            }
            found.push((
                entry.order.unwrap_or(i64::MAX),
                entry.name.clone(),
                Item::Entry(Entry {
                    id: stem.to_string(),
                    name: entry.name,
                    description: entry.description,
                    flags: entry.flags,
                    multi_select: parent_multi_select && entry.multi_select,
                    action,
                }),
            ));
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    Ok(found.into_iter().map(|(_, _, item)| item).collect())
}

/// The entry's action; `None` when a script's interpreter is missing and
/// validation is on, so the entry is hidden like one with failed preconditions.
fn action_of(
    entry: &EntryFile,
    path: &Path,
    validate: bool,
) -> Result<Option<Action>, CatalogError> {
    let kinds = usize::from(entry.package.is_some())
        + usize::from(entry.script.is_some())
        + usize::from(entry.command.is_some());
    if kinds != 1 {
        return Err(CatalogError::Invalid(format!(
            "{} must have exactly one of [package], [script] or [command]",
            path.display()
        )));
    }
    if let Some(package) = &entry.package {
        let packages = PackageNames {
            arch: package.arch.clone(),
            debian: package.debian.clone(),
            fedora: package.fedora.clone(),
        };
        if !valid_package_name(&package.binary)
            || packages.is_empty()
            || [&packages.arch, &packages.debian, &packages.fedora]
                .into_iter()
                .flatten()
                .any(|name| !valid_package_name(name))
        {
            return Err(CatalogError::Invalid(format!(
                "{} has an invalid package section",
                path.display()
            )));
        }
        return Ok(Some(Action::Package {
            binary: package.binary.clone(),
            packages,
        }));
    }
    if let Some(script) = &entry.script {
        if script.path.is_absolute()
            || script
                .path
                .components()
                .any(|part| part.as_os_str() == "..")
        {
            return Err(CatalogError::Invalid(format!(
                "{} names a script outside its directory",
                path.display()
            )));
        }
        let file = path.parent().unwrap_or(Path::new(".")).join(&script.path);
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
        return Ok(Some(Action::Script { file }));
    }
    let command = entry.command.as_ref().expect("checked above");
    if command.line.trim().is_empty() {
        return Err(CatalogError::Invalid(format!(
            "{} has an empty command",
            path.display()
        )));
    }
    Ok(Some(Action::Command {
        line: command.line.clone(),
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
    use super::*;

    fn write(root: &Path, name: &str, text: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn sample() -> tempfile::TempDir {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        write(root, "tabs.toml", "tabs = [\"apps\"]\n");
        write(root, "apps/tab.toml", "name = \"Applications\"\n");
        write(
            root,
            "apps/tools/group.toml",
            "name = \"Tools\"\nmulti_select = false\norder = 1\n",
        );
        write(
            root,
            "apps/tools/git.toml",
            "name = \"Git\"\ndescription = \"Version control.\"\nflags = \"I\"\n\n[package]\nbinary = \"git\"\narch = \"git\"\ndebian = \"git\"\n",
        );
        write(
            root,
            "apps/tools/hello.toml",
            "name = \"Hello\"\norder = 1\n\n[script]\npath = \"hello.sh\"\n",
        );
        write(root, "apps/tools/hello.sh", "#!/bin/sh\necho hello\n");
        write(
            root,
            "apps/tools/hidden.toml",
            "name = \"Hidden\"\n\n[[preconditions]]\nmatches = true\ncheck = \"command_exists\"\nvalues = [\"no-such-command-helixforge\"]\n\n[command]\nline = \"true\"\n",
        );
        write(
            root,
            "apps/tools/exotic.toml",
            "name = \"Exotic\"\n\n[script]\npath = \"exotic.sh\"\n",
        );
        write(root, "apps/tools/exotic.sh", "#!/no/such/interpreter\n");
        write(
            root,
            "apps/zed.toml",
            "name = \"Zed\"\norder = 5\n\n[command]\nline = \"echo zed\"\n",
        );
        write(
            root,
            "apps/only-here.toml",
            "name = \"Only here\"\n\n[[preconditions]]\nmatches = true\ncheck = { containing_file = \"/etc/os-release\" }\nvalues = [\"ID=\"]\n\n[command]\nline = \"true\"\n",
        );
        directory
    }

    #[test]
    fn the_tree_is_ordered_and_filtered() {
        let directory = sample();
        let catalog = Catalog::from_directory(directory.path(), true).unwrap();
        let tab = &catalog.tabs[0];
        let names: Vec<&str> = tab.items.iter().map(Item::name).collect();
        assert_eq!(names, ["Tools", "Zed", "Only here"]);
        let Item::Group(tools) = &tab.items[0] else {
            panic!("expected a group");
        };
        let names: Vec<&str> = tools.items.iter().map(Item::name).collect();
        assert_eq!(
            names,
            ["Hello", "Git"],
            "order before name; hidden and exotic left out"
        );
        let Item::Entry(git) = &tools.items[1] else {
            panic!("expected an entry");
        };
        assert!(!git.multi_select, "the group turns multi-select off");
        assert_eq!(git.flags, "I");
        assert_eq!(
            catalog.find("hello").map(|entry| entry.name.as_str()),
            Some("Hello")
        );
        assert_eq!(catalog.entries().len(), 4);
    }

    #[test]
    fn unvalidated_loading_keeps_everything() {
        let directory = sample();
        let catalog = Catalog::from_directory(directory.path(), false).unwrap();
        assert_eq!(catalog.entries().len(), 6);
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
                "unknown.toml",
                "name = \"Unknown\"\nbogus = 1\n[command]\nline = \"true\"\n",
            ),
        ] {
            let directory = tempfile::tempdir().unwrap();
            write(directory.path(), "tabs.toml", "tabs = [\"t\"]\n");
            write(directory.path(), "t/tab.toml", "name = \"T\"\n");
            write(directory.path(), &format!("t/{name}"), text);
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
        assert!(!catalog.tabs.is_empty());
        assert!(catalog.find("git").is_some());
    }
}
