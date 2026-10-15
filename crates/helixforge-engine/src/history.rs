//! An append-only record of runs in `$XDG_STATE_HOME/helixforge/history.jsonl`,
//! with saved terminal output beside it under `logs/`.

use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub unix_timestamp: u64,
    /// Entry ids, in the order they ran.
    pub entries: Vec<String>,
    pub names: Vec<String>,
    pub success: bool,
    /// The exit status, or why the run could not start.
    pub result: String,
    #[serde(default)]
    pub log: Option<PathBuf>,
}

fn state_dir() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(directory);
        if path.is_absolute() {
            return Some(path.join("helixforge"));
        }
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/helixforge"))
}

fn ensure_private(path: &PathBuf) -> std::io::Result<()> {
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

/// Append one record. Returns the record as written.
pub fn record(
    entries: &[(String, String)],
    success: bool,
    result: &str,
    log: Option<PathBuf>,
) -> std::io::Result<Record> {
    let directory =
        state_dir().ok_or_else(|| std::io::Error::other("no HOME or XDG_STATE_HOME"))?;
    ensure_private(&directory)?;
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

/// The most recent records first.
pub fn read(limit: usize) -> std::io::Result<Vec<Record>> {
    let Some(directory) = state_dir() else {
        return Ok(Vec::new());
    };
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

/// Save terminal output under `logs/` and return its path.
pub fn save_log(output: &[u8]) -> std::io::Result<PathBuf> {
    let directory = state_dir()
        .ok_or_else(|| std::io::Error::other("no HOME or XDG_STATE_HOME"))?
        .join("logs");
    ensure_private(&directory)?;
    let path = directory.join(format!("{}-{}.log", now(), std::process::id()));
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

    #[test]
    fn records_round_trip_newest_first() {
        let directory = tempfile::tempdir().unwrap();
        // SAFETY: tests in this module are the only readers of this variable.
        unsafe { env::set_var("XDG_STATE_HOME", directory.path()) };
        record(&[("one".into(), "One".into())], true, "exit 0", None).unwrap();
        let log = save_log(b"output").unwrap();
        record(
            &[("two".into(), "Two".into())],
            false,
            "exit 1",
            Some(log.clone()),
        )
        .unwrap();
        let records = read(10).unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].entries, ["two"]);
        assert_eq!(records[0].log.as_deref(), Some(log.as_path()));
        assert!(!records[0].success);
        assert_eq!(fs::read(log).unwrap(), b"output");
        unsafe { env::remove_var("XDG_STATE_HOME") };
    }
}
