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

/// What an entry does when run: the operation model shared by every
/// interface. The engine turns these into explicit plans.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Install one native package with the family's package manager.
    Package {
        /// The executable whose presence means the package is installed.
        binary: String,
        packages: PackageNames,
    },
    /// Run a script from its own directory. Interactive scripts need a
    /// terminal for their prompts; the others could run unattended.
    Script { file: PathBuf, interactive: bool },
    /// Run an inline shell command.
    Command { line: String },
    /// A Nix operation.
    Nix { action: NixAction },
    /// An operation implemented in Rust by the engine itself.
    Native { action: NativeAction },
    /// A reviewable list of other entries, run in order.
    Profile { entries: Vec<String> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NixAction {
    /// `nix profile install <installable>`.
    ProfileInstall { installable: String },
    /// `home-manager switch --flake <flake>`.
    HomeManagerSwitch { flake: String },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeAction {
    /// Print what HelixForge knows about the machine.
    SystemReport,
}

impl NativeAction {
    pub const fn id(self) -> &'static str {
        match self {
            NativeAction::SystemReport => "system-report",
        }
    }

    pub fn parse(id: &str) -> Option<Self> {
        match id {
            "system-report" => Some(NativeAction::SystemReport),
            _ => None,
        }
    }
}

/// How much an entry can affect the system, derived from its flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Risk {
    /// Reads or user files only.
    #[default]
    Low,
    /// Installs packages or changes system files with privileges.
    Medium,
    /// Disk, kernel or whole-system changes.
    High,
}

impl Risk {
    /// `D`, `K` and `SI` are high; anything privileged or installing is
    /// medium; the rest is low.
    pub fn from_flags(flags: &str) -> Self {
        let mut risk = Risk::Low;
        for flag in flags.split_whitespace() {
            let level = match flag {
                "D" | "K" | "SI" | "PD" | "PK" => Risk::High,
                "I" | "SS" | "RP" | "MP" => Risk::Medium,
                other if other.starts_with('P') => Risk::Medium,
                _ => Risk::Low,
            };
            risk = risk.max(level);
        }
        risk
    }

    pub const fn label(self) -> &'static str {
        match self {
            Risk::Low => "low",
            Risk::Medium => "medium",
            Risk::High => "high",
        }
    }
}

/// Whether an entry needs elevated privileges, from its flags and kind.
pub fn privileged(flags: &str, action: &Action) -> bool {
    matches!(action, Action::Package { .. })
        || flags
            .split_whitespace()
            .any(|flag| matches!(flag, "I" | "SS" | "D" | "K" | "SI") || flag.starts_with('P'))
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
    /// User paths the entry replaces, with `~` for the home directory, so the
    /// engine can refuse to run over files another tool manages.
    pub writes: Vec<String>,
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

/// One directory or file name in a script id: lowercase, digits, hyphens and
/// underscores, as the scripts carried over from Linutil are named.
pub fn valid_script_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment.len() <= 64
        && !segment.starts_with(['-', '_'])
        && segment
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
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
        assert!(valid_script_segment("utility_functions"));
        for bad in ["", "..", "_x", "A", "a.b"] {
            assert!(!valid_script_segment(bad));
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
            writes: Vec::new(),
            action: Action::Command {
                line: "true".into(),
            },
        };
        assert!(!privileged("FM", &entry.action));
        assert!(privileged("I FM", &entry.action));
        assert_eq!(Risk::from_flags("I FM"), Risk::Medium);
        assert_eq!(Risk::from_flags("PFM"), Risk::Medium);
        assert_eq!(Risk::from_flags("D"), Risk::High);
        assert_eq!(Risk::from_flags(""), Risk::Low);
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
