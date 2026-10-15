//! Shared platform-independent data types.

use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Detect {
    pub binary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Install {
    pub kind: String,
    pub packages: PackageNames,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub detect: Detect,
    pub install: Install,
}

/// Reject unsafe values before they can become subprocess arguments.
/// This is not shell escaping: commands are always executed without a shell.
pub fn valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.len() <= 128
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"+._@-".contains(&b))
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
}
