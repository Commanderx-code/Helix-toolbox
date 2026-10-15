//! Tracing to a log file in the state directory; `HELIXFORGE_LOG` sets the
//! level (error, warn, info, debug or trace; the default is info).

use helixforge_engine::history::state_dir;
use std::fs;
use tracing::Level;

pub fn init() {
    let level = std::env::var("HELIXFORGE_LOG")
        .ok()
        .and_then(|value| value.parse::<Level>().ok())
        .unwrap_or(Level::INFO);
    let Some(directory) = state_dir().map(|state| state.join("logs")) else {
        return;
    };
    if fs::create_dir_all(&directory).is_err() {
        return;
    }
    let Ok(file) = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(directory.join("helixforge.log"))
    else {
        return;
    };
    let _ = tracing_subscriber::fmt()
        .with_max_level(level)
        .with_ansi(false)
        .with_writer(file)
        .try_init();
}
