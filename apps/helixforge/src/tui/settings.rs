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
    /// The setup wizard has been offered: false only on the very first run.
    pub set_up: bool,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            theme: Theme::Solid,
            icons: true,
            keep_going: false,
            snapshots: true,
            set_up: false,
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
    // Settings saved before the wizard existed belong to a machine already set up.
    let mut look = Look {
        set_up: true,
        ..Look::default()
    };
    let get = |name: &str| table.get(name).cloned();
    if let Some(theme) = get("theme").and_then(|value| value.try_into().ok()) {
        look.theme = theme;
    }
    for (name, field) in [
        ("icons", &mut look.icons),
        ("keep_going", &mut look.keep_going),
        ("snapshots", &mut look.snapshots),
        ("set_up", &mut look.set_up),
    ] {
        if let Some(value) = get(name).and_then(|value| value.as_bool()) {
            *field = value;
        }
    }
    look
}

/// Pinned entries, kept beside the settings as `favorites.toml`.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
struct Favorites {
    entries: Vec<String>,
}

/// Where releases before 1.5.9 kept the favorites, beside the settings.
const OLD_FAVORITES: &str = "favourites.toml";

/// Where the favorites are, beside `settings`.
pub fn favorites_path(settings: &Path) -> PathBuf {
    settings.with_file_name("favorites.toml")
}

/// The pinned entry ids, in the order pinned; none when there is no file.
/// The file an earlier release wrote is read instead when it is newer, as
/// it is after going back to that release and pinning something.
pub fn load_favorites(path: &Path) -> Vec<String> {
    let old = path.with_file_name(OLD_FAVORITES);
    let path = match (modified(&old), modified(path)) {
        (Some(old_time), Some(time)) if old_time > time => &old,
        (Some(_), None) => &old,
        _ => path,
    };
    fs::read_to_string(path)
        .ok()
        .and_then(|text| toml::from_str::<Favorites>(&text).ok())
        .map(|favorites| favorites.entries)
        .unwrap_or_default()
}

/// When a regular file (not a link) was last changed.
fn modified(path: &Path) -> Option<std::time::SystemTime> {
    fs::symlink_metadata(path)
        .ok()
        .filter(|metadata| metadata.is_file())
        .and_then(|metadata| metadata.modified().ok())
}

pub fn save_favorites(path: &Path, entries: &[String]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(&Favorites {
        entries: entries.to_vec(),
    })
    .map_err(std::io::Error::other)?;
    let staging = path.with_extension("toml.new");
    fs::write(
        &staging,
        format!("# Entries pinned in HelixForge with *.\n{text}"),
    )?;
    fs::rename(&staging, path)?;
    // Where an earlier release kept them, leave a link to the new file, so
    // going back to that release still finds the pins.
    let old = path.with_file_name(OLD_FAVORITES);
    if fs::symlink_metadata(&old).is_ok() {
        let link = path.with_extension("toml.link");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(path.file_name().unwrap_or_default(), &link)?;
        fs::rename(&link, &old)?;
    }
    Ok(())
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
    fn favorites_move_from_the_old_file_and_leave_a_link() {
        let directory = tempfile::tempdir().unwrap();
        let path = favorites_path(&directory.path().join("settings.toml"));
        let old = directory.path().join(OLD_FAVORITES);
        assert!(load_favorites(&path).is_empty());

        // Pins an earlier release saved are read, then saved under the new
        // name, with the old one a link to it.
        fs::write(&old, "entries = [\"ripgrep\"]\n").unwrap();
        assert_eq!(load_favorites(&path), ["ripgrep"]);
        save_favorites(&path, &["ripgrep".into(), "fzf".into()]).unwrap();
        assert!(fs::symlink_metadata(&old).unwrap().is_symlink());
        assert_eq!(
            fs::read_to_string(&old).unwrap(),
            fs::read_to_string(&path).unwrap()
        );
        assert_eq!(load_favorites(&path), ["ripgrep", "fzf"]);

        // An earlier release writing the old file again wins while newer.
        fs::remove_file(&old).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&old, "entries = [\"bat\"]\n").unwrap();
        assert_eq!(load_favorites(&path), ["bat"]);

        // A new install never makes the old file.
        let fresh = tempfile::tempdir().unwrap();
        let path = favorites_path(&fresh.path().join("settings.toml"));
        save_favorites(&path, &["fzf".into()]).unwrap();
        assert!(!fresh.path().join(OLD_FAVORITES).exists());
    }

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
            set_up: false,
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
        // Settings from before the setup wizard mean the machine was set up.
        assert_eq!(
            load(&path),
            Look {
                set_up: true,
                ..Look::default()
            }
        );
        assert!(
            !Look::default().set_up,
            "with no settings at all, it is the first run"
        );
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
                set_up: true,
                ..Look::default()
            }
        );
    }
}
