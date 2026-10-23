//! Shared data types: the catalog tree of tabs, groups and entries, and the
//! actions an entry performs. Nothing here touches the system.

pub mod search;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// The package manager families that package entries can name.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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

    /// The family's name to set.
    pub fn for_manager_mut(&mut self, manager: PackageManager) -> &mut Option<String> {
        match manager {
            PackageManager::Arch => &mut self.arch,
            PackageManager::Debian => &mut self.debian,
            PackageManager::Fedora => &mut self.fedora,
            PackageManager::Suse => &mut self.suse,
            PackageManager::Alpine => &mut self.alpine,
            PackageManager::Void => &mut self.void,
            PackageManager::Solus => &mut self.solus,
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
        /// The tool's own GitHub release, tried after every package source,
        /// or alone when the entry names no package.
        release: Option<Release>,
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
    /// Print what Helix Toolbox knows about the machine.
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

/// A tool installed from its project's GitHub release: one pinned file per
/// architecture, checked against its SHA-256 before anything is unpacked,
/// and put in `~/.local/bin` without root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Release {
    /// `owner/name` on GitHub.
    pub repository: String,
    /// The release's tag, as its download URLs carry it (`v0.55.1`).
    pub tag: String,
    /// The version it installs, compared with what is installed.
    pub version: String,
    /// The command it installs, as `~/.local/bin/<binary>`.
    pub binary: String,
    /// The file to download for each architecture there is one for.
    pub assets: Vec<ReleaseAsset>,
}

/// One architecture's file of a release.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReleaseAsset {
    /// `x86_64` or `aarch64`.
    pub architecture: String,
    /// The file's name in the release.
    pub file: String,
    /// Its SHA-256, 64 lowercase hexadecimal digits.
    pub sha256: String,
    /// The program inside the archive; None for a file that is the program.
    pub path: Option<String>,
}

/// How a release file is packed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Archive {
    TarGz,
    TarXz,
    Zip,
    /// The file is the program itself.
    Bare,
}

impl Archive {
    /// From the file's name.
    pub fn of(file: &str) -> Self {
        if file.ends_with(".tar.gz") || file.ends_with(".tgz") {
            Archive::TarGz
        } else if file.ends_with(".tar.xz") {
            Archive::TarXz
        } else if file.ends_with(".zip") {
            Archive::Zip
        } else {
            Archive::Bare
        }
    }

    /// The program that unpacks it, if any.
    pub const fn tool(self) -> Option<&'static str> {
        match self {
            Archive::TarGz | Archive::TarXz => Some("tar"),
            Archive::Zip => Some("unzip"),
            Archive::Bare => None,
        }
    }
}

impl Release {
    /// The file for this architecture; None where the project builds none.
    pub fn asset_for(&self, architecture: &str) -> Option<&ReleaseAsset> {
        self.assets
            .iter()
            .find(|asset| asset.architecture == architecture)
    }

    /// Where a file of the release is downloaded from, below `base`
    /// (`https://github.com`).
    pub fn url_from(&self, base: &str, asset: &ReleaseAsset) -> String {
        format!(
            "{base}/{}/releases/download/{}/{}",
            self.repository, self.tag, asset.file
        )
    }

    /// Whether every value is safe to put on a command line and in a URL,
    /// and the files and paths it names stay where they belong.
    pub fn is_valid(&self) -> bool {
        valid_repository(&self.repository)
            && valid_release_name(&self.tag)
            && valid_release_name(&self.version)
            && valid_binary_name(&self.binary)
            && !self.assets.is_empty()
            && self.assets.iter().enumerate().all(|(index, asset)| {
                ARCHITECTURES.contains(&asset.architecture.as_str())
                    && !self.assets[..index]
                        .iter()
                        .any(|other| other.architecture == asset.architecture)
                    && valid_release_name(&asset.file)
                    && valid_sha256(&asset.sha256)
                    && match (Archive::of(&asset.file), &asset.path) {
                        (Archive::Bare, None) => true,
                        (Archive::Bare, Some(_)) | (_, None) => false,
                        (_, Some(path)) => valid_member_path(path),
                    }
            })
    }
}

/// A GitHub repository, `owner/name`.
pub fn valid_repository(repository: &str) -> bool {
    let Some((owner, name)) = repository.split_once('/') else {
        return false;
    };
    let part = |text: &str| {
        !text.is_empty()
            && text.len() <= 100
            && !text.starts_with(['-', '.'])
            && text
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
    };
    part(owner) && part(name)
}

/// A tag, version or file name of a release: no slashes, no `..`, nothing a
/// shell or URL would read specially.
pub fn valid_release_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['-', '.'])
        && !name.contains("..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+._-".contains(&b))
}

/// A SHA-256 as 64 lowercase hexadecimal digits.
pub fn valid_sha256(sum: &str) -> bool {
    sum.len() == 64
        && sum
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// A file inside an archive: relative, with no empty, `.` or `..` parts.
pub fn valid_member_path(path: &str) -> bool {
    path.len() <= 255 && path.split('/').all(valid_release_name)
}

/// Order two versions by their numbers, part by part (`0.9.10` after
/// `0.9.2`); text after a part's number breaks ties.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |text: &str| -> Vec<(u64, String)> {
        text.trim_start_matches(['v', 'V'])
            .split(['.', '-', '+', '_'])
            .map(|part| {
                let digits = part.bytes().take_while(u8::is_ascii_digit).count();
                (
                    part[..digits].parse().unwrap_or(0),
                    part[digits..].to_string(),
                )
            })
            .collect()
    };
    let (a, b) = (parts(a), parts(b));
    for index in 0..a.len().max(b.len()) {
        let left = a.get(index).cloned().unwrap_or_default();
        let right = b.get(index).cloned().unwrap_or_default();
        let order = left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1));
        if order.is_ne() {
            return order;
        }
    }
    std::cmp::Ordering::Equal
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
    // A release alone installs into the user's own folder.
    let packaged = match action {
        Action::Package {
            packages,
            aur,
            flatpak,
            ..
        } => !packages.is_empty() || aur.is_some() || flatpak.is_some(),
        _ => false,
    };
    packaged
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
    /// What holds once it has run, checked afterward (`[[verify]]`).
    pub checks: Vec<Verify>,
}

impl Effects {
    /// Nothing for the change preview to show; checks are not shown there.
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

/// Something that holds once an entry has run, checked afterward from the
/// machine itself, read-only and as the user.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Verify {
    pub claim: Claim,
    /// Checked only where this path exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when_exists: Option<String>,
    /// It holds only once the machine has started again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub after_reboot: bool,
}

/// What a check claims. Paths are absolute, under `~/`, or under `{grub}/`
/// (GRUB's folder in /boot).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Claim {
    /// A file or folder is there, or is gone.
    Path {
        path: String,
        /// "file" or "directory"; either when not given.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        absent: bool,
    },
    /// A file has a word (separated by spaces) or a whole line, or lacks it:
    /// `/proc/cmdline`, a value in `/sys`.
    Text {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        word: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line: Option<String>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        absent: bool,
    },
    /// A setting in `key=value` files, INI or shell style; with several
    /// files, the last one that sets it wins, as programs read them.
    Setting {
        files: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
        key: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        equals: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contains: Option<String>,
        /// A path made from the value, `{value}` standing for it, that
        /// must exist: the theme a setting selects is installed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        points_to: Option<String>,
        /// Not set at all, or set to nothing.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        unset: bool,
    },
    /// A systemd unit is enabled, active, or neither; the user's own with
    /// `user`.
    Service {
        unit: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        enabled: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        active: Option<bool>,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        user: bool,
    },
}

/// A systemd unit name: letters, digits and `-_.@:\`, ending in a unit type.
pub fn valid_unit_name(name: &str) -> bool {
    const TYPES: [&str; 8] = [
        ".service",
        ".timer",
        ".socket",
        ".target",
        ".mount",
        ".path",
        ".swap",
        ".automount",
    ];
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with(['-', '.'])
        && TYPES.iter().any(|kind| name.ends_with(kind))
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.@:\\".contains(&b))
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

/// Text from outside Helix Toolbox (journal lines, file and unit names, error
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
    /// The tool's GitHub release, for a package entry that has one.
    pub release: Option<&'a Release>,
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
                    release,
                    ..
                },
                _,
            ) => Some(Identity {
                binary: binary.as_deref(),
                packages,
                aur: aur.as_deref(),
                flatpak: flatpak.as_deref(),
                conflicts,
                release: release.as_ref(),
            }),
            (_, Some(detect)) => Some(Identity {
                binary: detect.binary.as_deref(),
                packages: &detect.packages,
                aur: detect.aur.as_deref(),
                flatpak: detect.flatpak.as_deref(),
                conflicts: &[],
                release: None,
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

    fn sample_release() -> Release {
        Release {
            repository: "jesseduffield/lazygit".into(),
            tag: "v0.55.1".into(),
            version: "0.55.1".into(),
            binary: "lazygit".into(),
            assets: vec![ReleaseAsset {
                architecture: "x86_64".into(),
                file: "lazygit_0.55.1_linux_x86_64.tar.gz".into(),
                sha256: "a".repeat(64),
                path: Some("lazygit".into()),
            }],
        }
    }

    #[test]
    fn releases_take_only_values_that_stay_in_place() {
        let good = sample_release();
        assert!(good.is_valid());
        assert_eq!(
            good.url_from("https://github.com", &good.assets[0]),
            "https://github.com/jesseduffield/lazygit/releases/download/v0.55.1/lazygit_0.55.1_linux_x86_64.tar.gz"
        );
        assert_eq!(good.asset_for("x86_64"), Some(&good.assets[0]));
        assert!(good.asset_for("aarch64").is_none());
        let broken: [fn(&mut Release); 12] = [
            |release| release.repository = "lazygit".into(),
            |release| release.repository = "a/b/c".into(),
            |release| release.repository = "../lazygit".into(),
            |release| release.tag = "v1/../../x".into(),
            |release| release.tag = "-v1".into(),
            |release| release.binary = "../bin".into(),
            |release| release.assets[0].sha256 = "A".repeat(64),
            |release| release.assets[0].sha256 = "a".repeat(63),
            |release| release.assets[0].path = Some("../lazygit".into()),
            |release| release.assets[0].path = Some("/bin/lazygit".into()),
            // An archive needs its program's path; a bare file has none.
            |release| release.assets[0].path = None,
            |release| release.assets[0].architecture = "riscv64".into(),
        ];
        for (index, breaking) in broken.iter().enumerate() {
            let mut release = sample_release();
            breaking(&mut release);
            assert!(!release.is_valid(), "case {index} was accepted");
        }
        let mut bare = sample_release();
        bare.assets[0].file = "lazygit-linux-amd64".into();
        bare.assets[0].path = None;
        assert!(bare.is_valid());
        assert_eq!(Archive::of(&bare.assets[0].file), Archive::Bare);
        bare.assets[0].path = Some("lazygit".into());
        assert!(!bare.is_valid());
        let mut twice = sample_release();
        twice.assets.push(twice.assets[0].clone());
        assert!(!twice.is_valid());
        assert!(valid_member_path("yazi-x86_64-unknown-linux-musl/yazi"));
        assert!(!valid_member_path("a//b"));
        assert!(!valid_member_path("./a"));
        assert_eq!(Archive::of("a.tar.xz").tool(), Some("tar"));
        assert_eq!(Archive::of("a.zip").tool(), Some("unzip"));
    }

    #[test]
    fn versions_order_by_their_numbers() {
        use std::cmp::Ordering::*;
        assert_eq!(compare_versions("0.9.10", "0.9.2"), Greater);
        assert_eq!(compare_versions("v1.2.0", "1.2.0"), Equal);
        assert_eq!(compare_versions("1.2", "1.2.1"), Less);
        assert_eq!(compare_versions("25.5.31", "26.1.0"), Less);
        assert_eq!(compare_versions("0.55.1", "0.55.1"), Equal);
    }

    #[test]
    fn a_release_alone_needs_no_root() {
        let alone = Action::Package {
            binary: Some("lazygit".into()),
            packages: PackageNames::default(),
            aur: None,
            flatpak: None,
            conflicts: Vec::new(),
            removable: true,
            release: Some(sample_release()),
        };
        assert!(!privileged("", &alone));
        let mut packaged = alone.clone();
        if let Action::Package { packages, .. } = &mut packaged {
            packages.arch = Some("lazygit".into());
        }
        assert!(privileged("", &packaged));
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
