//! Tracing to a log file in the state directory; `HELIX_TOOLBOX_LOG` sets the
//! level (error, warn, info, debug or trace; the default is info), or
//! `HELIXFORGE_LOG`, its name before 1.7. The file
//! and its directory are private to the user, like the history beside them.

use helix_toolbox_engine::history::state_dir;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
};
use tracing::Level;

/// The log level the user asked for, if any.
pub fn requested() -> Option<String> {
    std::env::var("HELIX_TOOLBOX_LOG")
        .or_else(|_| std::env::var("HELIXFORGE_LOG"))
        .ok()
}

pub fn init() {
    let level = requested()
        .and_then(|value| value.parse::<Level>().ok())
        .unwrap_or(Level::INFO);
    let Some(directory) = state_dir().map(|state| state.join("logs")) else {
        return;
    };
    if fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&directory)
        .is_err()
    {
        return;
    }
    let Ok(file) = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(directory.join("helix-toolbox.log"))
    else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .with_writer(file)
        .try_init();
}
