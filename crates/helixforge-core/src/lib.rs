//! Shared data types: the catalog tree of tabs, groups and entries, and the
//! actions an entry performs. Nothing here touches the system.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The package manager families that package entries can name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Arch,
    Debian,
    Fedora,
}

impl PackageManager {
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Arch => "pacman",
            Self::Debian => "apt-get",
            Self::Fedora => "dnf",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Arch => "pacman (Arch family)",
            Self::Debian => "APT (Debian family)",
            Self::Fedora => "DNF (Fedora family)",
        }
    }
}

/// Package names per family; a missing family means the entry is not
/// available there.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct PackageNames {
    pub arch: Option<String>,
    pub debian: Option<String>,
    pub fedora: Option<String>,
}

impl PackageNames {
    pub fn for_manager(&self, manager: PackageManager) -> Option<&str> {
        match manager {
            PackageManager::Arch => self.arch.as_deref(),
            PackageManager::Debian => self.debian.as_deref(),
            PackageManager::Fedora => self.fedora.as_deref(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.arch.is_none() && self.debian.is_none() && self.fedora.is_none()
    }
}

/// What an entry does when run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Install one native package with the family's package manager.
    Package {
        /// The executable whose presence means the package is installed.
        binary: String,
        packages: PackageNames,
    },
    /// Run a script from its own directory in a terminal.
    Script { file: PathBuf },
    /// Run an inline shell command.
    Command { line: String },
}

/// A condition that hides an entry on machines it does not apply to.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Precondition {
    /// Whether the values must match (true) or must not match (false).
    pub matches: bool,
    pub check: Check,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    /// An environment variable whose value is one of the values.
    Environment(String),
    /// A file containing every value.
    ContainingFile(PathBuf),
    /// Every value is an existing file.
    FileExists,
    /// Every value is a command on the PATH.
    CommandExists,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Short codes for what the entry does, shown in the flags column.
    pub flags: String,
    /// Whether the entry may be queued with others.
    pub multi_select: bool,
    pub action: Action,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: String,
    pub multi_select: bool,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Item {
    Group(Group),
    Entry(Entry),
}

impl Item {
    pub fn name(&self) -> &str {
        match self {
            Item::Group(group) => &group.name,
            Item::Entry(entry) => &entry.name,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tab {
    pub id: String,
    pub name: String,
    pub items: Vec<Item>,
}

impl Tab {
    /// The items at a path of group indexes, starting at the tab's own list.
    pub fn items_at(&self, path: &[usize]) -> Option<&[Item]> {
        let mut items = self.items.as_slice();
        for &index in path {
            match items.get(index)? {
                Item::Group(group) => items = &group.items,
                Item::Entry(_) => return None,
            }
        }
        Some(items)
    }

    /// Every entry in the tab, depth first in catalog order.
    pub fn entries(&self) -> Vec<&Entry> {
        fn walk<'a>(items: &'a [Item], into: &mut Vec<&'a Entry>) {
            for item in items {
                match item {
                    Item::Group(group) => walk(&group.items, into),
                    Item::Entry(entry) => into.push(entry),
                }
            }
        }
        let mut result = Vec::new();
        walk(&self.items, &mut result);
        result
    }
}

/// Reject values that could become flags or shell syntax in a command line.
pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+._@-".contains(&b))
}

/// Entry, group and tab identifiers: lowercase, digits and hyphens.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !id.starts_with('-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_names_disallow_flags_and_metacharacters() {
        for bad in ["", "--help", "git;reboot", "git foo", "$(cmd)", "a/b"] {
            assert!(!valid_package_name(bad), "unexpectedly accepted {bad}");
        }
        for good in ["git", "vim-runtime", "python3.12", "libstdc++"] {
            assert!(valid_package_name(good), "unexpectedly rejected {good}");
        }
    }

    #[test]
    fn identifiers_are_plain() {
        assert!(valid_id("commander-ghostty"));
        for bad in ["", "-x", "Caps", "a b", "a/b"] {
            assert!(!valid_id(bad));
        }
    }

    #[test]
    fn paths_walk_groups_only() {
        let entry = Entry {
            id: "e".into(),
            name: "E".into(),
            description: String::new(),
            flags: String::new(),
            multi_select: true,
            action: Action::Command {
                line: "true".into(),
            },
        };
        let tab = Tab {
            id: "t".into(),
            name: "T".into(),
            items: vec![
                Item::Group(Group {
                    id: "g".into(),
                    name: "G".into(),
                    description: String::new(),
                    multi_select: true,
                    items: vec![Item::Entry(entry.clone())],
                }),
                Item::Entry(entry),
            ],
        };
        assert_eq!(tab.items_at(&[]).unwrap().len(), 2);
        assert_eq!(tab.items_at(&[0]).unwrap().len(), 1);
        assert!(tab.items_at(&[1]).is_none());
        assert!(tab.items_at(&[5]).is_none());
        assert_eq!(tab.entries().len(), 2);
    }
}
