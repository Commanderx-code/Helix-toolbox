//! The look of the interface, remembered between runs in
//! `$XDG_CONFIG_HOME/helixforge/settings.toml`.

use super::theme::Theme;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Look {
    pub theme: Theme,
    /// Nerd Font icons; off draws plain text instead.
    pub icons: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: Theme::Default,
            icons: true,
        }
    }
}

/// `$XDG_CONFIG_HOME/helixforge/settings.toml`, or under `~/.config`.
pub fn path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("helixforge/settings.toml"))
}

/// The saved look, or the default when there is none or it cannot be read.
pub fn load(path: &Path) -> Look {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, look: &Look) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(look).map_err(std::io::Error::other)?;
    // Write beside it and rename, so a crash never leaves half a file.
    let staging = path.with_extension("toml.new");
    fs::write(
        &staging,
        format!("# HelixForge interface settings; Settings in the menu changes them.\n{text}"),
    )?;
    fs::rename(&staging, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_look_round_trips_and_falls_back_to_the_default() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("helixforge/settings.toml");
        assert_eq!(load(&path), Look::default());
        let look = Look {
            theme: Theme::Solid,
            icons: false,
        };
        save(&path, &look).unwrap();
        assert_eq!(load(&path), look);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("theme = \"solid\"")
        );
        fs::write(&path, "theme = 3\n").unwrap();
        assert_eq!(load(&path), Look::default());
    }
}
