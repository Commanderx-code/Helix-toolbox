//! Restic backups of your own files: where the repository is and how its
//! password is found, the snapshots and the files in them, a file's history
//! across snapshots, and recovery tests that restore one file and compare
//! it with a SHA-256 recorded earlier. Reading only asks restic; checking,
//! backing up, restoring and testing are plans, reviewed first. Helix
//! Toolbox never keeps a password: restic reads it from a file, a command
//! or its own environment variables.
//!
//! The repository comes from the `[restic]` table of `settings.toml`, else
//! restic's `RESTIC_REPOSITORY`, else the settings Command Center left
//! behind, read only.

use crate::{Plan, Step, quote};
use helix_toolbox_core::{Risk, printable};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub const TIMER: &str = "helix-toolbox-restic.timer";
pub const SERVICE: &str = "helix-toolbox-restic.service";
/// The first line of the units Helix Toolbox writes, so that it never
/// replaces or removes a file of someone else's.
const HEADER: &str = "# Written by Helix Toolbox: helix-toolbox restic timer on.";
/// The baselines kept.
const KEPT: usize = 50;
/// The largest file a baseline is recorded of.
const LARGEST: u64 = 100 * 1024 * 1024;

/// Where the repository's settings came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// The `[restic]` table of `settings.toml`.
    Settings,
    /// restic's own `RESTIC_REPOSITORY`.
    Environment,
    /// Command Center's settings, read only.
    CommandCenter,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Settings => "Helix Toolbox's settings",
            Origin::Environment => "RESTIC_REPOSITORY",
            Origin::CommandCenter => "Command Center's settings",
        }
    }
}

/// How restic gets the repository's password.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case", tag = "kind", content = "value")]
pub enum Password {
    /// `--password-file`.
    File(PathBuf),
    /// `--password-command`, such as a query of KWallet.
    Command(String),
    /// restic's own variables: RESTIC_PASSWORD_FILE, _COMMAND or RESTIC_PASSWORD.
    Environment,
}

impl Password {
    pub fn describe(&self) -> String {
        match self {
            Password::File(path) => format!("the file {}", path.display()),
            Password::Command(command) => format!("the command {}", printable(command)),
            Password::Environment => "restic's own environment variables".to_string(),
        }
    }
}

/// A repository, and what Helix Toolbox backs up into it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Config {
    pub repository: String,
    pub origin: Origin,
    pub password: Password,
    /// Folders the backup takes; the home folder unless set.
    pub paths: Vec<PathBuf>,
    /// What the backup leaves out, as restic's `--exclude` patterns.
    pub excludes: Vec<String>,
}

/// What is set up, or why nothing can be read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Found {
    Ready(Config),
    /// A repository is named, but something about it is wrong.
    Problem(String),
    /// Nothing names a repository.
    Missing,
}

/// The facts the settings are found from, so tests can supply them.
pub struct Facts<'a> {
    pub home: &'a Path,
    /// The `[restic]` table of `settings.toml`, when there is one.
    pub settings: Option<&'a toml::Table>,
    /// An environment variable.
    pub variable: &'a dyn Fn(&str) -> Option<String>,
    /// Command Center's `settings.json`, when there is one.
    pub command_center: Option<&'a str>,
    pub is_file: &'a dyn Fn(&Path) -> bool,
}

/// The repository set up here, for the recovery checks: its name only,
/// since listing its snapshots may ask for a password.
pub fn repository_here() -> Option<crate::recovery::Repository> {
    match here() {
        Found::Ready(config) => Some(crate::recovery::Repository {
            name: config.repository,
            newest: None,
        }),
        _ => None,
    }
}

/// Where Command Center kept its settings.
pub fn command_center_settings() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("io.helixstack.commandcenter/settings.json"))
}

/// The settings on this machine.
pub fn here() -> Found {
    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Found::Problem("there is no home folder".to_string());
    };
    let table = crate::settings::path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("restic")?.as_table().cloned());
    let command_center = command_center_settings().and_then(|path| fs::read_to_string(path).ok());
    find(&Facts {
        home: &home,
        settings: table.as_ref(),
        variable: &|name| env::var(name).ok().filter(|value| !value.is_empty()),
        command_center: command_center.as_deref(),
        is_file: &|path| path.is_file(),
    })
}

/// `~/` made the home folder.
fn expand(home: &Path, value: &str) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if value == "~" => home.to_path_buf(),
        None => PathBuf::from(value),
    }
}

fn text_list(table: &toml::Table, key: &str) -> Vec<String> {
    table
        .get(key)
        .and_then(toml::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(toml::Value::as_str)
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// What the settings say, checked.
pub fn find(facts: &Facts) -> Found {
    let text = |table: &toml::Table, key: &str| {
        table
            .get(key)
            .and_then(toml::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
    };
    // restic's own variables, else a KWallet entry that RESTIC_WALLET,
    // RESTIC_WALLET_ENTRY and RESTIC_WALLET_FOLDER name, as backup scripts
    // set them beside RESTIC_REPOSITORY.
    let from_environment = || -> Option<Password> {
        if [
            "RESTIC_PASSWORD_FILE",
            "RESTIC_PASSWORD_COMMAND",
            "RESTIC_PASSWORD",
        ]
        .iter()
        .any(|name| (facts.variable)(name).is_some())
        {
            return Some(Password::Environment);
        }
        let wallet = (facts.variable)("RESTIC_WALLET")?;
        let entry = (facts.variable)("RESTIC_WALLET_ENTRY")?;
        let folder = (facts.variable)("RESTIC_WALLET_FOLDER");
        Some(wallet_query(&wallet, folder.as_deref(), &entry))
    };
    let (repository, origin, password, paths, excludes) = if let Some(table) = facts.settings
        && let Some(repository) = text(table, "repository")
    {
        let password = match (
            text(table, "password_file"),
            text(table, "password_command"),
        ) {
            (Some(file), _) => Some(Password::File(expand(facts.home, &file))),
            (None, Some(command)) => Some(Password::Command(command)),
            (None, None) => from_environment(),
        };
        (
            repository,
            Origin::Settings,
            password,
            text_list(table, "paths"),
            table.get("excludes").map(|_| text_list(table, "excludes")),
        )
    } else if let Some(repository) = (facts.variable)("RESTIC_REPOSITORY") {
        (
            repository,
            Origin::Environment,
            from_environment(),
            Vec::new(),
            None,
        )
    } else if let Some(text) = facts.command_center
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
        && let Some(found) = value.get("integrations")
        && let Some(repository) = found
            .get("resticRepository")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    {
        let field = |key: &str| {
            found
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
        };
        let password = match (
            field("resticPasswordFile"),
            field("wallet"),
            field("walletEntry"),
        ) {
            (Some(file), _, _) => Some(Password::File(expand(facts.home, file))),
            (None, Some(wallet), Some(entry)) => {
                Some(wallet_query(wallet, field("walletFolder"), entry))
            }
            _ => from_environment(),
        };
        (
            repository.to_string(),
            Origin::CommandCenter,
            password,
            Vec::new(),
            None,
        )
    } else {
        return Found::Missing;
    };
    if let Err(why) = check_repository(&repository) {
        return Found::Problem(why);
    }
    let Some(password) = password else {
        return Found::Problem(
            "nothing gives restic the repository's password: set password_file or password_command under [restic] in settings.toml, or restic's RESTIC_PASSWORD_FILE".to_string(),
        );
    };
    if let Password::File(path) = &password
        && !(facts.is_file)(path)
    {
        return Found::Problem(format!("the password file {} is not there", path.display()));
    }
    let repository = match repository.strip_prefix("~/") {
        Some(_) => expand(facts.home, &repository).display().to_string(),
        None => repository,
    };
    let paths: Vec<PathBuf> = if paths.is_empty() {
        vec![facts.home.to_path_buf()]
    } else {
        paths.iter().map(|path| expand(facts.home, path)).collect()
    };
    if let Some(path) = paths.iter().find(|path| !path.is_absolute()) {
        return Found::Problem(format!(
            "a folder to back up, {}, is not a full path",
            path.display()
        ));
    }
    let excludes = excludes.unwrap_or_else(|| {
        [".cache", ".local/share/Trash"]
            .iter()
            .map(|folder| facts.home.join(folder).display().to_string())
            .collect()
    });
    Found::Ready(Config {
        repository,
        origin,
        password,
        paths,
        excludes,
    })
}

/// The password read from a KWallet entry.
fn wallet_query(wallet: &str, folder: Option<&str>, entry: &str) -> Password {
    Password::Command(format!(
        "kwallet-query -f {} -r {} {}",
        quote(folder.unwrap_or("Passwords")),
        quote(entry),
        quote(wallet)
    ))
}

/// A repository restic can be pointed at without leaking a password on a
/// command line: no option-like start, no password in a URL.
fn check_repository(repository: &str) -> Result<(), String> {
    if repository.starts_with('-') || repository.contains(['\0', '\n', '\r']) {
        return Err("the repository's name is not usable".to_string());
    }
    let url = repository.strip_prefix("rest:").unwrap_or(repository);
    if let Some((_, rest)) = url.split_once("://") {
        let authority = rest.split('/').next().unwrap_or_default();
        if let Some((user, _)) = authority.rsplit_once('@')
            && user.contains(':')
        {
            return Err(
                "the repository's address holds a password; give restic the password another way"
                    .to_string(),
            );
        }
    }
    Ok(())
}

/// restic's options for the repository and its password, which come before
/// the command.
pub fn global(config: &Config) -> Vec<String> {
    let mut args = vec!["--repo".to_string(), config.repository.clone()];
    match &config.password {
        Password::File(path) => {
            args.push("--password-file".into());
            args.push(path.display().to_string());
        }
        Password::Command(command) => {
            args.push("--password-command".into());
            args.push(command.clone());
        }
        Password::Environment => {}
    }
    args
}

/// Running restic: its output, or what it said went wrong.
pub trait Restic {
    fn run(&self, args: &[String]) -> Result<String, String>;
}

/// restic on this machine, with no terminal to ask a password in.
pub struct Here;

impl Restic for Here {
    fn run(&self, args: &[String]) -> Result<String, String> {
        let output = Command::new("restic")
            .args(args)
            .env("LC_ALL", "C")
            .env("RESTIC_PROGRESS_FPS", "0")
            .stdin(Stdio::null())
            .output()
            .map_err(|error| format!("cannot run restic: {error}"))?;
        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).into_owned())
        } else {
            let said = String::from_utf8_lossy(&output.stderr);
            let last = said
                .lines()
                .map(str::trim)
                .rfind(|line| !line.is_empty())
                .unwrap_or("restic failed");
            Err(printable(last.strip_prefix("Fatal: ").unwrap_or(last)))
        }
    }
}

fn with(config: &Config, rest: &[&str]) -> Vec<String> {
    let mut args = global(config);
    args.extend(rest.iter().map(|value| value.to_string()));
    args
}

/// A snapshot of the repository.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Snapshot {
    pub id: String,
    #[serde(default)]
    pub short_id: String,
    /// When it was taken, as restic writes it (RFC 3339).
    pub time: String,
    #[serde(default)]
    pub hostname: String,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
}

impl Snapshot {
    /// When it was taken, shortened to the minute.
    pub fn when(&self) -> String {
        let time = self.time.replace('T', " ");
        time.get(..16).unwrap_or(&time).to_string()
    }
}

/// A snapshot id, or `latest`.
pub fn valid_snapshot(id: &str) -> bool {
    id == "latest" || (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The repository answers with the password it was given.
pub fn access(restic: &dyn Restic, config: &Config) -> Result<(), String> {
    restic.run(&with(config, &["cat", "config"])).map(|_| ())
}

/// The snapshots, newest first: the latest 30 of each host and set of
/// folders.
pub fn snapshots(restic: &dyn Restic, config: &Config) -> Result<Vec<Snapshot>, String> {
    let text = restic.run(&with(config, &["snapshots", "--json", "--latest", "30"]))?;
    parse_snapshots(&text)
}

fn parse_snapshots(text: &str) -> Result<Vec<Snapshot>, String> {
    let mut found: Vec<Snapshot> = serde_json::from_str::<Option<Vec<Snapshot>>>(text.trim())
        .map_err(|error| format!("restic's list of snapshots did not read: {error}"))?
        .unwrap_or_default();
    for snapshot in &mut found {
        if snapshot.short_id.is_empty() {
            snapshot.short_id = snapshot.id.chars().take(8).collect();
        }
    }
    // RFC 3339 times of one zone sort as text; restic writes the local one.
    found.sort_by(|a, b| b.time.cmp(&a.time));
    Ok(found)
}

/// A file or folder in a snapshot.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Node {
    /// Left out of restic's search results, which give the path.
    #[serde(default)]
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub size: Option<u64>,
    #[serde(default)]
    pub mtime: Option<String>,
}

impl Node {
    pub fn is_dir(&self) -> bool {
        self.kind == "dir"
    }
}

/// A folder of a snapshot: an absolute path with nothing that climbs.
pub fn valid_folder(folder: &str) -> bool {
    folder.starts_with('/')
        && !folder.contains(['\0', '\n', '\r'])
        && !Path::new(folder)
            .components()
            .any(|part| part == Component::ParentDir)
}

/// What a folder of a snapshot holds, folders first.
pub fn files(
    restic: &dyn Restic,
    config: &Config,
    snapshot: &str,
    folder: &str,
) -> Result<Vec<Node>, String> {
    if !valid_snapshot(snapshot) {
        return Err(format!("{} is not a snapshot id", printable(snapshot)));
    }
    if !valid_folder(folder) {
        return Err(format!("{} is not a full path", printable(folder)));
    }
    let text = restic.run(&with(config, &["ls", "--json", snapshot, folder]))?;
    Ok(parse_nodes(&text, folder))
}

fn parse_nodes(text: &str, folder: &str) -> Vec<Node> {
    let folder = folder.trim_end_matches('/');
    let mut found: Vec<Node> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|value| value.get("struct_type").and_then(|kind| kind.as_str()) == Some("node"))
        .filter_map(|value| serde_json::from_value::<Node>(value).ok())
        // The folder itself, and anything deeper restic may list.
        .filter(|node| {
            Path::new(&node.path).parent().map(Path::to_path_buf)
                == Some(PathBuf::from(if folder.is_empty() { "/" } else { folder }))
        })
        .collect();
    found.sort_by(|a, b| b.is_dir().cmp(&a.is_dir()).then(a.name.cmp(&b.name)));
    found
}

/// How a file's version compares with the one before it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    /// The oldest snapshot that has it.
    First,
    /// Its size or modification time differs from the version before.
    Changed,
    Same,
}

/// One snapshot's copy of a file.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Version {
    pub snapshot: String,
    pub short_id: String,
    pub time: String,
    pub hostname: String,
    pub size: Option<u64>,
    pub mtime: Option<String>,
    pub change: Change,
}

/// A file found in the snapshots, newest version first.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct History {
    pub path: String,
    pub versions: Vec<Version>,
    /// Newer snapshots of the same machine and folder without it: it was
    /// deleted or moved after the newest version.
    pub missing_from: usize,
}

/// A pattern restic's find takes: a name, or a path, with wildcards.
pub fn valid_pattern(pattern: &str) -> bool {
    !pattern.is_empty()
        && pattern.len() <= 1024
        && !pattern.starts_with('-')
        && !pattern.contains(['\0', '\n', '\r'])
}

/// Every version of the files a pattern matches, across all snapshots.
pub fn history(
    restic: &dyn Restic,
    config: &Config,
    home: &Path,
    pattern: &str,
    ignore_case: bool,
) -> Result<Vec<History>, String> {
    let pattern = pattern.trim();
    let pattern = match pattern.strip_prefix("~/") {
        Some(_) => expand(home, pattern).display().to_string(),
        None => pattern.to_string(),
    };
    if !valid_pattern(&pattern) {
        return Err(format!(
            "{} is not a pattern to look for",
            printable(&pattern)
        ));
    }
    let all = parse_snapshots(&restic.run(&with(config, &["snapshots", "--json"]))?)?;
    let mut find = vec!["find", "--json"];
    if ignore_case {
        find.push("--ignore-case");
    }
    find.extend(["--", &pattern]);
    let matches = restic.run(&with(config, &find))?;
    parse_history(&all, &matches)
}

#[derive(Deserialize)]
struct Hit {
    snapshot: String,
    #[serde(default)]
    matches: Vec<Node>,
}

fn parse_history(snapshots: &[Snapshot], text: &str) -> Result<Vec<History>, String> {
    let found: Vec<Hit> = serde_json::from_str::<Option<Vec<Hit>>>(text.trim())
        .map_err(|error| format!("restic's search did not read: {error}"))?
        .unwrap_or_default();
    let by_id: HashMap<&str, &Snapshot> = snapshots
        .iter()
        .map(|snapshot| (snapshot.id.as_str(), snapshot))
        .collect();
    let mut files: HashMap<String, Vec<Version>> = HashMap::new();
    for hit in &found {
        let Some(snapshot) = by_id.get(hit.snapshot.as_str()) else {
            continue;
        };
        for node in &hit.matches {
            files.entry(node.path.clone()).or_default().push(Version {
                snapshot: snapshot.id.clone(),
                short_id: snapshot.short_id.clone(),
                time: snapshot.time.clone(),
                hostname: snapshot.hostname.clone(),
                size: node.size,
                mtime: node.mtime.clone(),
                change: Change::Same,
            });
        }
    }
    let mut history: Vec<History> = files
        .into_iter()
        .map(|(path, mut versions)| {
            versions.sort_by(|a, b| b.time.cmp(&a.time));
            versions.dedup_by(|a, b| a.snapshot == b.snapshot);
            let count = versions.len();
            for index in 0..count {
                versions[index].change = if index + 1 == count {
                    Change::First
                } else if versions[index].size != versions[index + 1].size
                    || versions[index].mtime != versions[index + 1].mtime
                {
                    Change::Changed
                } else {
                    Change::Same
                };
            }
            let newest = &versions[0];
            let missing_from = snapshots
                .iter()
                .filter(|snapshot| {
                    snapshot.time > newest.time
                        && snapshot.hostname == newest.hostname
                        && snapshot
                            .paths
                            .iter()
                            .any(|folder| Path::new(&path).starts_with(folder))
                })
                .count();
            History {
                path,
                versions,
                missing_from,
            }
        })
        .collect();
    history.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(history)
}

fn restic_step(id: &str, name: &str, program: String, display: String) -> Step {
    Step::Shell {
        id: id.to_string(),
        name: name.to_string(),
        program,
        display,
        requires: vec!["restic".to_string()],
        changes: Vec::new(),
    }
}

fn plan_of(step: Step, notes: Vec<String>, risk: Risk) -> Plan {
    Plan {
        steps: vec![step],
        notes,
        requirements: vec!["restic".to_string()],
        privileged: false,
        risk,
        depends: vec![Vec::new()],
        ..Plan::default()
    }
}

/// The command line as plans show it, with each part quoted.
fn shown(args: &[String]) -> String {
    let mut line = String::from("restic");
    for arg in args {
        line.push(' ');
        line.push_str(&quote(arg));
    }
    printable(&line)
}

/// Check the repository's structure (not every byte of its data).
pub fn plan_check(config: &Config) -> Plan {
    let args = with(config, &["check"]);
    plan_of(
        restic_step(
            "restic-check",
            "Check the restic repository",
            shown(&args),
            shown(&args),
        ),
        vec![
            "It reads the repository's index and structure; the data itself is not read back."
                .to_string(),
        ],
        Risk::Low,
    )
}

/// Back up the folders the settings name, leaving out their excludes and
/// caches marked as such.
pub fn plan_backup(config: &Config) -> Plan {
    let mut rest = vec!["backup".to_string(), "--exclude-caches".to_string()];
    for exclude in &config.excludes {
        rest.push("--exclude".to_string());
        rest.push(exclude.clone());
    }
    rest.push("--".to_string());
    rest.extend(config.paths.iter().map(|path| path.display().to_string()));
    let mut args = global(config);
    args.extend(rest);
    plan_of(
        restic_step(
            "restic-backup",
            "Back up to the restic repository",
            shown(&args),
            shown(&args),
        ),
        vec![format!(
            "It takes a new snapshot of {}; nothing on this machine changes.",
            config
                .paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )],
        Risk::Low,
    )
}

/// A folder to restore into: new, with a parent that is in the home folder.
pub fn check_target(home: &Path, target: &Path) -> Result<PathBuf, String> {
    let target = if target.is_absolute() {
        target.to_path_buf()
    } else {
        return Err("restore into a full path, such as ~/restored".to_string());
    };
    if target.components().any(|part| part == Component::ParentDir) {
        return Err("the folder to restore into must not climb with ..".to_string());
    }
    if target.symlink_metadata().is_ok() {
        return Err(format!(
            "{} is there already: restore into a new folder, so nothing is overwritten",
            target.display()
        ));
    }
    let (Some(parent), Some(_)) = (target.parent(), target.file_name()) else {
        return Err("restore into a folder of its own".to_string());
    };
    let parent = parent
        .canonicalize()
        .map_err(|_| format!("{} is not there", parent.display()))?;
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    if !parent.starts_with(&home) {
        return Err(format!(
            "restore into your home folder ({}), not {}",
            home.display(),
            parent.display()
        ));
    }
    Ok(parent.join(target.file_name().unwrap_or_default()))
}

/// Whether this restic restores without overwriting and verifies what it
/// wrote (0.17 and later).
pub fn restores_safely(restic: &dyn Restic) -> bool {
    restic
        .run(&["restore".to_string(), "--help".to_string()])
        .is_ok_and(|help| help.contains("--overwrite") && help.contains("--verify"))
}

/// Restore a snapshot, or one path of it, into a new folder.
pub fn plan_restore(
    config: &Config,
    snapshot: &str,
    target: &Path,
    include: Option<&str>,
) -> Result<Plan, String> {
    if !valid_snapshot(snapshot) {
        return Err(format!("{} is not a snapshot id", printable(snapshot)));
    }
    if let Some(include) = include
        && !valid_folder(include)
    {
        return Err(format!(
            "{} is not a full path in the snapshot",
            printable(include)
        ));
    }
    let target = target.display().to_string();
    let mut rest = vec![
        "restore".to_string(),
        snapshot.to_string(),
        "--target".to_string(),
        target.clone(),
        "--overwrite".to_string(),
        "never".to_string(),
        "--verify".to_string(),
    ];
    if let Some(include) = include {
        rest.push("--include".to_string());
        rest.push(include.to_string());
    }
    let mut args = global(config);
    args.extend(rest);
    let program = format!("mkdir -- {}\n{}", quote(&target), shown(&args));
    let display = format!("mkdir {}; {}", quote(&target), shown(&args));
    Ok(plan_of(
        restic_step(
            "restic-restore",
            "Restore from the restic repository",
            program,
            display,
        ),
        vec![format!(
            "It writes only into the new folder {target}; nothing there is overwritten, and what it writes is read back."
        )],
        Risk::Low,
    ))
}

/// A file's SHA-256 recorded, to test a restore of it against.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Baseline {
    pub id: String,
    pub path: String,
    pub sha256: String,
    /// When, in seconds since 1970.
    pub recorded: u64,
}

pub fn baselines_path(state: &Path) -> PathBuf {
    state.join("recovery-baselines.json")
}

/// The baselines recorded, newest first.
pub fn baselines(state: &Path) -> Vec<Baseline> {
    fs::read_to_string(baselines_path(state))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Record a file of the home folder's SHA-256, now.
pub fn record(state: &Path, home: &Path, file: &Path, now: u64) -> Result<Baseline, String> {
    let path = file
        .canonicalize()
        .map_err(|_| format!("{} is not there", file.display()))?;
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err(format!("{} is not a file", path.display()));
    }
    if !path.starts_with(&home) {
        return Err(format!("{} is not in your home folder", path.display()));
    }
    if metadata.len() > LARGEST {
        return Err(format!("{} is larger than 100 MB", path.display()));
    }
    let mut hasher = Sha256::new();
    let mut opened = fs::File::open(&path).map_err(|error| error.to_string())?;
    let mut buffer = [0u8; 65536];
    loop {
        let read = opened
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    let sha256: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let mut all = baselines(state);
    let mut id = (now * 1000).to_string();
    while all.iter().any(|baseline| baseline.id == id) {
        id = (id.parse::<u64>().unwrap_or_default() + 1).to_string();
    }
    let baseline = Baseline {
        id,
        path: path.display().to_string(),
        sha256,
        recorded: now,
    };
    all.insert(0, baseline.clone());
    all.truncate(KEPT);
    fs::create_dir_all(state).map_err(|error| error.to_string())?;
    let staging = baselines_path(state).with_extension("json.new");
    fs::write(
        &staging,
        serde_json::to_string_pretty(&all).unwrap_or_default() + "\n",
    )
    .map_err(|error| error.to_string())?;
    fs::rename(&staging, baselines_path(state)).map_err(|error| error.to_string())?;
    Ok(baseline)
}

/// A baseline by its place in the list (1 is the newest) or its id.
pub fn find_baseline<'a>(all: &'a [Baseline], which: &str) -> Option<&'a Baseline> {
    all.iter()
        .find(|baseline| baseline.id == which)
        .or_else(|| {
            which
                .parse::<usize>()
                .ok()
                .filter(|&place| place >= 1)
                .and_then(|place| all.get(place - 1))
        })
}

/// Restore the baseline's file from a snapshot into a folder of its own and
/// compare its SHA-256 with the one recorded; the folder is removed after.
pub fn plan_test(config: &Config, baseline: &Baseline, snapshot: &str) -> Result<Plan, String> {
    if !valid_snapshot(snapshot) {
        return Err(format!("{} is not a snapshot id", printable(snapshot)));
    }
    if baseline.sha256.len() != 64 || !baseline.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err("the baseline's SHA-256 is not usable".to_string());
    }
    let mut args = global(config);
    args.extend([
        "dump".to_string(),
        snapshot.to_string(),
        baseline.path.clone(),
    ]);
    let program = [
        "probe=$(mktemp -d)".to_string(),
        "trap 'rm -f -- \"$probe/restored\"; rmdir -- \"$probe\"' EXIT".to_string(),
        format!("{} > \"$probe/restored\" || exit 1", shown(&args)),
        "actual=$(sha256sum -- \"$probe/restored\")".to_string(),
        "actual=${actual%% *}".to_string(),
        format!(
            "if [ \"$actual\" != {expected} ]; then printf 'FAIL: %s from %s has SHA-256 %s, not %s as recorded\\n' {path} {snapshot} \"$actual\" {expected}; exit 1; fi",
            expected = quote(&baseline.sha256),
            path = quote(&baseline.path),
            snapshot = quote(snapshot),
        ),
        format!(
            "printf 'PASS: %s from %s matches the SHA-256 recorded\\n' {} {}",
            quote(&baseline.path),
            quote(snapshot)
        ),
    ]
    .join("\n");
    let display = format!(
        "{} > <a new temporary folder>/restored; sha256sum, compared with {}",
        shown(&args),
        baseline.sha256
    );
    let mut plan = plan_of(
        restic_step(
            "restic-recovery-test",
            "Test a restore against its baseline",
            program,
            display,
        ),
        vec![format!(
            "It restores {} into a temporary folder, compares it and removes it; nothing else changes.",
            baseline.path
        )],
        Risk::Low,
    );
    plan.requirements.push("sha256sum".to_string());
    if let Some(Step::Shell { requires, .. }) = plan.steps.first_mut() {
        requires.push("sha256sum".to_string());
    }
    Ok(plan)
}

/// How often the timer backs up.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Every {
    Day,
    Week,
}

impl Every {
    fn calendar(self) -> &'static str {
        match self {
            Every::Day => "daily",
            Every::Week => "weekly",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Every::Day => "every day",
            Every::Week => "every week",
        }
    }
}

/// The timer's state: off, or on and how often.
pub fn timer(units: &Path) -> Option<Every> {
    let text = fs::read_to_string(units.join(TIMER)).ok()?;
    if !text.starts_with(HEADER)
        || units
            .join("timers.target.wants")
            .join(TIMER)
            .symlink_metadata()
            .is_err()
    {
        return None;
    }
    if text.contains("OnCalendar=weekly") {
        Some(Every::Week)
    } else {
        Some(Every::Day)
    }
}

fn quoted_unit(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

pub fn service_unit(program: &Path) -> String {
    format!(
        "{HEADER}\n# helix-toolbox restic timer off removes it.\n\
         [Unit]\n\
         Description=Helix Toolbox: back up to the restic repository\n\
         \n\
         [Service]\n\
         Type=oneshot\n\
         ExecStart={} restic backup --yes\n\
         Nice=10\n\
         IOSchedulingClass=idle\n",
        quoted_unit(&program.to_string_lossy())
    )
}

pub fn timer_unit(every: Every) -> String {
    format!(
        "{HEADER}\n# helix-toolbox restic timer off removes it.\n\
         [Unit]\n\
         Description=Helix Toolbox: back up to the restic repository {}\n\
         \n\
         [Timer]\n\
         OnCalendar={}\n\
         Persistent=true\n\
         RandomizedDelaySec=30min\n\
         \n\
         [Install]\n\
         WantedBy=timers.target\n",
        every.label(),
        every.calendar()
    )
}

/// A file Helix Toolbox may replace or remove: absent, or one it wrote.
fn ours(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(metadata) if metadata.file_type().is_symlink() => Err(format!(
            "{} is a link Helix Toolbox did not make; it is left alone",
            path.display()
        )),
        Ok(_) => match fs::read_to_string(path) {
            Ok(text) if text.starts_with(HEADER) => Ok(()),
            _ => Err(format!(
                "{} was not written by Helix Toolbox; it is left alone",
                path.display()
            )),
        },
    }
}

/// Write the timer and its service and enable the timer. The backup it
/// runs has no terminal and none of your environment, so the password must
/// come from a file or a command.
pub fn timer_on(
    units: &Path,
    program: &Path,
    config: &Config,
    every: Every,
    systemctl: &dyn Fn(&[&str]) -> Option<bool>,
) -> Result<crate::notifications::Started, String> {
    if config.password == Password::Environment {
        return Err(
            "the timer's backup runs without your environment: set password_file or password_command under [restic] in settings.toml".to_string(),
        );
    }
    if !program.is_absolute() {
        return Err(format!(
            "the command line is not installed where a timer can run it ({})",
            program.display()
        ));
    }
    if systemctl(&["--version"]).is_none() {
        return Err("this machine has no systemd, which runs the timer".to_string());
    }
    ours(&units.join(SERVICE))?;
    ours(&units.join(TIMER))?;
    let wants = units.join("timers.target.wants");
    fs::create_dir_all(&wants)
        .map_err(|error| format!("cannot make {}: {error}", wants.display()))?;
    for (name, text) in [(SERVICE, service_unit(program)), (TIMER, timer_unit(every))] {
        let path = units.join(name);
        let staging = units.join(format!("{name}.new"));
        fs::write(&staging, text)
            .and_then(|()| fs::rename(&staging, &path))
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    let link = wants.join(TIMER);
    if link.symlink_metadata().is_ok() {
        fs::remove_file(&link)
            .map_err(|error| format!("cannot replace {}: {error}", link.display()))?;
    }
    std::os::unix::fs::symlink(units.join(TIMER), &link)
        .map_err(|error| format!("cannot enable the timer: {error}"))?;
    let answered =
        systemctl(&["daemon-reload"]) == Some(true) && systemctl(&["restart", TIMER]) == Some(true);
    Ok(if answered {
        crate::notifications::Started::Now
    } else {
        crate::notifications::Started::AtLogin
    })
}

/// Stop the timer and remove the files Helix Toolbox wrote for it.
pub fn timer_off(units: &Path, systemctl: &dyn Fn(&[&str]) -> Option<bool>) -> Result<(), String> {
    ours(&units.join(SERVICE))?;
    ours(&units.join(TIMER))?;
    let _ = systemctl(&["stop", TIMER]);
    for path in [
        units.join("timers.target.wants").join(TIMER),
        units.join(TIMER),
        units.join(SERVICE),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("cannot remove {}: {error}", path.display())),
        }
    }
    let _ = systemctl(&["daemon-reload"]);
    Ok(())
}

/// The user's systemd units folder.
pub fn units_here() -> Option<PathBuf> {
    crate::notifications::Places::here().map(|places| places.units)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        answers: Vec<(&'static str, Result<String, String>)>,
        asked: RefCell<Vec<Vec<String>>>,
    }

    impl Restic for Fake {
        fn run(&self, args: &[String]) -> Result<String, String> {
            self.asked.borrow_mut().push(args.to_vec());
            let line = args.join(" ");
            self.answers
                .iter()
                .find(|(start, _)| line.contains(start))
                .map(|(_, answer)| answer.clone())
                .unwrap_or_else(|| Err(format!("not answered: {line}")))
        }
    }

    fn config() -> Config {
        Config {
            repository: "/backups/repo".into(),
            origin: Origin::Settings,
            password: Password::File("/home/me/.restic-password".into()),
            paths: vec!["/home/me".into()],
            excludes: vec!["/home/me/.cache".into()],
        }
    }

    fn found(
        settings: Option<&str>,
        variables: &[(&str, &str)],
        command_center: Option<&str>,
    ) -> Found {
        let table: Option<toml::Table> = settings.map(|text| text.parse().unwrap());
        let variables: HashMap<String, String> = variables
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        find(&Facts {
            home: Path::new("/home/me"),
            settings: table.as_ref(),
            variable: &|name| variables.get(name).cloned(),
            command_center,
            is_file: &|path| path == Path::new("/home/me/.restic-password"),
        })
    }

    #[test]
    fn the_repository_comes_from_settings_then_restic_then_command_center() {
        let Found::Ready(from_settings) = found(
            Some(
                "repository = \"~/backups\"\npassword_file = \"~/.restic-password\"\npaths = [\"~/Documents\"]\nexcludes = []\n",
            ),
            &[("RESTIC_REPOSITORY", "/elsewhere")],
            None,
        ) else {
            panic!()
        };
        assert_eq!(from_settings.repository, "/home/me/backups");
        assert_eq!(from_settings.origin, Origin::Settings);
        assert_eq!(from_settings.paths, [PathBuf::from("/home/me/Documents")]);
        assert!(from_settings.excludes.is_empty());
        assert_eq!(
            global(&from_settings),
            [
                "--repo",
                "/home/me/backups",
                "--password-file",
                "/home/me/.restic-password"
            ]
        );
        let Found::Ready(from_restic) = found(
            None,
            &[
                ("RESTIC_REPOSITORY", "sftp:host:/repo"),
                ("RESTIC_PASSWORD_FILE", "/x"),
            ],
            None,
        ) else {
            panic!()
        };
        assert_eq!(from_restic.password, Password::Environment);
        assert_eq!(from_restic.paths, [PathBuf::from("/home/me")]);
        assert_eq!(from_restic.excludes.len(), 2);
        let command_center = r#"{"integrations": {"resticRepository": "/mnt/backup/restic", "wallet": "kdewallet", "walletEntry": "restic"}}"#;
        let Found::Ready(from_command_center) = found(None, &[], Some(command_center)) else {
            panic!()
        };
        assert_eq!(from_command_center.origin, Origin::CommandCenter);
        assert_eq!(
            from_command_center.password,
            Password::Command("kwallet-query -f 'Passwords' -r 'restic' 'kdewallet'".into())
        );
        let Found::Ready(from_wallet) = found(
            None,
            &[
                ("RESTIC_REPOSITORY", "/r"),
                ("RESTIC_WALLET", "kdewallet"),
                ("RESTIC_WALLET_ENTRY", "restic"),
                ("RESTIC_WALLET_FOLDER", "Backups"),
            ],
            None,
        ) else {
            panic!()
        };
        assert_eq!(
            from_wallet.password,
            Password::Command("kwallet-query -f 'Backups' -r 'restic' 'kdewallet'".into())
        );
        assert_eq!(found(None, &[], None), Found::Missing);
    }

    #[test]
    fn a_password_is_never_taken_from_the_address_and_must_come_from_somewhere() {
        for repository in [
            "rest:https://me:secret@host:8000/repo",
            "s3:https://me:secret@s3.example.com/bucket",
            "-oProxy=x",
        ] {
            let settings =
                format!("repository = \"{repository}\"\npassword_file = \"~/.restic-password\"\n");
            assert!(
                matches!(found(Some(&settings), &[], None), Found::Problem(_)),
                "{repository}"
            );
        }
        assert!(matches!(
            found(
                Some(
                    "repository = \"rest:https://me@host/repo\"\npassword_file = \"~/.restic-password\"\n"
                ),
                &[],
                None
            ),
            Found::Ready(_)
        ));
        let Found::Problem(why) = found(Some("repository = \"/r\"\n"), &[], None) else {
            panic!()
        };
        assert!(why.contains("password"));
        let Found::Problem(why) = found(
            Some("repository = \"/r\"\npassword_file = \"/nope\"\n"),
            &[],
            None,
        ) else {
            panic!()
        };
        assert!(why.contains("/nope"));
    }

    #[test]
    fn snapshots_files_and_a_files_history_are_read_from_restic() {
        let listing = r#"[
            {"id": "aaaaaaaa11", "short_id": "aaaaaaaa", "time": "2026-10-01T10:00:00+02:00", "hostname": "t15p", "paths": ["/home/me"]},
            {"id": "bbbbbbbb22", "short_id": "bbbbbbbb", "time": "2026-10-02T10:00:00+02:00", "hostname": "t15p", "paths": ["/home/me"]},
            {"id": "cccccccc33", "short_id": "cccccccc", "time": "2026-10-03T10:00:00+02:00", "hostname": "t15p", "paths": ["/home/me"]},
            {"id": "dddddddd44", "short_id": "dddddddd", "time": "2026-10-04T10:00:00+02:00", "hostname": "t15p", "paths": ["/home/me"]}
        ]"#;
        let ls = [
            r#"{"time":"2026-10-04T10:00:00+02:00","paths":["/home/me"],"id":"dddddddd44","struct_type":"snapshot"}"#,
            r#"{"name":"me","type":"dir","path":"/home/me","struct_type":"node"}"#,
            r#"{"name":"notes.txt","type":"file","path":"/home/me/notes.txt","size":12,"struct_type":"node"}"#,
            r#"{"name":"Documents","type":"dir","path":"/home/me/Documents","struct_type":"node"}"#,
        ]
        .join("\n");
        let matches = r#"[
            {"matches": [{"type": "file", "path": "/home/me/notes.txt", "size": 10, "mtime": "a"}], "hits": 1, "snapshot": "aaaaaaaa11"},
            {"matches": [{"name": "notes.txt", "type": "file", "path": "/home/me/notes.txt", "size": 10, "mtime": "a"}], "hits": 1, "snapshot": "bbbbbbbb22"},
            {"matches": [{"name": "notes.txt", "type": "file", "path": "/home/me/notes.txt", "size": 12, "mtime": "b"}], "hits": 1, "snapshot": "cccccccc33"}
        ]"#;
        let fake = Fake {
            answers: vec![
                ("snapshots --json --latest 30", Ok(listing.to_string())),
                ("snapshots --json", Ok(listing.to_string())),
                ("ls --json", Ok(ls)),
                ("find --json", Ok(matches.to_string())),
            ],
            asked: RefCell::new(Vec::new()),
        };
        let listed = snapshots(&fake, &config()).unwrap();
        assert_eq!(listed[0].short_id, "dddddddd");
        assert_eq!(listed[0].when(), "2026-10-04 10:00");
        let nodes = files(&fake, &config(), "dddddddd", "/home/me").unwrap();
        let names: Vec<&str> = nodes.iter().map(|node| node.name.as_str()).collect();
        assert_eq!(names, ["Documents", "notes.txt"]);
        assert!(files(&fake, &config(), "nothex!", "/home/me").is_err());
        assert!(files(&fake, &config(), "dddddddd", "/home/../etc").is_err());
        let found = history(&fake, &config(), Path::new("/home/me"), "notes.txt", false).unwrap();
        assert_eq!(found.len(), 1);
        let changes: Vec<Change> = found[0]
            .versions
            .iter()
            .map(|version| version.change)
            .collect();
        assert_eq!(changes, [Change::Changed, Change::Same, Change::First]);
        // The newest snapshot of the same folder no longer has it.
        assert_eq!(found[0].missing_from, 1);
        assert!(history(&fake, &config(), Path::new("/home/me"), "-x", false).is_err());
        // The repository and its password always come before the command.
        let first = &fake.asked.borrow()[0];
        assert_eq!(
            &first[..4],
            [
                "--repo",
                "/backups/repo",
                "--password-file",
                "/home/me/.restic-password"
            ]
        );
    }

    #[test]
    fn restores_go_into_a_new_folder_of_the_home_folder_and_tests_compare_the_sha256() {
        let home = tempfile::tempdir().unwrap();
        let existing = home.path().join("there");
        fs::create_dir(&existing).unwrap();
        assert!(check_target(home.path(), &existing).is_err());
        assert!(check_target(home.path(), Path::new("relative")).is_err());
        assert!(check_target(home.path(), Path::new("/tmp/../etc/new")).is_err());
        assert!(check_target(home.path(), Path::new("/usr/new-folder")).is_err());
        let target = check_target(home.path(), &home.path().join("restored")).unwrap();
        let planned =
            plan_restore(&config(), "dddddddd", &target, Some("/home/me/notes.txt")).unwrap();
        let Step::Shell { program, .. } = &planned.steps[0] else {
            panic!()
        };
        assert!(program.starts_with("mkdir -- "));
        assert!(
            program.contains("'--overwrite' 'never' '--verify' '--include' '/home/me/notes.txt'")
        );
        assert!(plan_restore(&config(), "zz", &target, None).is_err());

        let state = home.path().join("state");
        let file = home.path().join("notes.txt");
        fs::write(&file, "hello\n").unwrap();
        let recorded = record(&state, home.path(), &file, 1_800_000_000).unwrap();
        assert_eq!(
            recorded.sha256,
            "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03"
        );
        let again = record(&state, home.path(), &file, 1_800_000_000).unwrap();
        assert_ne!(again.id, recorded.id);
        assert_eq!(baselines(&state).len(), 2);
        assert_eq!(find_baseline(&baselines(&state), "2"), Some(&recorded));
        assert!(record(&state, home.path(), Path::new("/etc/hostname"), 0).is_err());
        let test = plan_test(&config(), &recorded, "latest").unwrap();
        let Step::Shell { program, .. } = &test.steps[0] else {
            panic!()
        };
        assert!(program.contains("'dump' 'latest'"));
        assert!(program.contains("PASS: "));
        assert!(program.contains(&recorded.sha256));
    }

    #[test]
    fn the_timer_is_written_and_removed_only_as_helix_toolboxs_own() {
        let units = tempfile::tempdir().unwrap();
        let systemctl = |_: &[&str]| Some(true);
        let mut from_environment = config();
        from_environment.password = Password::Environment;
        assert!(
            timer_on(
                units.path(),
                Path::new("/usr/bin/helix-toolbox"),
                &from_environment,
                Every::Day,
                &systemctl
            )
            .is_err()
        );
        timer_on(
            units.path(),
            Path::new("/usr/bin/helix-toolbox"),
            &config(),
            Every::Week,
            &systemctl,
        )
        .unwrap();
        assert_eq!(timer(units.path()), Some(Every::Week));
        assert!(
            fs::read_to_string(units.path().join(SERVICE))
                .unwrap()
                .contains("ExecStart=\"/usr/bin/helix-toolbox\" restic backup --yes")
        );
        timer_off(units.path(), &systemctl).unwrap();
        assert_eq!(timer(units.path()), None);
        assert!(!units.path().join(TIMER).exists());
        // Someone else's unit of the same name is left alone.
        fs::write(units.path().join(TIMER), "[Timer]\nOnCalendar=hourly\n").unwrap();
        assert!(
            timer_on(
                units.path(),
                Path::new("/usr/bin/helix-toolbox"),
                &config(),
                Every::Day,
                &systemctl
            )
            .is_err()
        );
        assert!(timer_off(units.path(), &systemctl).is_err());
        assert!(units.path().join(TIMER).exists());
    }
}
