//! What the interfaces keep in `$XDG_CONFIG_HOME/helix-toolbox/`: the
//! choices about runs in `settings.toml`, which every interface reads, and
//! the pinned entries in `favorites.toml`. The terminal interface keeps its
//! look in the same `settings.toml`.

use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

/// `$XDG_CONFIG_HOME/helix-toolbox/settings.toml`, or under `~/.config`.
pub fn path() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("helix-toolbox/settings.toml"))
}

/// How runs go, from `settings.toml`; Settings in the terminal interface
/// changes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunChoices {
    /// After a failed step, continue with the steps that do not need it.
    pub keep_going: bool,
    /// Take a snapshot before system changes, where the machine can.
    pub snapshots: bool,
}

impl Default for RunChoices {
    fn default() -> Self {
        Self {
            keep_going: false,
            snapshots: true,
        }
    }
}

/// The saved choices, each falling back to its default when it is missing
/// or cannot be read.
pub fn run_choices(path: &Path) -> RunChoices {
    let mut choices = RunChoices::default();
    let Some(table) = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
    else {
        return choices;
    };
    for (name, field) in [
        ("keep_going", &mut choices.keep_going),
        ("snapshots", &mut choices.snapshots),
    ] {
        if let Some(value) = table.get(name).and_then(toml::Value::as_bool) {
            *field = value;
        }
    }
    choices
}

/// Save the run choices into `settings.toml`, leaving every other setting
/// in it (the terminal interface's look) as it is.
pub fn save_run_choices(path: &Path, choices: RunChoices) -> std::io::Result<()> {
    let mut table = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    table.insert(
        "keep_going".into(),
        toml::Value::Boolean(choices.keep_going),
    );
    table.insert("snapshots".into(), toml::Value::Boolean(choices.snapshots));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(&table).map_err(std::io::Error::other)?;
    let staging = path.with_extension("toml.new");
    fs::write(
        &staging,
        format!("# Helix Toolbox interface settings; Settings in the menu changes them.\n{text}"),
    )?;
    fs::rename(&staging, path)
}

/// Whether the setup wizard has been offered on this machine (`set_up`):
/// no settings file yet means not; a file without the key was saved before
/// the wizard existed, on a machine already set up.
pub fn set_up(path: &Path) -> bool {
    let Some(table) = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
    else {
        return false;
    };
    table
        .get("set_up")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true)
}

/// Remember that the wizard was offered, leaving the other settings as they
/// are.
pub fn mark_set_up(path: &Path) -> std::io::Result<()> {
    let mut table = fs::read_to_string(path)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .unwrap_or_default();
    table.insert("set_up".into(), toml::Value::Boolean(true));
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let text = toml::to_string(&table).map_err(std::io::Error::other)?;
    let staging = path.with_extension("toml.new");
    fs::write(
        &staging,
        format!("# Helix Toolbox interface settings; Settings in the menu changes them.\n{text}"),
    )?;
    fs::rename(&staging, path)
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
        format!("# Entries pinned in Helix Toolbox with *.\n{text}"),
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
    fn saving_run_choices_keeps_the_other_settings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("helix-toolbox/settings.toml");
        save_run_choices(
            &path,
            RunChoices {
                keep_going: true,
                snapshots: false,
            },
        )
        .unwrap();
        assert_eq!(
            run_choices(&path),
            RunChoices {
                keep_going: true,
                snapshots: false,
            }
        );
        fs::write(
            &path,
            "theme = \"neon\"\nicons = false\nkeep_going = true\n",
        )
        .unwrap();
        save_run_choices(&path, RunChoices::default()).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("theme = \"neon\""), "{text}");
        assert!(text.contains("icons = false"));
        assert_eq!(run_choices(&path), RunChoices::default());
    }

    #[test]
    fn the_wizard_is_offered_until_marked_and_the_mark_keeps_the_rest() {
        let home = tempfile::tempdir().unwrap();
        let path = home.path().join("helix-toolbox/settings.toml");
        // No settings yet: the first start.
        assert!(!set_up(&path));
        // Settings from before the wizard existed: a machine already set up.
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "theme = \"ocean\"\nicons = false\n").unwrap();
        assert!(set_up(&path));
        fs::write(&path, "theme = \"ocean\"\nset_up = false\n").unwrap();
        assert!(!set_up(&path));
        mark_set_up(&path).unwrap();
        assert!(set_up(&path));
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("theme = \"ocean\""), "{text}");
        assert!(text.contains("set_up = true"), "{text}");
        // From nothing, the file is made with only the mark.
        let fresh = home.path().join("fresh/settings.toml");
        mark_set_up(&fresh).unwrap();
        assert!(set_up(&fresh));
        assert_eq!(run_choices(&fresh), RunChoices::default());
    }

    #[test]
    fn run_choices_are_read_on_their_own() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        assert_eq!(run_choices(&path), RunChoices::default());
        fs::write(
            &path,
            "theme = \"solid\"\nkeep_going = true\nsnapshots = 3\n",
        )
        .unwrap();
        assert_eq!(
            run_choices(&path),
            RunChoices {
                keep_going: true,
                snapshots: true,
            }
        );
    }
}
