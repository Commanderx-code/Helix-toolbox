//! What carries over from HelixForge, the name before 1.7.
//!
//! The configuration and state folders were `helixforge`. The first start of
//! a later release moves each to `helix-toolbox` and leaves a link at the old
//! name, so an older release, or a script that reads the old path, still
//! finds the same files. Files of installed software under
//! `~/.local/share/helixforge` stay where they are: the launchers and
//! entries made for it point there.

use std::{
    io,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

/// The folder name before 1.7.
pub const OLD_NAME: &str = "helixforge";

/// The folder name from 1.7.
pub const NAME: &str = "helix-toolbox";

/// What happened to one folder.
#[derive(Debug, Eq, PartialEq)]
pub enum Moved {
    /// There was no old folder, or the new one was already there.
    Nothing,
    /// The old folder was moved, with a link left at its old name.
    Moved,
    /// The old folder could not be moved, so the new name links to it.
    Linked,
}

/// Move `base/helixforge` to `base/helix-toolbox`, leaving a link behind.
pub fn move_old_folder(base: &Path) -> io::Result<Moved> {
    let old = base.join(OLD_NAME);
    let new = base.join(NAME);
    if new.symlink_metadata().is_ok() {
        return Ok(Moved::Nothing);
    }
    let Ok(metadata) = old.symlink_metadata() else {
        return Ok(Moved::Nothing);
    };
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        // Something the user made: point the new name at it.
        symlink(OLD_NAME, &new)?;
        return Ok(Moved::Linked);
    }
    match std::fs::rename(&old, &new) {
        Ok(()) => {
            symlink(NAME, &old)?;
            Ok(Moved::Moved)
        }
        Err(_) => {
            symlink(OLD_NAME, &new)?;
            Ok(Moved::Linked)
        }
    }
}

/// The configuration and state folders' parents, from the XDG variables.
fn bases() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    [
        ("XDG_CONFIG_HOME", ".config"),
        ("XDG_STATE_HOME", ".local/state"),
    ]
    .into_iter()
    .filter_map(|(variable, fallback)| {
        std::env::var_os(variable)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    })
    .collect()
}

/// Move this user's old folders, once; what could not be done, to say.
pub fn move_old_folders() -> Vec<String> {
    bases()
        .into_iter()
        .filter_map(|base| {
            move_old_folder(&base).err().map(|error| {
                format!(
                    "{} could not be moved to {}: {error}",
                    base.join(OLD_NAME).display(),
                    base.join(NAME).display()
                )
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_folder_moves_and_its_name_still_reaches_it() {
        let base = tempfile::tempdir().unwrap();
        let old = base.path().join(OLD_NAME);
        std::fs::create_dir_all(old.join("profiles")).unwrap();
        std::fs::write(old.join("settings.toml"), "theme = \"neon\"\n").unwrap();
        assert_eq!(move_old_folder(base.path()).unwrap(), Moved::Moved);
        let new = base.path().join(NAME);
        assert!(new.symlink_metadata().unwrap().is_dir());
        assert_eq!(std::fs::read_link(&old).unwrap(), Path::new(NAME));
        assert_eq!(
            std::fs::read_to_string(old.join("settings.toml")).unwrap(),
            "theme = \"neon\"\n"
        );
        assert!(new.join("profiles").is_dir());
        // Once is enough.
        assert_eq!(move_old_folder(base.path()).unwrap(), Moved::Nothing);
    }

    #[test]
    fn nothing_happens_without_an_old_folder_or_with_a_new_one() {
        let base = tempfile::tempdir().unwrap();
        assert_eq!(move_old_folder(base.path()).unwrap(), Moved::Nothing);
        assert!(!base.path().join(NAME).exists());
        std::fs::create_dir(base.path().join(OLD_NAME)).unwrap();
        std::fs::create_dir(base.path().join(NAME)).unwrap();
        assert_eq!(move_old_folder(base.path()).unwrap(), Moved::Nothing);
        assert!(
            base.path()
                .join(OLD_NAME)
                .symlink_metadata()
                .unwrap()
                .is_dir()
        );
    }

    #[test]
    fn an_old_name_that_is_a_link_is_left_and_followed() {
        let base = tempfile::tempdir().unwrap();
        let elsewhere = base.path().join("synced");
        std::fs::create_dir(&elsewhere).unwrap();
        symlink(&elsewhere, base.path().join(OLD_NAME)).unwrap();
        assert_eq!(move_old_folder(base.path()).unwrap(), Moved::Linked);
        assert_eq!(
            std::fs::canonicalize(base.path().join(NAME)).unwrap(),
            std::fs::canonicalize(&elsewhere).unwrap()
        );
    }
}
