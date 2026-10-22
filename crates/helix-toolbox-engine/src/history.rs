//! An append-only record of runs in `$XDG_STATE_HOME/helix-toolbox/history.jsonl`,
//! with saved terminal output beside it under `logs/`.
//!
//! A run in progress also has a marker under `running/` naming the process
//! that owns it. The finished record replaces the marker. When Helix Toolbox dies
//! during a run, the marker stays; the next time the history is read, a marker
//! whose process is gone becomes a record that says the run did not finish.
//! `history.jsonl` keeps one format throughout, so older versions read it.

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

/// The state directory: `$XDG_STATE_HOME/helix-toolbox` or `~/.local/state/helix-toolbox`.
pub fn state_dir() -> Option<PathBuf> {
    if let Some(directory) = env::var_os("XDG_STATE_HOME") {
        let path = PathBuf::from(directory);
        if path.is_absolute() {
            return Some(path.join("helix-toolbox"));
        }
    }
    env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state/helix-toolbox"))
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

/// The result recorded for a run whose Helix Toolbox stopped before it ended.
pub const UNFINISHED: &str = "did not finish: Helix Toolbox stopped during the run";

/// A run in progress, with its marker under `running/`.
#[derive(Debug)]
pub struct Pending {
    directory: PathBuf,
    path: PathBuf,
    marker: Marker,
}

#[derive(Debug, Serialize, Deserialize)]
struct Marker {
    /// The run as it would be recorded if it never finished, stamped with
    /// its start.
    record: Record,
    pid: u32,
    /// The owner's start time from `/proc/<pid>/stat`, so a reused process id
    /// is not mistaken for the owner.
    process_start: Option<u64>,
    /// How the run ended, once it has, if the record is not written yet.
    #[serde(default)]
    outcome: Option<(bool, String)>,
}

/// When a process started, in clock ticks after boot; `None` if it is gone.
fn process_start(pid: u32) -> Option<u64> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // After the command name: state is field 3 and the start time field 22.
    let (_, rest) = stat.rsplit_once(')')?;
    rest.split_whitespace().nth(19)?.parse().ok()
}

fn alive(marker: &Marker) -> bool {
    match (marker.process_start, process_start(marker.pid)) {
        (Some(expected), Some(actual)) => expected == actual,
        (None, actual) => actual.is_some(),
        (Some(_), None) => false,
    }
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)
}

/// Note that a run started, in the default history.
pub fn begin(entries: &[(String, String)]) -> std::io::Result<Pending> {
    begin_in(&default_dir()?, entries)
}

/// Note that a run started, in the history in `directory`.
pub fn begin_in(directory: &Path, entries: &[(String, String)]) -> std::io::Result<Pending> {
    let running = directory.join("running");
    ensure_private(&running)?;
    reconcile_in(directory);
    let pid = std::process::id();
    let marker = Marker {
        record: Record {
            unix_timestamp: now(),
            entries: entries.iter().map(|(id, _)| id.clone()).collect(),
            names: entries.iter().map(|(_, name)| name.clone()).collect(),
            success: false,
            result: UNFINISHED.to_string(),
            log: None,
        },
        pid,
        process_start: process_start(pid),
        outcome: None,
    };
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = running.join(format!("{stamp}-{pid}.json"));
    write_private(&path, &serde_json::to_vec(&marker)?)?;
    Ok(Pending {
        directory: directory.to_path_buf(),
        path,
        marker,
    })
}

impl Pending {
    /// Keep how the run ended in its marker, for when the record is written
    /// later (the interface writes it when its window closes).
    /// The history's folder.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    pub fn ended(&mut self, success: bool, result: &str) -> std::io::Result<()> {
        self.marker.outcome = Some((success, result.to_string()));
        let staging = self.path.with_extension("new");
        let _ = fs::remove_file(&staging);
        write_private(&staging, &serde_json::to_vec(&self.marker)?)?;
        fs::rename(&staging, &self.path)
    }

    /// Write the finished record and remove the marker.
    pub fn finish(
        mut self,
        success: bool,
        result: &str,
        log: Option<PathBuf>,
    ) -> std::io::Result<Record> {
        // If writing the record fails, the marker still knows the outcome.
        let _ = self.ended(success, result);
        let entries: Vec<(String, String)> = self
            .marker
            .record
            .entries
            .iter()
            .cloned()
            .zip(self.marker.record.names.iter().cloned())
            .collect();
        let record = record_in(&self.directory, &entries, success, result, log)?;
        let _ = fs::remove_file(&self.path);
        Ok(record)
    }
}

/// The runs of other Helix Toolbox processes that are still going, in the
/// default history: each as its entries' names.
pub fn active() -> Vec<String> {
    default_dir()
        .map(|directory| active_in(&directory))
        .unwrap_or_default()
}

/// `active` in the history in `directory`.
pub fn active_in(directory: &Path) -> Vec<String> {
    let Ok(listing) = fs::read_dir(directory.join("running")) else {
        return Vec::new();
    };
    listing
        .flatten()
        .filter(|item| {
            item.path()
                .extension()
                .is_some_and(|extension| extension == "json")
        })
        .filter_map(|item| {
            let bytes = fs::read(item.path()).ok()?;
            serde_json::from_slice::<Marker>(&bytes).ok()
        })
        .filter(|marker| {
            marker.pid != std::process::id() && marker.outcome.is_none() && alive(marker)
        })
        .map(|marker| marker.record.names.join(", "))
        .collect()
}

/// Turn the markers of runs whose Helix Toolbox has gone into records. Each
/// marker is claimed by renaming it first, so two Helix Toolbox processes never
/// record the same run. Returns how many runs were recorded.
pub fn reconcile_in(directory: &Path) -> usize {
    let Ok(listing) = fs::read_dir(directory.join("running")) else {
        return 0;
    };
    let mut recorded = 0;
    for item in listing.flatten() {
        let path = item.path();
        if path.extension().is_none_or(|extension| extension != "json") {
            continue;
        }
        let Ok(marker) = fs::read(&path)
            .map_err(|_| ())
            .and_then(|bytes| serde_json::from_slice::<Marker>(&bytes).map_err(|_| ()))
        else {
            continue;
        };
        if alive(&marker) {
            continue;
        }
        let claimed = path.with_extension(format!("claimed-{}", std::process::id()));
        if fs::rename(&path, &claimed).is_err() {
            continue;
        }
        let mut record = marker.record;
        if let Some((success, result)) = marker.outcome {
            record.success = success;
            record.result = result;
        }
        if append(directory, &record).is_ok() {
            let _ = fs::remove_file(&claimed);
            recorded += 1;
        } else {
            let _ = fs::rename(&claimed, &path);
        }
    }
    recorded
}

fn append(directory: &Path, record: &Record) -> std::io::Result<()> {
    ensure_private(directory)?;
    let mut file = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(directory.join("history.jsonl"))?;
    let mut line = serde_json::to_vec(record)?;
    line.push(b'\n');
    // One write per record, so concurrent appends do not interleave.
    file.write_all(&line)
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
    let record = Record {
        unix_timestamp: now(),
        entries: entries.iter().map(|(id, _)| id.clone()).collect(),
        names: entries.iter().map(|(_, name)| name.clone()).collect(),
        success,
        result: result.to_string(),
        log,
    };
    append(directory, &record)?;
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
    reconcile_in(directory);
    let path = directory.join("history.jsonl");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)?;
    let mut records: Vec<Record> = text
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // A run recorded late, after its Helix Toolbox died, carries its start time.
    records.sort_by_key(|record| record.unix_timestamp);
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

    fn entries() -> Vec<(String, String)> {
        vec![("one".into(), "One".into())]
    }

    fn markers(directory: &Path) -> usize {
        fs::read_dir(directory.join("running"))
            .map(|listing| listing.count())
            .unwrap_or(0)
    }

    /// Make a marker look as if its Helix Toolbox had died: point it at a
    /// process that has exited.
    fn orphan(pending: &Pending) {
        let mut child = std::process::Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        let mut marker: Marker = serde_json::from_slice(&fs::read(&pending.path).unwrap()).unwrap();
        marker.pid = pid;
        marker.process_start = Some(1);
        fs::write(&pending.path, serde_json::to_vec(&marker).unwrap()).unwrap();
    }

    #[test]
    fn a_finished_run_replaces_its_marker_with_the_record() {
        let directory = tempfile::tempdir().unwrap();
        let pending = begin_in(directory.path(), &entries()).unwrap();
        assert_eq!(markers(directory.path()), 1);
        assert_eq!(
            fs::metadata(&pending.path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        pending.finish(true, "exit 0", None).unwrap();
        assert_eq!(markers(directory.path()), 0);
        let records = read_in(directory.path(), 5).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].result, "exit 0");
    }

    #[test]
    fn other_runs_still_going_are_found() {
        let directory = tempfile::tempdir().unwrap();
        let pending = begin_in(directory.path(), &entries()).unwrap();
        // Its own run is not "another".
        assert!(active_in(directory.path()).is_empty());
        // Another process that is alive and has not ended its run.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .unwrap();
        let mut marker: Marker = serde_json::from_slice(&fs::read(&pending.path).unwrap()).unwrap();
        marker.pid = child.id();
        marker.process_start = process_start(child.id());
        fs::write(&pending.path, serde_json::to_vec(&marker).unwrap()).unwrap();
        assert_eq!(active_in(directory.path()).len(), 1);
        // Once its program has ended, it no longer counts.
        marker.outcome = Some((true, "exit 0".into()));
        fs::write(&pending.path, serde_json::to_vec(&marker).unwrap()).unwrap();
        assert!(active_in(directory.path()).is_empty());
        child.kill().unwrap();
        child.wait().unwrap();
        // Nor does a process that has gone.
        orphan(&pending);
        let mut marker: Marker = serde_json::from_slice(&fs::read(&pending.path).unwrap()).unwrap();
        marker.outcome = None;
        fs::write(&pending.path, serde_json::to_vec(&marker).unwrap()).unwrap();
        assert!(active_in(directory.path()).is_empty());
    }

    #[test]
    fn a_run_in_progress_is_not_recorded_while_its_process_lives() {
        let directory = tempfile::tempdir().unwrap();
        let _pending = begin_in(directory.path(), &entries()).unwrap();
        assert!(read_in(directory.path(), 5).unwrap().is_empty());
        assert_eq!(markers(directory.path()), 1);
    }

    #[test]
    fn a_run_whose_helix_toolbox_died_is_recorded_once_as_unfinished() {
        let directory = tempfile::tempdir().unwrap();
        record_in(directory.path(), &entries(), true, "exit 0", None).unwrap();
        let pending = begin_in(directory.path(), &[("two".into(), "Two".into())]).unwrap();
        let started = pending.marker.record.unix_timestamp;
        orphan(&pending);
        let records = read_in(directory.path(), 5).unwrap();
        assert_eq!(records.len(), 2);
        let unfinished = records
            .iter()
            .find(|record| record.entries == ["two"])
            .unwrap();
        assert_eq!(unfinished.result, UNFINISHED);
        assert!(!unfinished.success);
        assert_eq!(unfinished.unix_timestamp, started);
        assert_eq!(markers(directory.path()), 0);
        assert_eq!(read_in(directory.path(), 5).unwrap().len(), 2);
    }

    #[test]
    fn an_outcome_kept_before_helix_toolbox_died_is_what_gets_recorded() {
        let directory = tempfile::tempdir().unwrap();
        let mut pending = begin_in(directory.path(), &entries()).unwrap();
        pending.ended(true, "exit 0").unwrap();
        orphan(&pending);
        assert_eq!(reconcile_in(directory.path()), 1);
        let records = read_in(directory.path(), 5).unwrap();
        assert_eq!(records[0].result, "exit 0");
        assert!(records[0].success);
    }

    #[test]
    fn a_marker_claimed_by_another_process_is_not_recorded_twice() {
        let directory = tempfile::tempdir().unwrap();
        let pending = begin_in(directory.path(), &entries()).unwrap();
        orphan(&pending);
        // Another Helix Toolbox got there first and renamed it.
        fs::rename(&pending.path, pending.path.with_extension("claimed-1")).unwrap();
        assert_eq!(reconcile_in(directory.path()), 0);
        assert!(read_in(directory.path(), 5).unwrap().is_empty());
    }
}
