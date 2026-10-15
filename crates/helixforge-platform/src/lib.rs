//! Read-only Linux identification and executable detection.

use helixforge_core::PackageManager;
use std::{env, fs, path::Path};
use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LinuxInfo {
    pub id: String,
    pub pretty_name: String,
    pub id_like: Vec<String>,
    pub manager: Option<PackageManager>,
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("cannot read /etc/os-release: {0}")]
    OsRelease(#[from] std::io::Error),
    #[error("missing ID field in os-release")]
    MissingId,
}

impl LinuxInfo {
    pub fn detect() -> Result<Self, PlatformError> {
        // Most Linux distributions expose this file, including Garuda.
        let contents = fs::read_to_string("/etc/os-release")?;
        Self::from_os_release(&contents)
    }

    pub fn from_os_release(text: &str) -> Result<Self, PlatformError> {
        let mut id = String::new();
        let mut pretty_name = String::new();
        let mut id_like = Vec::new();
        for line in text.lines() {
            if line.trim_start().starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let val = value.trim().trim_matches('"').trim_matches('\'');
                match key.trim() {
                    "ID" => id = val.to_ascii_lowercase(),
                    "PRETTY_NAME" => pretty_name = val.to_owned(),
                    "ID_LIKE" => {
                        id_like = val
                            .split_whitespace()
                            .map(str::to_ascii_lowercase)
                            .collect()
                    }
                    _ => {}
                }
            }
        }
        if id.is_empty() {
            return Err(PlatformError::MissingId);
        }
        if pretty_name.is_empty() {
            pretty_name = id.clone();
        }
        let family = std::iter::once(id.as_str()).chain(id_like.iter().map(String::as_str));
        let names: Vec<_> = family.collect();
        let any = |set: &[&str]| names.iter().any(|n| set.contains(n));
        let manager = if any(&["arch", "garuda", "manjaro", "endeavouros", "cachyos"]) {
            Some(PackageManager::Arch)
        } else if any(&["debian", "ubuntu", "linuxmint", "zorin", "pop"]) {
            Some(PackageManager::Debian)
        } else if any(&["fedora", "rhel", "centos"]) {
            Some(PackageManager::Fedora)
        } else if any(&[
            "suse",
            "opensuse",
            "opensuse-tumbleweed",
            "opensuse-leap",
            "sles",
        ]) {
            Some(PackageManager::Suse)
        } else if any(&["alpine"]) {
            Some(PackageManager::Alpine)
        } else if any(&["void"]) {
            Some(PackageManager::Void)
        } else if any(&["solus"]) {
            Some(PackageManager::Solus)
        } else {
            None
        };
        Ok(Self {
            id,
            pretty_name,
            id_like,
            manager,
        })
    }
}

/// Inspect PATH without spawning a shell. Used for read-only package detection.
pub fn binary_exists(binary: &str) -> bool {
    if binary.is_empty() || binary.contains('/') {
        return false;
    }
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&paths).any(|dir| is_executable(&dir.join(binary)))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn garuda_maps_to_arch() {
        let info =
            LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
                .unwrap();
        assert_eq!(info.manager, Some(PackageManager::Arch));
        assert_eq!(info.pretty_name, "Garuda Linux");
    }

    #[test]
    fn ubuntu_maps_to_debian() {
        let info = LinuxInfo::from_os_release("ID=ubuntu\nID_LIKE=debian\n").unwrap();
        assert_eq!(info.manager, Some(PackageManager::Debian));
    }

    #[test]
    fn the_other_families_are_recognised() {
        for (text, manager) in [
            (
                "ID=opensuse-tumbleweed\nID_LIKE=\"opensuse suse\"\n",
                PackageManager::Suse,
            ),
            ("ID=alpine\n", PackageManager::Alpine),
            ("ID=void\n", PackageManager::Void),
            ("ID=solus\n", PackageManager::Solus),
        ] {
            assert_eq!(
                LinuxInfo::from_os_release(text).unwrap().manager,
                Some(manager),
                "{text}"
            );
        }
    }

    #[test]
    fn unknown_os_is_read_only() {
        let info = LinuxInfo::from_os_release("ID=experimental\n").unwrap();
        assert_eq!(info.manager, None);
    }

    #[test]
    fn path_injection_is_rejected() {
        assert!(!binary_exists("/bin/sh"));
        assert!(!binary_exists("../foo"));
    }
}
