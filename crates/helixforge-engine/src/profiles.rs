//! Workstation profiles of the user's own: a name, a description and the
//! entries to run, kept as TOML in `$XDG_CONFIG_HOME/helixforge/profiles/`,
//! exported and imported as the same files. Each becomes a profile entry in
//! the catalog's Profiles area, under "Your profiles", so it is planned,
//! queued and run like the built-in ones.

use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Group, Item, printable, valid_id};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Prefix of the ids user profiles have in the catalog.
pub const PREFIX: &str = "profile-";
/// The id of the group they appear in.
pub const GROUP: &str = "your-profiles";

/// A profile as stored and exported.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// Catalog entry ids, run in this order (dependencies still go first).
    pub entries: Vec<String>,
}

/// Why a profile cannot be used.
#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("cannot read {0}: {1}")]
    Read(PathBuf, String),
    #[error("{0} is not a HelixForge profile: {1}")]
    Format(PathBuf, String),
    #[error("the profile has no entries")]
    Empty,
    #[error("the profile's name or description contains control characters")]
    Control,
    #[error("the profile names entries this catalog does not have: {}", .0.join(", "))]
    Unknown(Vec<String>),
    #[error("the name {0:?} gives no usable file name; use letters or digits")]
    Name(String),
    #[error("cannot write {0}: {1}")]
    Write(PathBuf, String),
}

/// `$XDG_CONFIG_HOME/helixforge/profiles`, or under `~/.config`.
pub fn dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("helixforge/profiles"))
}

/// A file name from a profile name: lower case letters, digits and dashes.
pub fn slug(name: &str) -> Option<String> {
    let mut slug = String::new();
    for character in name.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let slug = slug.trim_end_matches('-').to_string();
    (!slug.is_empty() && valid_id(&format!("{PREFIX}{slug}"))).then_some(slug)
}

/// Read and check one profile file against the catalog.
pub fn read(path: &Path, catalog: &Catalog) -> Result<Profile, ProfileError> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| ProfileError::Read(path.to_path_buf(), error.to_string()))?;
    let profile: Profile = toml::from_str(&text)
        .map_err(|error| ProfileError::Format(path.to_path_buf(), printable(error.message())))?;
    check(&profile, catalog)?;
    Ok(profile)
}

/// Every entry must exist in the catalog, there must be one, and the name
/// and description must be plain text: a shared file could otherwise carry
/// terminal escape sequences into the menu.
pub fn check(profile: &Profile, catalog: &Catalog) -> Result<(), ProfileError> {
    if profile.entries.is_empty() {
        return Err(ProfileError::Empty);
    }
    if [&profile.name, &profile.description]
        .iter()
        .any(|text| text.chars().any(char::is_control))
    {
        return Err(ProfileError::Control);
    }
    let unknown: Vec<String> = profile
        .entries
        .iter()
        .filter(|id| catalog.find(id).is_none())
        .map(|id| printable(id))
        .collect();
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(ProfileError::Unknown(unknown))
    }
}

/// Write a profile into `directory` under its name's slug; an existing one
/// of that name is replaced. Returns the file.
pub fn save(
    directory: &Path,
    profile: &Profile,
    catalog: &Catalog,
) -> Result<PathBuf, ProfileError> {
    check(profile, catalog)?;
    let slug = slug(&profile.name).ok_or_else(|| ProfileError::Name(profile.name.clone()))?;
    let path = directory.join(format!("{slug}.toml"));
    write(&path, profile)?;
    Ok(path)
}

/// Write a profile to any file, as `export` does.
pub fn write(path: &Path, profile: &Profile) -> Result<(), ProfileError> {
    let failed = |error: String| ProfileError::Write(path.to_path_buf(), error);
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent).map_err(|error| failed(error.to_string()))?;
    }
    let text = toml::to_string(profile).map_err(|error| failed(error.to_string()))?;
    // Beside it first, then renamed, so a crash never leaves half a file.
    let staging = path.with_extension("toml.new");
    std::fs::write(
        &staging,
        format!("# A HelixForge workstation profile: helixforge profile import <file>\n{text}"),
    )
    .map_err(|error| failed(error.to_string()))?;
    std::fs::rename(&staging, path).map_err(|error| failed(error.to_string()))
}

/// The profiles in `directory`, each with its slug; unreadable or invalid
/// ones are returned as problems instead.
pub fn load_all(directory: &Path, catalog: &Catalog) -> (Vec<(String, Profile)>, Vec<String>) {
    let mut found = Vec::new();
    let mut problems = Vec::new();
    let Ok(listing) = std::fs::read_dir(directory) else {
        return (found, problems);
    };
    let mut paths: Vec<PathBuf> = listing
        .flatten()
        .map(|item| item.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect();
    paths.sort();
    for path in paths {
        let Some(slug) = path
            .file_stem()
            .and_then(|stem| slug(&stem.to_string_lossy()))
        else {
            problems.push(format!(
                "{}: not a usable profile file name",
                path.display()
            ));
            continue;
        };
        match read(&path, catalog) {
            Ok(profile) => found.push((slug, profile)),
            Err(error) => problems.push(error.to_string()),
        }
    }
    (found, problems)
}

/// A profile as a catalog entry.
pub fn entry(slug: &str, profile: &Profile) -> Entry {
    Entry {
        id: format!("{PREFIX}{slug}"),
        name: printable(&profile.name),
        description: if profile.description.is_empty() {
            format!("Your profile: {}.", printable(&profile.entries.join(", ")))
        } else {
            printable(&profile.description)
        },
        flags: String::new(),
        multi_select: true,
        writes: Vec::new(),
        action: Action::Profile {
            entries: profile.entries.clone(),
        },
        detect: None,
        needs: Default::default(),
    }
}

/// Add the user's profiles to the catalog's Profiles area as "Your profiles",
/// replacing an earlier copy of that group. Returns the problems found.
pub fn add_to(catalog: &mut Catalog, directory: &Path) -> Vec<String> {
    let (profiles, problems) = load_all(directory, catalog);
    let Some(tab) = catalog.tabs.iter_mut().find(|tab| tab.id == "profiles") else {
        return problems;
    };
    tab.items
        .retain(|item| !matches!(item, Item::Group(group) if group.id == GROUP));
    if !profiles.is_empty() {
        tab.items.insert(
            0,
            Item::Group(Group {
                id: GROUP.to_string(),
                name: "Your profiles".to_string(),
                description: format!("Saved in {}", directory.display()),
                multi_select: true,
                items: profiles
                    .iter()
                    .map(|(slug, profile)| Item::Entry(entry(slug, profile)))
                    .collect(),
            }),
        );
    }
    problems
}

/// The entries of a planned queue as a profile: profile entries contribute
/// their members, so a profile is never nested in another.
pub fn from_queue(name: &str, entries: &[Entry]) -> Profile {
    let mut ids: Vec<String> = Vec::new();
    for entry in entries {
        let members = match &entry.action {
            Action::Profile { entries } => entries.clone(),
            _ => vec![entry.id.clone()],
        };
        for id in members {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    Profile {
        name: name.trim().to_string(),
        description: String::new(),
        entries: ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str, entries: &[&str]) -> Profile {
        Profile {
            name: name.into(),
            description: String::new(),
            entries: entries.iter().map(|id| id.to_string()).collect(),
        }
    }

    #[test]
    fn names_become_file_names() {
        assert_eq!(slug("My Workstation!").as_deref(), Some("my-workstation"));
        assert_eq!(
            slug("  Gaming / RuneScape  ").as_deref(),
            Some("gaming-runescape")
        );
        assert_eq!(slug("???"), None);
        assert_eq!(slug(""), None);
    }

    #[test]
    fn profiles_are_saved_loaded_checked_and_listed() {
        let catalog = Catalog::builtin(false).unwrap();
        let directory = tempfile::tempdir().unwrap();
        let saved = save(
            directory.path(),
            &profile("Dev Box", &["git", "ripgrep", "dotfiles-nvim"]),
            &catalog,
        )
        .unwrap();
        assert_eq!(saved, directory.path().join("dev-box.toml"));
        assert!(
            std::fs::read_to_string(&saved)
                .unwrap()
                .starts_with("# A HelixForge workstation profile")
        );
        assert_eq!(
            read(&saved, &catalog).unwrap().entries,
            ["git", "ripgrep", "dotfiles-nvim"]
        );
        // Saving under the same name replaces it.
        save(directory.path(), &profile("dev box", &["git"]), &catalog).unwrap();
        assert_eq!(read(&saved, &catalog).unwrap().entries, ["git"]);
        assert!(matches!(
            save(directory.path(), &profile("Bad", &["git", "no-such-entry"]), &catalog),
            Err(ProfileError::Unknown(unknown)) if unknown == ["no-such-entry"]
        ));
        assert!(matches!(
            save(directory.path(), &profile("Empty", &[]), &catalog),
            Err(ProfileError::Empty)
        ));
        assert!(matches!(
            save(directory.path(), &profile("!!!", &["git"]), &catalog),
            Err(ProfileError::Name(_))
        ));
        // Escape sequences in a shared file never reach the terminal.
        assert!(matches!(
            save(
                directory.path(),
                &profile("Evil\u{1b}[2J", &["git"]),
                &catalog
            ),
            Err(ProfileError::Control)
        ));
        let unknown = check(&profile("x", &["git\u{1b}]52;c;x\u{7}"]), &catalog)
            .unwrap_err()
            .to_string();
        assert!(!unknown.chars().any(char::is_control), "{unknown:?}");
        let written = directory.path().join("evil.toml");
        std::fs::write(&written, "name = \"Evil\\u001b[2J\"\nentries = [\"git\"]\n").unwrap();
        assert!(matches!(
            read(&written, &catalog),
            Err(ProfileError::Control)
        ));
        std::fs::remove_file(&written).unwrap();
        std::fs::write(directory.path().join("broken.toml"), "name = 3\n").unwrap();
        let (found, problems) = load_all(directory.path(), &catalog);
        assert_eq!(found.len(), 1);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("broken.toml"));
    }

    #[test]
    fn they_appear_in_the_profiles_area_and_plan_like_any_profile() {
        let mut catalog = Catalog::builtin(false).unwrap();
        let directory = tempfile::tempdir().unwrap();
        save(
            directory.path(),
            &profile("Dev Box", &["git", "ripgrep"]),
            &catalog,
        )
        .unwrap();
        assert!(add_to(&mut catalog, directory.path()).is_empty());
        // Adding again replaces the group rather than repeating it.
        add_to(&mut catalog, directory.path());
        let tab = catalog
            .tabs
            .iter()
            .find(|tab| tab.id == "profiles")
            .unwrap();
        let groups: Vec<&str> = tab
            .items
            .iter()
            .filter_map(|item| match item {
                Item::Group(group) => Some(group.id.as_str()),
                Item::Entry(_) => None,
            })
            .collect();
        assert_eq!(groups, [GROUP]);
        let entry = catalog.find("profile-dev-box").unwrap();
        assert_eq!(entry.name, "Dev Box");
        assert!(
            matches!(&entry.action, Action::Profile { entries } if entries == &["git", "ripgrep"])
        );
        let platform = helixforge_platform::LinuxInfo {
            id: "arch".into(),
            pretty_name: "Arch Linux".into(),
            id_like: vec![],
            manager: Some(helixforge_core::PackageManager::Arch),
        };
        let nothing = crate::Machine {
            holds: &|_| false,
            on_path: &|_| false,
        };
        let planned =
            crate::plan_with([entry], &platform, &|id| catalog.find(id), nothing).unwrap();
        let ids: Vec<&str> = planned.steps.iter().map(crate::Step::id).collect();
        assert_eq!(ids, ["git", "ripgrep"]);
    }

    #[test]
    fn a_queue_becomes_a_flat_profile() {
        let catalog = Catalog::builtin(false).unwrap();
        let queue: Vec<Entry> = ["developer-basics", "git", "meld"]
            .iter()
            .map(|id| catalog.find(id).unwrap().clone())
            .collect();
        let made = from_queue(" Mine ", &queue);
        assert_eq!(made.name, "Mine");
        assert_eq!(made.entries, ["git", "ripgrep", "fzf", "neovim", "meld"]);
    }
}
