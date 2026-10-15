//! An append-only record of runs in `$XDG_STATE_HOME/helixforge/history.jsonl`,
//! with saved terminal output beside it under `logs/`.

use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub unix_timestamp: u64,
    /// Entry ids, in the order they ran.
    pub entries: Vec<String>,
    pub names: Vec<String>,
    pub success: bool,
    /// The exit status, "interrupted", or why the run could not start.
    pub result: String,
    #[serde(default)]
    pub log: Option<PathBuf>,
}

/// The state directory: `$XDG_STATE_HOME/helixforge` or `~/.local/state/helixforge`.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(directory);
        if path.is_absolute() {
            return Some(path.join("helixforge"));
        }
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/helixforge"))
}

fn default_dir() -> std::io::Result<PathBuf> {
    state_dir().ok_or_else(|| std::io::Error::other("no HOME or XDG_STATE_HOME"))
}

fn ensure_private(path: &Path) -> std::io::Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Append one record to the default history.
pub fn record(
    entries: &[(String, String)],
    success: bool,
    result: &str,
    log: Option<PathBuf>,
) -> std::io::Result<Record> {
    record_in(&default_dir()?, entries, success, result, log)
}

/// Append one record to the history in `directory`.
pub fn record_in(
    directory: &Path,
    entries: &[(String, String)],
    success: bool,
    result: &str,
    log: Option<PathBuf>,
) -> std::io::Result<Record> {
    ensure_private(directory)?;
    let record = Record {
        unix_timestamp: now(),
        entries: entries.iter().map(|(id, _)| id.clone()).collect(),
        names: entries.iter().map(|(_, name)| name.clone()).collect(),
        success,
        result: result.to_string(),
        log,
    };
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(directory.join("history.jsonl"))?;
    serde_json::to_writer(&mut file, &record)?;
    writeln!(file)?;
    Ok(record)
}

/// The most recent records first, from the default history.
pub fn read(limit: usize) -> std::io::Result<Vec<Record>> {
    match state_dir() {
        Some(directory) => read_in(&directory, limit),
        None => Ok(Vec::new()),
    }
}

/// The most recent records first, from the history in `directory`.
pub fn read_in(directory: &Path, limit: usize) -> std::io::Result<Vec<Record>> {
    let path = directory.join("history.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut records: Vec<Record> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    records.reverse();
    records.truncate(limit);
    Ok(records)
}

/// Save terminal output under `logs/` in the default state directory.
pub fn save_log(output: &[u8]) -> std::io::Result<PathBuf> {
    save_log_in(&default_dir()?, output)
}

/// Save terminal output under `logs/` in `directory` and return its path.
pub fn save_log_in(directory: &Path, output: &[u8]) -> std::io::Result<PathBuf> {
    let logs = directory.join("logs");
    ensure_private(&logs)?;
    let path = logs.join(format!("{}-{}.log", now(), std::process::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(output)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn records_round_trip_newest_first() {
        let directory = tempfile::tempdir().unwrap();
        record_in(
            directory.path(),
            &[("one".into(), "One".into())],
            true,
            "exit 0",
            None,
        )
        .unwrap();
        let log = save_log_in(directory.path(), b"output").unwrap();
        record_in(
            directory.path(),
            &[("two".into(), "Two".into())],
            false,
            "exit 1",
            Some(log.clone()),
        )
        .unwrap();
        let records = read_in(directory.path(), 10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].entries, ["two"]);
        assert_eq!(records[0].log.as_deref(), Some(log.as_path()));
        assert!(!records[0].success);
        assert_eq!(fs::read(&log).unwrap(), b"output");
        assert_eq!(
            fs::metadata(&log).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(directory.path().join("logs"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert!(
            read_in(&directory.path().join("nothing"), 5)
                .unwrap()
                .is_empty()
        );
    }
}
