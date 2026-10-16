//! The look of the interface, remembered between runs in
//! `$XDG_CONFIG_HOME/helixforge/settings.toml`.

use super::theme::Theme;
use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Look {
    pub theme: Theme,
    /// Nerd Font icons; off draws plain text instead.
    pub icons: bool,
    /// After a failed step, continue with the steps that do not need it.
    pub keep_going: bool,
    /// Take a snapshot before system changes, where the machine can.
    pub snapshots: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: Theme::Solid,
            icons: true,
            keep_going: false,
            snapshots: true,
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
/// Each setting is read on its own: one this version does not understand,
/// such as a theme from a later version, falls back to its default without
/// losing the others, and settings it does not know are ignored.
pub fn load(path: &Path) -> Look {
    let Some(table) = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
    else {
        return Look::default();
    };
    let mut look = Look::default();
    let get = |name: &str| table.get(name).cloned();
    if let Some(theme) = get("theme").and_then(|value| value.try_into().ok()) {
        look.theme = theme;
    }
    for (name, field) in [
        ("icons", &mut look.icons),
        ("keep_going", &mut look.keep_going),
        ("snapshots", &mut look.snapshots),
    ] {
        if let Some(value) = get(name).and_then(|value| value.as_bool()) {
            *field = value;
        }
    }
    look
}

/// Pinned entries, kept beside the settings as `favourites.toml`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Favourites {
    entries: Vec<String>,
}

/// Where the favourites are, beside `settings`.
pub fn favourites_path(settings: &Path) -> PathBuf {
    settings.with_file_name("favourites.toml")
}

/// The pinned entry ids, in the order pinned; none when there is no file.
pub fn load_favourites(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<Favourites>(&text).ok())
        .map(|favourites| favourites.entries)
        .unwrap_or_default()
}

pub fn save_favourites(path: &Path, entries: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(&Favourites {
        entries: entries.to_vec(),
    })
    .map_err(std::io::Error::other)?;
    let staging = path.with_extension("toml.new");
    fs::write(
        &staging,
        format!("# Entries pinned in HelixForge with *.\n{text}"),
    )?;
    fs::rename(&staging, path)
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
        assert_eq!(Look::default().theme, Theme::Solid);
        let look = Look {
            theme: Theme::Transparent,
            icons: false,
            keep_going: true,
            snapshots: false,
        };
        save(&path, &look).unwrap();
        assert_eq!(load(&path), look);
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("theme = \"transparent\"")
        );
        // The transparent theme's old name still loads.
        fs::write(&path, "theme = \"default\"\n").unwrap();
        assert_eq!(load(&path).theme, Theme::Transparent);
        fs::write(&path, "theme = 3\n").unwrap();
        assert_eq!(load(&path), Look::default());
        // A later version's theme and settings cost nothing else.
        fs::write(
            &path,
            "theme = \"aurora\"\nicons = false\nanimations = true\n[future]\nx = 1\n",
        )
        .unwrap();
        assert_eq!(
            load(&path),
            Look {
                icons: false,
                ..Look::default()
            }
        );
    }
}
