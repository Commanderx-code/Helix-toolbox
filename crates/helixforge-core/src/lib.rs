//! Shared data types: the catalog tree of tabs, groups and entries, and the
//! actions an entry performs. Nothing here touches the system.

pub mod search;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The package manager families that package entries can name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PackageManager {
    Arch,
    Debian,
    Fedora,
    Suse,
    Alpine,
    Void,
    Solus,
}

impl PackageManager {
    pub const fn executable(self) -> &'static str {
        match self {
            Self::Arch => "pacman",
            Self::Debian => "apt-get",
            Self::Fedora => "dnf",
            Self::Suse => "zypper",
            Self::Alpine => "apk",
            Self::Void => "xbps-install",
            Self::Solus => "eopkg",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Arch => "pacman (Arch family)",
            Self::Debian => "APT (Debian family)",
            Self::Fedora => "DNF (Fedora family)",
            Self::Suse => "zypper (openSUSE family)",
            Self::Alpine => "apk (Alpine family)",
            Self::Void => "xbps (Void family)",
            Self::Solus => "eopkg (Solus family)",
        }
    }

    /// The command that installs packages, without the package names.
    pub fn install_command(self) -> Vec<&'static str> {
        match self {
            Self::Arch => vec!["sudo", "pacman", "-S", "--needed"],
            Self::Debian => vec!["sudo", "apt-get", "install"],
            Self::Fedora => vec!["sudo", "dnf", "install"],
            Self::Suse => vec!["sudo", "zypper", "install"],
            Self::Alpine => vec!["sudo", "apk", "add"],
            Self::Void => vec!["sudo", "xbps-install", "-S"],
            Self::Solus => vec!["sudo", "eopkg", "install"],
        }
    }

    /// The command that removes packages, without the package names.
    pub fn remove_command(self) -> Vec<&'static str> {
        match self {
            Self::Arch => vec!["sudo", "pacman", "-R"],
            Self::Debian => vec!["sudo", "apt-get", "remove"],
            Self::Fedora => vec!["sudo", "dnf", "remove"],
            Self::Suse => vec!["sudo", "zypper", "remove"],
            Self::Alpine => vec!["sudo", "apk", "del"],
            Self::Void => vec!["sudo", "xbps-remove"],
            Self::Solus => vec!["sudo", "eopkg", "remove"],
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
    pub suse: Option<String>,
    pub alpine: Option<String>,
    pub void: Option<String>,
    pub solus: Option<String>,
}

impl PackageNames {
    pub fn for_manager(&self, manager: PackageManager) -> Option<&str> {
        match manager {
            PackageManager::Arch => self.arch.as_deref(),
            PackageManager::Debian => self.debian.as_deref(),
            PackageManager::Fedora => self.fedora.as_deref(),
            PackageManager::Suse => self.suse.as_deref(),
            PackageManager::Alpine => self.alpine.as_deref(),
            PackageManager::Void => self.void.as_deref(),
            PackageManager::Solus => self.solus.as_deref(),
        }
    }

    /// Every family with its package name, for listings and validation.
    pub fn all(&self) -> [(&'static str, Option<&str>); 7] {
        [
            ("arch", self.arch.as_deref()),
            ("debian", self.debian.as_deref()),
            ("fedora", self.fedora.as_deref()),
            ("suse", self.suse.as_deref()),
            ("alpine", self.alpine.as_deref()),
            ("void", self.void.as_deref()),
            ("solus", self.solus.as_deref()),
        ]
    }

    pub fn is_empty(&self) -> bool {
        self.all().iter().all(|(_, name)| name.is_none())
    }
}

/// What an entry does when run: the operation model shared by every
/// interface. The engine turns these into explicit plans.
// Catalog records, built once when the catalog loads; their size does not matter.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Action {
    /// Install one package: the family's native package, else on Arch the
    /// AUR package through paru or yay, else the Flatpak from Flathub.
    Package {
        /// A command the package provides. Whether the entry is installed is
        /// asked of the package manager and Flatpak first; the command only
        /// recognizes an install made some other way (Nix, a download) when
        /// none of its packages, and none it conflicts with, is installed.
        binary: Option<String>,
        packages: PackageNames,
        aur: Option<String>,
        flatpak: Option<String>,
        /// Packages of the same family this one replaces, such as wine for
        /// wine-staging: when one is installed, the plan says so.
        conflicts: Vec<String>,
        /// Whether the entry offers removal as well.
        removable: bool,
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
    /// What a script or command entry installs, so its status can be shown;
    /// package entries carry this in their action.
    pub detect: Option<Detect>,
    /// What it needs from the machine, checked before a run.
    pub needs: Needs,
    /// What a script entry is expected to change, for the change preview.
    pub effects: Effects,
}

/// What an entry's script is expected to change, declared in its catalog
/// file, so the change preview can show it before it runs. A script may do
/// more than this; the preview says so.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Effects {
    pub files: Vec<FileEffect>,
    /// Packages the script installs besides any of its own identity.
    pub packages: Vec<String>,
    /// Services it enables, disables, starts or restarts.
    pub services: Vec<String>,
    /// When the change takes effect, such as "at the next reboot".
    pub takes_effect: Option<String>,
    /// What cannot be known before it runs.
    pub unknown: Vec<String>,
}

impl Effects {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
            && self.packages.is_empty()
            && self.services.is_empty()
            && self.takes_effect.is_none()
            && self.unknown.is_empty()
    }
}

/// A file a script changes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct FileEffect {
    /// Absolute, or under `~/`.
    pub path: String,
    /// What happens to it.
    pub change: String,
    /// The arguments to the installers' `configure.py` that print its new
    /// contents without writing anything, when there is such a renderer.
    pub render: Vec<String>,
    /// When it applies, such as "the machine boots with GRUB".
    pub when: Option<String>,
}

/// The `configure.py` actions that only print a file's new contents.
pub const RENDERERS: [&str; 5] = [
    "grub",
    "grub-cmdline",
    "sddm",
    "mkinitcpio-hooks",
    "limine-cmdline",
];

/// What an entry needs from the machine beyond its tools.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Needs {
    /// The graphics vendors it is for (nvidia, amd, intel); any one will do.
    pub gpu: Vec<String>,
    /// Arch repositories that must be enabled, such as multilib.
    pub repositories: Vec<String>,
    /// Free space it needs on the root file system beyond its packages, in MiB.
    pub space_mb: Option<u64>,
    /// Entries that run before it when both are queued.
    pub after: Vec<String>,
    /// The processor architectures it works on (x86_64, aarch64); any when
    /// empty. It is left out of the catalog on others, and planned anyway
    /// it is blocked.
    pub architectures: Vec<String>,
}

impl Needs {
    /// Whether it works on `architecture`.
    pub fn runs_on(&self, architecture: &str) -> bool {
        self.architectures.is_empty()
            || self.architectures.iter().any(|found| found == architecture)
    }
}

/// Text from outside HelixForge (journal lines, file and unit names, error
/// messages) made safe to show in a terminal: control characters, which could
/// start escape sequences, become `?`; tabs become spaces.
pub fn printable(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '\t' => ' ',
            character if character.is_control() => '?',
            character => character,
        })
        .collect()
}

/// The graphics vendors `Needs::gpu` may name.
pub const GPU_VENDORS: [&str; 3] = ["nvidia", "amd", "intel"];

/// The processor architectures `Needs::architectures` may name: those there
/// are builds for.
pub const ARCHITECTURES: [&str; 2] = ["x86_64", "aarch64"];

/// The packages and command that show a non-package entry's software.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Detect {
    pub binary: Option<String>,
    pub packages: PackageNames,
    pub aur: Option<String>,
    pub flatpak: Option<String>,
}

/// What an entry installs, from its package action or its `detect` table.
#[derive(Clone, Copy, Debug)]
pub struct Identity<'a> {
    pub binary: Option<&'a str>,
    pub packages: &'a PackageNames,
    pub aur: Option<&'a str>,
    pub flatpak: Option<&'a str>,
    pub conflicts: &'a [String],
}

impl Entry {
    /// What the entry installs, if it says.
    pub fn identity(&self) -> Option<Identity<'_>> {
        match (&self.action, &self.detect) {
            (
                Action::Package {
                    binary,
                    packages,
                    aur,
                    flatpak,
                    conflicts,
                    ..
                },
                _,
            ) => Some(Identity {
                binary: binary.as_deref(),
                packages,
                aur: aur.as_deref(),
                flatpak: flatpak.as_deref(),
                conflicts,
            }),
            (_, Some(detect)) => Some(Identity {
                binary: detect.binary.as_deref(),
                packages: &detect.packages,
                aur: detect.aur.as_deref(),
                flatpak: detect.flatpak.as_deref(),
                conflicts: &[],
            }),
            (_, None) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Group {
    pub id: String,
    pub name: String,
    pub description: String,
    pub multi_select: bool,
    pub items: Vec<Item>,
}

// Catalog records, built once when the catalog loads; their size does not matter.
#[allow(clippy::large_enum_variant)]
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

/// An executable name to look for on the PATH, including Flatpak export names
/// such as `org.gnome.meld`.
pub fn valid_binary_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(['-', '.'])
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+._-".contains(&b))
}

/// A Flatpak application id: at least three dot-separated parts of letters,
/// digits, hyphens and underscores.
pub fn valid_flatpak_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('.').collect();
    parts.len() >= 3
        && id.len() <= 255
        && parts.iter().all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && part
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
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

    #[test]
    fn printable_text_carries_no_control_characters() {
        assert_eq!(printable("plain text"), "plain text");
        assert_eq!(
            printable("\u{1b}[31mred\u{1b}[0m\tend\r\u{7}"),
            "?[31mred?[0m end??"
        );
        assert_eq!(printable("ünïcode ✓"), "ünïcode ✓");
    }

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
        assert!(valid_binary_name("io.github.zen_browser.zen"));
        assert!(!valid_binary_name("-rf"));
        assert!(valid_flatpak_id("io.github.zen_browser.zen"));
        assert!(valid_flatpak_id("org.gnome.meld"));
        for bad in [
            "meld",
            "org.gnome",
            "org..meld",
            "org.gnome.-x",
            "org.gnome.a b",
        ] {
            assert!(!valid_flatpak_id(bad), "{bad}");
        }
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
            detect: None,
            needs: Default::default(),
            effects: Default::default(),
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
