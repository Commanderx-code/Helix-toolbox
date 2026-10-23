//! Tools installed from their projects' GitHub releases: the catalog pins
//! one file per architecture by its SHA-256. A run downloads it into a
//! private folder, checks it before anything is unpacked, takes only the
//! program out of it and puts it in `~/.local/bin`, without root. A small
//! record in `$XDG_STATE_HOME/helix-toolbox/releases/` says what was put
//! there, so its status, updates and drift can be told, and so a file
//! Helix Toolbox did not install is never overwritten or removed.

use crate::{Probe, Source};
use helix_toolbox_core::{
    Archive, Release, ReleaseAsset, compare_versions, valid_binary_name, valid_id,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{IsTerminal, Read, Write},
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};
use thiserror::Error;

/// Where releases are downloaded from.
pub const GITHUB: &str = "https://github.com";

/// The shell function a run's program defines to start Helix Toolbox again:
/// the release steps run inside it, as native steps do.
pub const SELF: &str = "hf_toolbox";

/// How plans show `SELF`.
pub const SELF_SHOWN: &str = "helix-toolbox";

/// What went wrong installing or removing a release.
#[derive(Debug, Error)]
pub enum ReleaseError {
    #[error("unsafe release value: {0}")]
    Invalid(String),
    #[error("{0} is already there and Helix Toolbox did not install it; move it away first")]
    NotOurs(String),
    #[error("{0} has changed since Helix Toolbox installed it; it was left as it is")]
    Changed(String),
    #[error("Helix Toolbox has no record of installing {0}")]
    NoRecord(String),
    #[error("downloading {0} failed")]
    Download(String),
    #[error("{0} does not match the SHA-256 the catalog pins; it was deleted")]
    Mismatch(String),
    #[error("{file} has no {path} to install")]
    Missing { file: String, path: String },
    #[error("no HOME to install into")]
    NoHome,
    #[error("{0}")]
    Io(String),
}

impl From<std::io::Error> for ReleaseError {
    fn from(error: std::io::Error) -> Self {
        ReleaseError::Io(error.to_string())
    }
}

/// What Helix Toolbox put in `~/.local/bin` for one entry.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub entry: String,
    /// The installed program.
    pub path: PathBuf,
    pub version: String,
    pub tag: String,
    pub repository: String,
    /// The installed program's SHA-256.
    pub sha256: String,
}

/// The folders a release goes into and is recorded in, and where it is
/// downloaded from.
#[derive(Clone, Debug)]
pub struct Places {
    /// `~/.local/bin`.
    pub bin: PathBuf,
    /// `$XDG_STATE_HOME/helix-toolbox/releases`.
    pub records: PathBuf,
    /// `https://github.com`; a `file://` folder in tests.
    pub base: String,
    /// What curl may use to reach `base`.
    pub protocols: String,
}

impl Places {
    /// This user's.
    pub fn here() -> Option<Self> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some(Places {
            bin: home.join(".local/bin"),
            records: crate::history::state_dir()?.join("releases"),
            base: GITHUB.to_string(),
            protocols: "=https".to_string(),
        })
    }
}

/// The record of the entry's release, when Helix Toolbox installed it.
pub fn record_in(records: &Path, entry: &str) -> Option<Record> {
    if !valid_id(entry) {
        return None;
    }
    let text = fs::read_to_string(records.join(format!("{entry}.toml"))).ok()?;
    let record: Record = toml::from_str(&text).ok()?;
    (record.entry == entry).then_some(record)
}

/// The record, when its program is still there.
pub fn installed_in(records: &Path, entry: &str) -> Option<Record> {
    record_in(records, entry).filter(|record| record.path.symlink_metadata().is_ok())
}

/// This user's record of the entry's release, with its program there.
pub fn installed(entry: &str) -> Option<Record> {
    installed_in(&Places::here()?.records, entry)
}

/// How a recorded release stands now.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Condition {
    /// As installed.
    Intact,
    /// Its program is gone.
    Missing,
    /// Its program is not the file that was installed.
    Changed,
}

/// Whether a recorded program is still the file Helix Toolbox installed.
pub fn condition(record: &Record) -> Condition {
    match sum_of(&record.path) {
        None if record.path.symlink_metadata().is_err() => Condition::Missing,
        Some(sum) if sum == record.sha256 => Condition::Intact,
        _ => Condition::Changed,
    }
}

/// Every release this user's records name, for drift.
pub fn records() -> Vec<Record> {
    let Some(places) = Places::here() else {
        return Vec::new();
    };
    records_in(&places.records)
}

/// Every record in a folder, in name order.
pub fn records_in(folder: &Path) -> Vec<Record> {
    let Ok(read) = fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut names: Vec<String> = read
        .filter_map(|item| item.ok()?.file_name().into_string().ok())
        .filter_map(|name| name.strip_suffix(".toml").map(String::from))
        .collect();
    names.sort();
    names
        .iter()
        .filter_map(|entry| record_in(folder, entry))
        .collect()
}

/// Whether the installed version is older than the release the catalog pins.
pub fn outdated(record: &Record, release: &Release) -> bool {
    compare_versions(&record.version, &release.version).is_lt()
}

/// The SHA-256 of a regular file, not following a link.
fn sum_of(path: &Path) -> Option<String> {
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// The probe that says the entry's release is installed.
pub fn probe(entry: &str, release: &Release) -> Probe {
    Probe::Release {
        entry: entry.to_string(),
        binary: release.binary.clone(),
    }
}

/// How plans and runs name a release source.
pub fn label(release: &Release) -> String {
    format!("the GitHub release ({})", release.tag)
}

/// The words of the command a run uses to install the entry's release for
/// `asset`'s architecture.
pub fn install_command(entry: &str, release: &Release, asset: &ReleaseAsset) -> Vec<String> {
    let mut words: Vec<String> = [
        SELF,
        "release",
        "install",
        entry,
        &release.repository,
        &release.tag,
        &release.version,
        &release.binary,
        &asset.file,
        &asset.sha256,
    ]
    .iter()
    .map(|word| word.to_string())
    .collect();
    words.extend(asset.path.clone());
    words
}

/// The source that installs the entry's release on `architecture`; None
/// where the project builds nothing for it.
pub fn source(entry: &str, release: &Release, architecture: &str) -> Option<Source> {
    let asset = release.asset_for(architecture)?;
    let mut requires = vec!["curl".to_string()];
    requires.extend(Archive::of(&asset.file).tool().map(String::from));
    Some(Source {
        label: label(release),
        commands: vec![install_command(entry, release, asset)],
        requires,
        probe: Some(probe(entry, release)),
    })
}

/// The source that removes the entry's release.
pub fn removal_source(entry: &str, release: &Release) -> Source {
    Source {
        label: label(release),
        commands: vec![
            [SELF, "release", "remove", entry, &release.binary]
                .iter()
                .map(|word| word.to_string())
                .collect(),
        ],
        requires: Vec::new(),
        probe: Some(probe(entry, release)),
    }
}

/// Install `asset` of the entry's `release` into `places`: download, check,
/// unpack, put in place, record. Returns the record.
pub fn install(
    entry: &str,
    release: &Release,
    asset: &ReleaseAsset,
    places: &Places,
) -> Result<Record, ReleaseError> {
    let shown = release.binary.clone();
    if !valid_id(entry)
        || !release.is_valid()
        || !release.assets.contains(asset)
        || !valid_binary_name(&release.binary)
    {
        return Err(ReleaseError::Invalid(shown));
    }
    let target = places.bin.join(&release.binary);
    // Only a file it installed itself, unchanged, is replaced.
    if target.symlink_metadata().is_ok() {
        let ours = record_in(&places.records, entry)
            .is_some_and(|record| record.path == target && condition(&record) == Condition::Intact);
        if !ours {
            return Err(ReleaseError::NotOurs(target.display().to_string()));
        }
    }
    let work = Work::new()?;
    let download = work.path.join(&asset.file);
    let url = release.url_from(&places.base, asset);
    println!("Downloading {url} …");
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--proto",
            &places.protocols,
            "--proto-redir",
            &places.protocols,
            "--retry",
            "2",
        ])
        .args(if std::io::stderr().is_terminal() {
            &["--progress-bar"][..]
        } else {
            &["--silent", "--show-error"][..]
        })
        .arg("--output")
        .arg(&download)
        .arg(&url)
        .status()
        .map_err(|error| ReleaseError::Io(format!("cannot run curl: {error}")))?;
    if !status.success() {
        return Err(ReleaseError::Download(url));
    }
    if sum_of(&download).as_deref() != Some(asset.sha256.as_str()) {
        let _ = fs::remove_file(&download);
        return Err(ReleaseError::Mismatch(asset.file.clone()));
    }
    println!(
        "Checked {} against the SHA-256 the catalog pins.",
        asset.file
    );
    let program = unpack(&download, asset, &work.path)?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o755)
        .create(&places.bin)?;
    // Written beside the target, then renamed over it, so a run that stops
    // part way never leaves half a program.
    let partial = places
        .bin
        .join(format!(".{}.helix-toolbox-new", release.binary));
    let _ = fs::remove_file(&partial);
    {
        let mut source = fs::File::open(&program)?;
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .custom_flags(nix::libc::O_NOFOLLOW)
            .open(&partial)?;
        std::io::copy(&mut source, &mut out)?;
        out.set_permissions(fs::Permissions::from_mode(0o755))?;
        out.sync_all()?;
    }
    fs::rename(&partial, &target).inspect_err(|_| {
        let _ = fs::remove_file(&partial);
    })?;
    let record = Record {
        entry: entry.to_string(),
        path: target.clone(),
        version: release.version.clone(),
        tag: release.tag.clone(),
        repository: release.repository.clone(),
        sha256: sum_of(&target)
            .ok_or_else(|| ReleaseError::Io(format!("cannot read {} back", target.display())))?,
    };
    write_record(&places.records, &record)?;
    println!(
        "Installed {} {} from {}'s release {}.",
        target.display(),
        release.version,
        release.repository,
        release.tag
    );
    let on_path = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|folder| folder == places.bin));
    if !on_path {
        println!(
            "{} is not on your PATH; add it to run {} by name.",
            places.bin.display(),
            release.binary
        );
    }
    Ok(record)
}

/// Remove the program Helix Toolbox installed for the entry, and its record.
/// A program that has changed since is left alone.
pub fn remove(entry: &str, binary: &str, places: &Places) -> Result<(), ReleaseError> {
    if !valid_id(entry) || !valid_binary_name(binary) {
        return Err(ReleaseError::Invalid(binary.to_string()));
    }
    let record = record_in(&places.records, entry)
        .filter(|record| record.path == places.bin.join(binary))
        .ok_or_else(|| ReleaseError::NoRecord(binary.to_string()))?;
    match condition(&record) {
        Condition::Changed => {
            return Err(ReleaseError::Changed(record.path.display().to_string()));
        }
        Condition::Intact => fs::remove_file(&record.path)?,
        Condition::Missing => {}
    }
    fs::remove_file(places.records.join(format!("{entry}.toml")))?;
    println!("Removed {}.", record.path.display());
    Ok(())
}

fn write_record(records: &Path, record: &Record) -> Result<(), ReleaseError> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(records)?;
    let text = toml::to_string(record).map_err(|error| ReleaseError::Io(error.to_string()))?;
    let path = records.join(format!("{}.toml", record.entry));
    let partial = records.join(format!(".{}.toml.new", record.entry));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&partial)?;
    file.write_all(text.as_bytes())?;
    file.sync_all()?;
    fs::rename(&partial, &path)?;
    Ok(())
}

/// Take the program out of a checked download; only `asset.path` is
/// unpacked, and it must be a plain file inside the folder.
fn unpack(download: &Path, asset: &ReleaseAsset, work: &Path) -> Result<PathBuf, ReleaseError> {
    let archive = Archive::of(&asset.file);
    let Some(member) = &asset.path else {
        return match archive {
            Archive::Bare => Ok(download.to_path_buf()),
            _ => Err(ReleaseError::Invalid(asset.file.clone())),
        };
    };
    let into = work.join("unpacked");
    fs::DirBuilder::new().mode(0o700).create(&into)?;
    let extract = |name: &str| -> Result<bool, ReleaseError> {
        let mut command = match archive {
            Archive::TarGz | Archive::TarXz => {
                let mut command = Command::new("tar");
                command
                    .arg(if archive == Archive::TarGz {
                        "-xzf"
                    } else {
                        "-xJf"
                    })
                    .arg(download)
                    .arg("-C")
                    .arg(&into)
                    .args(["--no-same-owner", "--no-same-permissions", "--"])
                    .arg(name);
                command
            }
            Archive::Zip => {
                let mut command = Command::new("unzip");
                command
                    .args(["-q", "-o"])
                    .arg(download)
                    .arg(name)
                    .arg("-d")
                    .arg(&into);
                command
            }
            Archive::Bare => return Err(ReleaseError::Invalid(asset.file.clone())),
        };
        let status = command
            .stderr(std::process::Stdio::null())
            .status()
            .map_err(|error| ReleaseError::Io(format!("cannot unpack {}: {error}", asset.file)))?;
        Ok(status.success())
    };
    let missing = || ReleaseError::Missing {
        file: asset.file.clone(),
        path: member.clone(),
    };
    // Some archives name their files `./name`.
    if !extract(member)? && !extract(&format!("./{member}"))? {
        return Err(missing());
    }
    let program = into.join(member);
    let metadata = program.symlink_metadata().map_err(|_| missing())?;
    let inside = fs::canonicalize(&program)
        .ok()
        .zip(fs::canonicalize(&into).ok())
        .is_some_and(|(program, into)| program.starts_with(into));
    if !metadata.is_file() || !inside {
        return Err(missing());
    }
    Ok(program)
}

/// A private folder for one download, removed when done.
struct Work {
    path: PathBuf,
}

impl Work {
    fn new() -> Result<Self, ReleaseError> {
        let base = std::env::temp_dir();
        for attempt in 0..16u32 {
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|time| time.subsec_nanos())
                .unwrap_or_default();
            let path = base.join(format!(
                "helix-toolbox-release-{}-{stamp}-{attempt}",
                std::process::id()
            ));
            match fs::DirBuilder::new().mode(0o700).create(&path) {
                Ok(()) => return Ok(Work { path }),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(ReleaseError::Io(
            "cannot make a private folder to download into".to_string(),
        ))
    }
}

impl Drop for Work {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sha(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// A release served from a folder, as curl's file:// sees it, with one
    /// tar.gz holding `tool-1.2.0/tool`.
    struct Served {
        _server: tempfile::TempDir,
        home: tempfile::TempDir,
        places: Places,
        release: Release,
    }

    fn served(contents: &[u8], pinned: Option<String>) -> Served {
        let server = tempfile::tempdir().unwrap();
        let folder = server.path().join("example/tool/releases/download/v1.2.0");
        fs::create_dir_all(folder.join("tool-1.2.0")).unwrap();
        fs::write(folder.join("tool-1.2.0/tool"), contents).unwrap();
        fs::write(folder.join("tool-1.2.0/README"), b"not this").unwrap();
        let status = Command::new("tar")
            .arg("-czf")
            .arg(folder.join("tool.tar.gz"))
            .arg("-C")
            .arg(&folder)
            .arg("tool-1.2.0")
            .status()
            .unwrap();
        assert!(status.success());
        let sum = pinned.unwrap_or_else(|| sha(&fs::read(folder.join("tool.tar.gz")).unwrap()));
        let home = tempfile::tempdir().unwrap();
        let places = Places {
            bin: home.path().join(".local/bin"),
            records: home.path().join("state/releases"),
            base: format!("file://{}", server.path().display()),
            protocols: "=file".to_string(),
        };
        let release = Release {
            repository: "example/tool".into(),
            tag: "v1.2.0".into(),
            version: "1.2.0".into(),
            binary: "tool".into(),
            assets: vec![ReleaseAsset {
                architecture: "x86_64".into(),
                file: "tool.tar.gz".into(),
                sha256: sum,
                path: Some("tool-1.2.0/tool".into()),
            }],
        };
        Served {
            _server: server,
            home,
            places,
            release,
        }
    }

    #[test]
    fn a_checked_release_is_installed_recorded_and_removed() {
        let served = served(b"#!/bin/sh\necho tool\n", None);
        let asset = &served.release.assets[0];
        let record = install("tool", &served.release, asset, &served.places).unwrap();
        let target = served.places.bin.join("tool");
        assert_eq!(record.path, target);
        assert_eq!(fs::read(&target).unwrap(), b"#!/bin/sh\necho tool\n");
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
        assert_eq!(record.sha256, sha(b"#!/bin/sh\necho tool\n"));
        assert_eq!(
            record_in(&served.places.records, "tool"),
            Some(record.clone())
        );
        assert_eq!(condition(&record), Condition::Intact);
        // Only the program came out of the archive.
        assert_eq!(fs::read_dir(&served.places.bin).unwrap().count(), 1);
        // Installing again replaces its own unchanged file.
        install("tool", &served.release, asset, &served.places).unwrap();

        let mut newer = served.release.clone();
        newer.version = "1.3.0".into();
        assert!(outdated(&record, &newer));
        assert!(!outdated(&record, &served.release));

        remove("tool", "tool", &served.places).unwrap();
        assert!(!target.exists());
        assert!(record_in(&served.places.records, "tool").is_none());
        assert!(matches!(
            remove("tool", "tool", &served.places),
            Err(ReleaseError::NoRecord(_))
        ));
        drop(served.home);
    }

    #[test]
    fn a_download_that_does_not_match_its_pin_is_never_unpacked() {
        let served = served(b"tool", Some("0".repeat(64)));
        let asset = &served.release.assets[0];
        let error = install("tool", &served.release, asset, &served.places).unwrap_err();
        assert!(matches!(error, ReleaseError::Mismatch(_)), "{error}");
        assert!(!served.places.bin.join("tool").exists());
        assert!(record_in(&served.places.records, "tool").is_none());
    }

    #[test]
    fn a_file_it_did_not_install_is_left_alone() {
        let served = served(b"tool", None);
        let asset = &served.release.assets[0];
        fs::create_dir_all(&served.places.bin).unwrap();
        let target = served.places.bin.join("tool");
        fs::write(&target, b"someone else's").unwrap();
        let error = install("tool", &served.release, asset, &served.places).unwrap_err();
        assert!(matches!(error, ReleaseError::NotOurs(_)), "{error}");
        assert_eq!(fs::read(&target).unwrap(), b"someone else's");

        // Installed, then changed by hand: neither replaced nor removed.
        fs::remove_file(&target).unwrap();
        let record = install("tool", &served.release, asset, &served.places).unwrap();
        fs::write(&target, b"edited").unwrap();
        assert_eq!(condition(&record), Condition::Changed);
        assert!(matches!(
            install("tool", &served.release, asset, &served.places),
            Err(ReleaseError::NotOurs(_))
        ));
        assert!(matches!(
            remove("tool", "tool", &served.places),
            Err(ReleaseError::Changed(_))
        ));
        assert_eq!(fs::read(&target).unwrap(), b"edited");
        fs::remove_file(&target).unwrap();
        assert_eq!(condition(&record), Condition::Missing);
        // A program already gone still lets its record go.
        remove("tool", "tool", &served.places).unwrap();
    }

    #[test]
    fn archives_that_name_their_files_dot_slash_unpack_too() {
        let work = tempfile::tempdir().unwrap();
        let tree = work.path().join("tree");
        fs::create_dir_all(&tree).unwrap();
        fs::write(tree.join("tool"), b"tool").unwrap();
        let archive = work.path().join("dotted.tar.gz");
        assert!(
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(&tree)
                .arg("./tool")
                .status()
                .unwrap()
                .success()
        );
        let asset = ReleaseAsset {
            architecture: "x86_64".into(),
            file: "dotted.tar.gz".into(),
            sha256: sha(&fs::read(&archive).unwrap()),
            path: Some("tool".into()),
        };
        let out = work.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let program = unpack(&archive, &asset, &out).unwrap();
        assert_eq!(fs::read(program).unwrap(), b"tool");
    }

    #[test]
    fn only_a_plain_file_inside_the_archive_is_taken() {
        let served = served(b"tool", None);
        let mut asset = served.release.assets[0].clone();
        // A member the archive does not have.
        asset.path = Some("tool-1.2.0/other".into());
        let mut release = served.release.clone();
        release.assets = vec![asset.clone()];
        let error = install("tool", &release, &asset, &served.places).unwrap_err();
        assert!(matches!(error, ReleaseError::Missing { .. }), "{error}");

        // A member that is a link out of the folder.
        let work = tempfile::tempdir().unwrap();
        let tree = work.path().join("tree");
        fs::create_dir_all(&tree).unwrap();
        std::os::unix::fs::symlink("/etc/hostname", tree.join("tool")).unwrap();
        let archive = work.path().join("link.tar.gz");
        assert!(
            Command::new("tar")
                .arg("-czf")
                .arg(&archive)
                .arg("-C")
                .arg(&tree)
                .arg("tool")
                .status()
                .unwrap()
                .success()
        );
        let linked = ReleaseAsset {
            architecture: "x86_64".into(),
            file: "link.tar.gz".into(),
            sha256: sha(&fs::read(&archive).unwrap()),
            path: Some("tool".into()),
        };
        let out = work.path().join("out");
        fs::create_dir_all(&out).unwrap();
        assert!(matches!(
            unpack(&archive, &linked, &out),
            Err(ReleaseError::Missing { .. })
        ));
    }

    #[test]
    fn unsafe_values_reach_nothing() {
        let served = served(b"tool", None);
        let asset = served.release.assets[0].clone();
        let mut bad = served.release.clone();
        bad.repository = "example/../tool".into();
        assert!(matches!(
            install("tool", &bad, &asset, &served.places),
            Err(ReleaseError::Invalid(_))
        ));
        assert!(matches!(
            install("Bad Id", &served.release, &asset, &served.places),
            Err(ReleaseError::Invalid(_))
        ));
        let mut other = asset.clone();
        other.file = "other.tar.gz".into();
        assert!(matches!(
            install("tool", &served.release, &other, &served.places),
            Err(ReleaseError::Invalid(_))
        ));
        assert!(matches!(
            remove("tool", "../tool", &served.places),
            Err(ReleaseError::Invalid(_))
        ));
        assert!(!served.places.bin.exists());
    }

    #[test]
    fn sources_name_the_release_and_what_they_need() {
        let served = served(b"tool", None);
        let release = &served.release;
        let source = source("tool", release, "x86_64").unwrap();
        assert_eq!(source.label, "the GitHub release (v1.2.0)");
        assert_eq!(source.requires, vec!["curl", "tar"]);
        assert_eq!(
            source.commands[0][..5],
            ["hf_toolbox", "release", "install", "tool", "example/tool"]
        );
        assert_eq!(source.commands[0].last().unwrap(), "tool-1.2.0/tool");
        assert_eq!(source.probe, Some(probe("tool", release)));
        assert!(super::source("tool", release, "aarch64").is_none());
        let removal = removal_source("tool", release);
        assert_eq!(
            removal.commands,
            vec![vec!["hf_toolbox", "release", "remove", "tool", "tool"]]
        );
    }
}
