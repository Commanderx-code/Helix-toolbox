//! The Git repositories dashboard: the repositories under the folders the
//! user keeps them in, each with its branch, changes and how it stands
//! against its upstream, and one repository's files, diffs, branches,
//! stashes and recent commits. Reading never changes a repository: every
//! probe runs with optional locks off, no hooks, no file system monitor and
//! the repository's own content filters disabled, within a time and size
//! limit. Staging, committing, branches, stashes and syncing are plans,
//! reviewed first; each checks at run time that the repository is still as
//! the review showed it.

use crate::{Plan, Step, quote};
use helix_toolbox_core::Risk;
use serde::Serialize;
use std::{
    collections::HashSet,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The folders searched when nothing says otherwise, those that exist.
pub const DEFAULT_ROOTS: &[&str] = &["~/github", "~/projects", "~/src", "~/code", "~/dotfiles"];
/// How deep under a folder a repository is looked for, by default.
pub const DEFAULT_DEPTH: usize = 3;
/// Folders never searched: they are large and hold no repositories of the
/// user's own.
const SKIPPED: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".cache",
    ".command-center-backups",
];
/// What one probe may print before it is cut off.
const OUTPUT_CAP: usize = 4 << 20;
/// How much of an untracked file is shown as its diff.
const UNTRACKED_CAP: u64 = 256_000;
/// How long one probe may take.
const PROBE_TIME: Duration = Duration::from_secs(15);

/// Where repositories are looked for, and why there.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Places {
    pub roots: Vec<PathBuf>,
    pub depth: usize,
    /// "settings", "Command Center" or "default".
    pub origin: String,
}

/// `~/` expanded; anything not absolute after that is refused.
fn expand(path: &str, home: &Path) -> Option<PathBuf> {
    let path = if path == "~" {
        home.to_path_buf()
    } else if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(path)
    };
    (path.is_absolute() && !path.components().any(|part| part == Component::ParentDir))
        .then_some(path)
}

/// The places from `[repositories]` in `settings.toml` (`roots`, `depth`);
/// when it sets none, those Command Center kept in its own settings, read
/// only; else the default folders that exist.
pub fn places(settings: &Path, home: &Path) -> Places {
    let table = std::fs::read_to_string(settings)
        .ok()
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("repositories")?.as_table().cloned());
    let depth_of = |value: Option<i64>| {
        value
            .and_then(|depth| usize::try_from(depth).ok())
            .filter(|depth| (1..=6).contains(depth))
    };
    let own_depth = depth_of(
        table
            .as_ref()
            .and_then(|table| table.get("depth")?.as_integer()),
    );
    if let Some(roots) = table
        .as_ref()
        .and_then(|table| table.get("roots")?.as_array().cloned())
    {
        return Places {
            roots: roots
                .iter()
                .filter_map(|root| expand(root.as_str()?, home))
                .take(32)
                .collect(),
            depth: own_depth.unwrap_or(DEFAULT_DEPTH),
            origin: "settings".to_string(),
        };
    }
    let command_center = home.join(".config/io.helixstack.commandcenter/settings.json");
    if let Some(value) = std::fs::read_to_string(command_center)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        && let Some(roots) = value.get("roots").and_then(|roots| roots.as_array())
    {
        return Places {
            roots: roots
                .iter()
                .filter_map(|root| expand(root.as_str()?, home))
                .take(32)
                .collect(),
            depth: own_depth
                .or_else(|| depth_of(value.get("scanDepth").and_then(|depth| depth.as_i64())))
                .unwrap_or(DEFAULT_DEPTH),
            origin: "Command Center".to_string(),
        };
    }
    Places {
        roots: DEFAULT_ROOTS
            .iter()
            .filter_map(|root| expand(root, home))
            .filter(|root| root.is_dir())
            .collect(),
        depth: own_depth.unwrap_or(DEFAULT_DEPTH),
        origin: "default".to_string(),
    }
}

/// The repositories under the places: each folder holding `.git`, without
/// following links or looking inside a repository for more. Sorted by name.
pub fn discover(places: &Places) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    for root in &places.roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        walk(&root, places.depth, &mut found, &mut seen);
    }
    found.sort_by_key(|path| {
        (
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
                .unwrap_or_default(),
            path.clone(),
        )
    });
    found
}

fn walk(folder: &Path, depth: usize, found: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    if std::fs::symlink_metadata(folder.join(".git")).is_ok() {
        if seen.insert(folder.to_path_buf()) {
            found.push(folder.to_path_buf());
        }
        return;
    }
    if depth == 0 {
        return;
    }
    let Ok(listing) = std::fs::read_dir(folder) else {
        return;
    };
    let mut children: Vec<PathBuf> = listing
        .flatten()
        .filter(|item| item.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|item| !SKIPPED.contains(&item.file_name().to_string_lossy().as_ref()))
        .map(|item| item.path())
        .collect();
    children.sort();
    for child in children {
        walk(&child, depth - 1, found, seen);
    }
}

/// Runs git for the probes, so tests can stand in for it.
pub trait Git: Sync {
    /// git with these arguments and variables; its success and standard
    /// output, cut off at `OUTPUT_CAP` (the flag says so). None when it
    /// could not run or took too long.
    fn run(&self, args: &[String], environment: &[(String, String)]) -> Option<Output>;

    /// Whether this git takes settings from `GIT_CONFIG_COUNT` (2.31 and
    /// later), as the probes need to empty the content filters.
    fn takes_settings_from_environment(&self) -> bool {
        settings_from_environment(self)
    }
}

/// Whether git says it is 2.31 or later, and gives back a value set only
/// through the environment: a value of this run's own, asked outside any
/// repository with no system or global settings read, so no settings file
/// can answer for it.
fn settings_from_environment(git: &(impl Git + ?Sized)) -> bool {
    let recent = git
        .run(&["--version".into()], &[])
        .filter(|output| output.success)
        .and_then(|output| version(&String::from_utf8_lossy(&output.stdout)))
        .is_some_and(|found| found >= (2, 31));
    if !recent {
        return false;
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or_default();
    let value = format!("{}-{nanos}", std::process::id());
    let key = "helix-toolbox.probe";
    git.run(
        &[
            "-C".into(),
            "/".into(),
            "config".into(),
            "--get".into(),
            key.into(),
        ],
        &[
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
            ("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()),
            ("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()),
            ("GIT_CONFIG_COUNT".into(), "1".into()),
            ("GIT_CONFIG_KEY_0".into(), key.into()),
            ("GIT_CONFIG_VALUE_0".into(), value.clone()),
        ],
    )
    .is_some_and(|output| output.success && output.stdout.trim_ascii() == value.as_bytes())
}

/// The major and minor version from `git --version`.
fn version(text: &str) -> Option<(u32, u32)> {
    let number = text.trim().strip_prefix("git version ")?;
    let mut parts = number.split(|character: char| !character.is_ascii_digit());
    Some((parts.next()?.parse().ok()?, parts.next()?.parse().ok()?))
}

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub success: bool,
    pub stdout: Vec<u8>,
    pub truncated: bool,
}

/// The real git, read-only: no prompts, no optional locks, no lazy fetches
/// and no protocols, in the C locale, within `PROBE_TIME`.
pub struct Here;

impl Git for Here {
    /// Asked once: git does not change while Helix Toolbox runs.
    fn takes_settings_from_environment(&self) -> bool {
        static TAKES: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *TAKES.get_or_init(|| settings_from_environment(self))
    }

    fn run(&self, args: &[String], environment: &[(String, String)]) -> Option<Output> {
        let mut child = Command::new("git")
            .args(args)
            .envs(environment.iter().map(|(key, value)| (key, value)))
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("GIT_NO_LAZY_FETCH", "1")
            .env("GIT_ALLOW_PROTOCOL", "")
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdout = child.stdout.take()?;
        let reader = std::thread::spawn(move || {
            let mut kept = Vec::new();
            let mut buffer = [0u8; 65536];
            let mut truncated = false;
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(count) => {
                        if kept.len() < OUTPUT_CAP {
                            let room = OUTPUT_CAP - kept.len();
                            kept.extend_from_slice(&buffer[..count.min(room)]);
                            truncated |= count > room;
                        } else {
                            truncated = true;
                        }
                    }
                }
            }
            (kept, truncated)
        });
        let deadline = Instant::now() + PROBE_TIME;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10))
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
            }
        };
        let (stdout, truncated) = reader.join().ok()?;
        Some(Output {
            success: status.success(),
            stdout,
            truncated,
        })
    }
}

/// One repository as the probes see it: git run in it with the hardening
/// every probe gets, the repository's content filters included.
struct Probe<'a> {
    git: &'a dyn Git,
    base: Vec<String>,
    /// Settings given through the environment, whose keys may hold any
    /// character: `-c` splits a key from its value at the first `=`.
    settings: Vec<(String, String)>,
}

impl<'a> Probe<'a> {
    /// The probe, once the repository's content filters are known and
    /// emptied; none when they cannot be known, so nothing that could run
    /// one is asked.
    fn new(git: &'a dyn Git, repo: &Path) -> Result<Self, String> {
        if !git.takes_settings_from_environment() {
            return Err("this git is too old to read repositories safely; Helix Toolbox needs git 2.31 or later".to_string());
        }
        let mut base: Vec<String> = vec!["-C".into(), repo.to_string_lossy().into_owned()];
        for setting in [
            "core.fsmonitor=false",
            "core.hooksPath=/dev/null",
            "log.showSignature=false",
            "diff.submodule=short",
            "diff.ignoreSubmodules=dirty",
            "submodule.recurse=false",
        ] {
            base.push("-c".into());
            base.push(setting.into());
        }
        let mut probe = Probe {
            git,
            base,
            settings: Vec::new(),
        };
        // A content filter runs a program of the repository's choosing on
        // status and diff: each one the configuration names is emptied.
        let listed = probe
            .output(&["config", "--includes", "--null", "--name-only", "--list"])
            .filter(|listed| listed.success && !listed.truncated && !listed.stdout.is_empty())
            .ok_or("its settings cannot be read, so it is not looked into")?;
        // A name that is not text, or holds a line break, could not be named
        // back to git exactly, so its filter would stay live.
        let mut filters: Vec<String> = Vec::new();
        for name in listed.stdout.split(|byte| *byte == 0) {
            let Some(rest) = name.strip_prefix(b"filter.") else {
                continue;
            };
            let rest = std::str::from_utf8(rest)
                .ok()
                .filter(|rest| !rest.contains(['\n', '\r']))
                .ok_or(
                    "it names a content filter that cannot be turned off, so it is not looked into",
                )?;
            if let Some((name, _)) = rest.rsplit_once('.') {
                filters.push(name.to_string());
            }
        }
        filters.sort();
        filters.dedup();
        for name in filters {
            for (key, value) in [
                ("clean", ""),
                ("smudge", ""),
                ("process", ""),
                ("required", "false"),
            ] {
                probe
                    .settings
                    .push((format!("filter.{name}.{key}"), value.to_string()));
            }
        }
        Ok(probe)
    }

    fn output(&self, args: &[&str]) -> Option<Output> {
        let mut all = self.base.clone();
        all.extend(args.iter().map(|arg| arg.to_string()));
        let mut environment = vec![(
            "GIT_CONFIG_COUNT".to_string(),
            self.settings.len().to_string(),
        )];
        for (index, (key, value)) in self.settings.iter().enumerate() {
            environment.push((format!("GIT_CONFIG_KEY_{index}"), key.clone()));
            environment.push((format!("GIT_CONFIG_VALUE_{index}"), value.clone()));
        }
        self.git.run(&all, &environment)
    }

    /// Standard output when git succeeded.
    fn text(&self, args: &[&str]) -> Option<String> {
        let output = self.output(args)?;
        output
            .success
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
    }

    fn line(&self, args: &[&str]) -> Option<String> {
        self.text(args)
            .map(|text| text.trim().to_string())
            .filter(|text| !text.is_empty())
    }
}

/// One repository in the list.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Summary {
    pub name: String,
    pub path: PathBuf,
    /// None when HEAD is detached.
    pub branch: Option<String>,
    /// Files with changes, untracked ones included.
    pub changed: usize,
    pub upstream: Option<String>,
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// The latest commit's subject and time, in seconds since 1970.
    pub last: Option<String>,
    pub last_at: Option<u64>,
    /// `origin`'s address, without credentials.
    pub origin: Option<String>,
    /// Why it could not be read, when it could not.
    pub problem: Option<String>,
}

/// An address with any user name and password taken out.
fn without_credentials(address: &str) -> String {
    match address.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((host, path)) => format!(
                "{scheme}://{}/{path}",
                host.rsplit_once('@').map(|(_, host)| host).unwrap_or(host)
            ),
            None => format!(
                "{scheme}://{}",
                rest.rsplit_once('@').map(|(_, host)| host).unwrap_or(rest)
            ),
        },
        None => address.to_string(),
    }
}

/// The repository's name, branch, changes, upstream and latest commit.
pub fn summary(git: &dyn Git, repo: &Path) -> Summary {
    let name = repo
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| repo.display().to_string());
    let probe = match Probe::new(git, repo) {
        Ok(probe) => probe,
        Err(problem) => {
            return Summary {
                name,
                path: repo.to_path_buf(),
                problem: Some(problem),
                ..Summary::default()
            };
        }
    };
    let Some(status) = probe.output(&[
        "status",
        "--porcelain=v1",
        "-z",
        "--ignore-submodules=dirty",
    ]) else {
        return Summary {
            name,
            path: repo.to_path_buf(),
            problem: Some("git did not answer in time".to_string()),
            ..Summary::default()
        };
    };
    if !status.success {
        return Summary {
            name,
            path: repo.to_path_buf(),
            problem: Some("git cannot read it".to_string()),
            ..Summary::default()
        };
    }
    let changed = parse_status(&status.stdout).len();
    let upstream = probe.line(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ]);
    let (ahead, behind) = match upstream {
        Some(_) => {
            counts(probe.line(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]))
        }
        None => (None, None),
    };
    let last = probe.line(&["log", "-1", "--format=%s%x1f%ct"]);
    let (last, last_at) = match last.as_deref().and_then(|line| line.split_once('\x1f')) {
        Some((subject, at)) => (Some(subject.to_string()), at.trim().parse().ok()),
        None => (None, None),
    };
    Summary {
        name,
        path: repo.to_path_buf(),
        branch: probe.line(&["branch", "--show-current"]),
        changed,
        upstream,
        ahead,
        behind,
        last,
        last_at,
        origin: probe
            .line(&["remote", "get-url", "origin"])
            .map(|address| without_credentials(&address)),
        problem: None,
    }
}

/// The summaries of many repositories, read a few at a time.
pub fn summaries(git: &dyn Git, repos: &[PathBuf]) -> Vec<Summary> {
    let mut found: Vec<Option<Summary>> = vec![None; repos.len()];
    std::thread::scope(|scope| {
        for (chunk, slots) in repos.chunks(4).zip(found.chunks_mut(4)) {
            scope.spawn(move || {
                for (repo, slot) in chunk.iter().zip(slots.iter_mut()) {
                    *slot = Some(summary(git, repo));
                }
            });
        }
    });
    found.into_iter().flatten().collect()
}

fn counts(line: Option<String>) -> (Option<u32>, Option<u32>) {
    let Some(line) = line else {
        return (None, None);
    };
    let mut parts = line.split_whitespace().map(|part| part.parse::<u32>().ok());
    (parts.next().flatten(), parts.next().flatten())
}

/// A changed file, as `git status --porcelain=v1` says.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct File {
    pub path: String,
    /// The path it had, for a rename or copy.
    pub original: Option<String>,
    /// Its state in the index and in the working tree (` `, `M`, `A`, `D`,
    /// `R`, `C`, `U`, `?`).
    pub staged: char,
    pub unstaged: char,
    pub conflict: bool,
}

impl File {
    pub fn untracked(&self) -> bool {
        self.staged == '?'
    }

    /// Whether anything of it is staged.
    pub fn is_staged(&self) -> bool {
        !matches!(self.staged, ' ' | '?') && !self.conflict
    }

    /// Whether anything of it is not staged.
    pub fn has_unstaged(&self) -> bool {
        self.unstaged != ' ' || self.untracked()
    }

    /// A word for its state.
    pub fn state(&self) -> &'static str {
        if self.conflict {
            return "conflict";
        }
        if self.untracked() {
            return "new";
        }
        let code = if self.staged != ' ' {
            self.staged
        } else {
            self.unstaged
        };
        match code {
            'A' => "added",
            'D' => "deleted",
            'R' => "renamed",
            'C' => "copied",
            'T' => "type changed",
            _ => "modified",
        }
    }
}

/// `git status --porcelain=v1 -z`: `XY path`, then the original path after
/// a rename or copy.
pub fn parse_status(text: &[u8]) -> Vec<File> {
    let mut files = Vec::new();
    let mut parts = text
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty());
    while let Some(part) = parts.next() {
        if part.len() < 4 {
            continue;
        }
        let staged = part[0] as char;
        let unstaged = part[1] as char;
        let path = String::from_utf8_lossy(&part[3..]).into_owned();
        let original = if matches!(staged, 'R' | 'C') || matches!(unstaged, 'R' | 'C') {
            parts
                .next()
                .map(|part| String::from_utf8_lossy(part).into_owned())
        } else {
            None
        };
        let conflict = staged == 'U'
            || unstaged == 'U'
            || (staged == 'A' && unstaged == 'A')
            || (staged == 'D' && unstaged == 'D');
        files.push(File {
            path,
            original,
            staged,
            unstaged,
            conflict,
        });
    }
    files
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Stash {
    /// The stash commit, which identifies it even as others are added.
    pub id: String,
    /// `stash@{n}`.
    pub name: String,
    pub subject: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub at: Option<u64>,
}

/// One repository in full.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Details {
    pub summary: Summary,
    pub files: Vec<File>,
    pub branches: Vec<String>,
    pub remotes: Vec<String>,
    pub stashes: Vec<Stash>,
    pub log: Vec<Commit>,
    /// The commit HEAD names; None before the first commit.
    pub head: Option<String>,
    /// `refs/heads/<branch>`; None when detached.
    pub branch_ref: Option<String>,
    /// The tree the index holds, which a commit would record.
    pub index_tree: Option<String>,
    /// A merge, rebase, cherry-pick or revert under way.
    pub busy: Option<String>,
    /// Where the branch pushes: its remote and the branch there.
    pub push_to: Option<(String, String)>,
    /// What is staged, as `git diff --cached --stat` says.
    pub staged_stat: String,
}

impl Details {
    pub fn conflicts(&self) -> bool {
        self.files.iter().any(|file| file.conflict)
    }

    pub fn clean(&self) -> bool {
        self.files.is_empty()
    }

    pub fn staged(&self) -> usize {
        self.files.iter().filter(|file| file.is_staged()).count()
    }

    pub fn file(&self, path: &str) -> Option<&File> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// What is under way in a git folder, by the files git keeps for it.
fn busy(git_dir: &Path) -> Option<String> {
    [
        ("MERGE_HEAD", "a merge"),
        ("CHERRY_PICK_HEAD", "a cherry-pick"),
        ("REVERT_HEAD", "a revert"),
        ("rebase-merge", "a rebase"),
        ("rebase-apply", "a rebase"),
        ("sequencer", "a cherry-pick or revert"),
    ]
    .into_iter()
    .find(|(file, _)| git_dir.join(file).exists())
    .map(|(_, what)| what.to_string())
}

/// Everything about one repository the dashboard shows.
pub fn details(git: &dyn Git, repo: &Path) -> Result<Details, String> {
    let probe = Probe::new(git, repo)?;
    let status = probe
        .output(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--untracked-files=all",
            "--ignore-submodules=dirty",
        ])
        .ok_or("git did not answer in time")?;
    if !status.success {
        return Err(format!("git cannot read {}", repo.display()));
    }
    if status.truncated {
        return Err("it has too many changed files to show".to_string());
    }
    let files = parse_status(&status.stdout);
    let summary = summary(git, repo);
    let lines = |args: &[&str]| -> Vec<String> {
        probe
            .text(args)
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(String::from)
            .collect()
    };
    let conflicts = files.iter().any(|file| file.conflict);
    let branch_ref = probe.line(&["symbolic-ref", "--quiet", "HEAD"]);
    let push_to = summary.branch.as_deref().and_then(|branch| {
        let remote = probe.line(&["config", "--get", &format!("branch.{branch}.remote")])?;
        let merge = probe.line(&["config", "--get", &format!("branch.{branch}.merge")])?;
        (!remote.starts_with('-') && merge.starts_with("refs/heads/")).then_some((remote, merge))
    });
    Ok(Details {
        branches: lines(&["for-each-ref", "--format=%(refname:strip=2)", "refs/heads/"]),
        remotes: lines(&["remote"]),
        stashes: lines(&["stash", "list", "--format=%H%x1f%gd%x1f%s"])
            .into_iter()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\x1f');
                Some(Stash {
                    id: parts.next()?.to_string(),
                    name: parts.next()?.to_string(),
                    subject: parts.next().unwrap_or_default().to_string(),
                })
            })
            .collect(),
        log: lines(&["log", "-15", "--format=%h%x1f%s%x1f%ct"])
            .into_iter()
            .filter_map(|line| {
                let mut parts = line.splitn(3, '\x1f');
                Some(Commit {
                    hash: parts.next()?.to_string(),
                    subject: parts.next().unwrap_or_default().to_string(),
                    at: parts.next().and_then(|at| at.parse().ok()),
                })
            })
            .collect(),
        head: probe.line(&["rev-parse", "--verify", "--quiet", "HEAD"]),
        index_tree: if conflicts {
            None
        } else {
            probe.line(&["write-tree"])
        },
        busy: probe
            .line(&["rev-parse", "--absolute-git-dir"])
            .and_then(|dir| busy(Path::new(&dir))),
        staged_stat: probe
            .text(&[
                "diff",
                "--cached",
                "--no-ext-diff",
                "--no-textconv",
                "--ignore-submodules=dirty",
                "--stat",
            ])
            .unwrap_or_default(),
        branch_ref,
        push_to,
        files,
        summary,
    })
}

/// A diff to show, and whether it was cut short.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Diff {
    pub text: String,
    pub truncated: bool,
}

/// The diff of the repository, or of one changed file: what is staged, or
/// what is not. An untracked file is shown as it is, up to a limit; a link
/// as where it points.
pub fn diff(
    git: &dyn Git,
    repo: &Path,
    found: &Details,
    file: Option<&str>,
    staged: bool,
) -> Result<Diff, String> {
    let probe = Probe::new(git, repo)?;
    let mut args: Vec<&str> = vec![
        "--literal-pathspecs",
        "diff",
        "--no-ext-diff",
        "--no-textconv",
        "--ignore-submodules=dirty",
        "--no-color",
        "--patch",
    ];
    if staged {
        args.push("--cached");
    }
    if let Some(path) = file {
        let Some(changed) = found.file(path) else {
            return Err(format!("{path} has no changes"));
        };
        if Path::new(path)
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        {
            return Err(format!("{path} is not a path in the repository"));
        }
        if changed.untracked() {
            return Ok(untracked(&repo.join(path)));
        }
        args.push("--");
        args.push(&changed.path);
        if let Some(original) = &changed.original {
            args.push(original);
        }
    }
    let output = probe.output(&args).ok_or("git did not answer in time")?;
    if !output.success {
        return Err("git could not show the diff".to_string());
    }
    Ok(Diff {
        text: String::from_utf8_lossy(&output.stdout).into_owned(),
        truncated: output.truncated,
    })
}

fn untracked(path: &Path) -> Diff {
    if let Ok(target) = std::fs::read_link(path) {
        return Diff {
            text: format!("A new link to {}", target.display()),
            truncated: false,
        };
    }
    let Ok(file) = std::fs::File::open(path) else {
        return Diff {
            text: "It cannot be read.".to_string(),
            truncated: false,
        };
    };
    let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let mut bytes = Vec::new();
    let _ = file.take(UNTRACKED_CAP).read_to_end(&mut bytes);
    if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() && size <= UNTRACKED_CAP {
        return Diff {
            text: "A new binary file.".to_string(),
            truncated: false,
        };
    }
    let text = String::from_utf8_lossy(&bytes);
    Diff {
        text: text.lines().map(|line| format!("+{line}\n")).collect(),
        truncated: size > UNTRACKED_CAP,
    }
}

/// A change to a repository, planned and reviewed before it runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Change {
    Stage(Vec<String>),
    StageAll,
    Unstage(Vec<String>),
    Commit(String),
    CreateBranch(String),
    SwitchBranch(String),
    /// Stash the changes, with a message, untracked files too or not.
    Stash {
        message: String,
        untracked: bool,
    },
    /// Apply a stash by its commit, keeping it.
    ApplyStash(String),
    Fetch,
    Pull,
    Push,
    /// Push the branch to a remote and track it there.
    Publish(String),
}

/// Whether git takes the name for a branch.
pub fn valid_branch(git: &dyn Git, repo: &Path, name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('-')
        && Probe::new(git, repo).is_ok_and(|probe| {
            probe
                .output(&["check-ref-format", &format!("refs/heads/{name}")])
                .is_some_and(|output| output.success)
        })
}

/// The plan for a change, from the repository as it was read; the run
/// checks again what the change depends on.
pub fn plan(git: &dyn Git, found: &Details, change: &Change) -> Result<Plan, String> {
    let repo = &found.summary.path;
    let name = &found.summary.name;
    if let Some(busy) = &found.busy
        && !matches!(
            change,
            Change::Fetch | Change::Stage(_) | Change::StageAll | Change::Unstage(_)
        )
    {
        return Err(format!(
            "{name} is in the middle of {busy}; finish it with git first"
        ));
    }
    let shown_repo = repo.display().to_string();
    // Paths are taken as they are, not as patterns; stash takes no such flag.
    let git_words = |words: &[&str]| -> (String, String) {
        let mut all = vec!["git"];
        if !matches!(words.first(), Some(&"stash")) {
            all.push("--literal-pathspecs");
        }
        all.extend(words);
        (
            all.iter()
                .map(|word| quote(word))
                .collect::<Vec<_>>()
                .join(" "),
            format!("{} {}", all[0], all[1..].join(" ")),
        )
    };
    let paths = |paths: &[String], unstage: bool| -> Result<Vec<String>, String> {
        if paths.is_empty() || paths.len() > 1000 {
            return Err("choose between 1 and 1000 files".to_string());
        }
        let mut words = Vec::new();
        for path in paths {
            let Some(file) = found.file(path) else {
                return Err(format!("{path} has no changes in {name}"));
            };
            if file.conflict {
                return Err(format!("{path} has a conflict; resolve it with git first"));
            }
            if unstage && !file.is_staged() {
                return Err(format!("{path} has nothing staged"));
            }
            words.push(file.path.clone());
            if let Some(original) = &file.original {
                words.push(original.clone());
            }
        }
        Ok(words)
    };
    let mut checks: Vec<String> = Vec::new();
    let mut network = false;
    let (title, (program, display)) = match change {
        Change::Stage(chosen) => {
            let words = paths(chosen, false)?;
            let mut all = vec!["add", "--all", "--"];
            all.extend(words.iter().map(String::as_str));
            (
                format!("Stage {} in {name}", count(chosen.len(), "file")),
                git_words(&all),
            )
        }
        Change::StageAll => {
            if found.clean() {
                return Err(format!("{name} has no changes"));
            }
            if found.conflicts() {
                return Err(format!("{name} has conflicts; resolve them with git first"));
            }
            (
                format!("Stage every change in {name}"),
                git_words(&["add", "--all", "--", "."]),
            )
        }
        Change::Unstage(chosen) => {
            let words = paths(chosen, true)?;
            let mut all: Vec<&str> = if found.head.is_some() {
                vec!["reset", "--quiet", "HEAD", "--"]
            } else {
                vec!["rm", "--cached", "-r", "-f", "--quiet", "--"]
            };
            all.extend(words.iter().map(String::as_str));
            (
                format!("Unstage {} in {name}", count(chosen.len(), "file")),
                git_words(&all),
            )
        }
        Change::Commit(message) => {
            let message = message.trim();
            if message.is_empty() || message.len() > 10_000 || message.contains('\0') {
                return Err("the commit needs a message".to_string());
            }
            if found.conflicts() {
                return Err(format!("{name} has conflicts; resolve them with git first"));
            }
            if found.staged() == 0 {
                return Err(format!("nothing is staged in {name}"));
            }
            let (Some(branch_ref), Some(tree)) = (&found.branch_ref, &found.index_tree) else {
                return Err(format!("{name} is not on a branch; switch to one first"));
            };
            checks.push(expect("git write-tree", tree, "what is staged changed"));
            checks.push(expect(
                "git rev-parse --verify --quiet HEAD",
                found.head.as_deref().unwrap_or(""),
                "another commit was made",
            ));
            checks.push(expect(
                "git symbolic-ref --quiet HEAD",
                branch_ref,
                "the branch changed",
            ));
            checks.push(NOT_BUSY.to_string());
            (
                format!(
                    "Commit {} to {}",
                    count(found.staged(), "staged file"),
                    found.summary.branch.as_deref().unwrap_or(name)
                ),
                git_words(&["commit", "-m", message]),
            )
        }
        Change::CreateBranch(branch) | Change::SwitchBranch(branch) => {
            let create = matches!(change, Change::CreateBranch(_));
            if !valid_branch(git, repo, branch) {
                return Err(format!("{branch:?} is not a name git takes for a branch"));
            }
            let exists = found.branches.iter().any(|other| other == branch);
            if create && exists {
                return Err(format!("{name} already has a branch {branch}"));
            }
            if !create && !exists {
                return Err(format!("{name} has no branch {branch}"));
            }
            if !found.clean() {
                return Err(format!("{name} has changes; commit or stash them first"));
            }
            if found.head.is_none() {
                return Err(format!("{name} has no commit yet"));
            }
            checks.push(CLEAN.to_string());
            checks.push(expect(
                "git rev-parse --verify --quiet HEAD",
                found.head.as_deref().unwrap_or(""),
                "another commit was made",
            ));
            if create {
                (
                    format!("Create the branch {branch} in {name}"),
                    git_words(&["switch", "--no-guess", "-c", branch]),
                )
            } else {
                (
                    format!("Switch {name} to {branch}"),
                    git_words(&["switch", "--no-guess", "--", branch]),
                )
            }
        }
        Change::Stash { message, untracked } => {
            if found.head.is_none() {
                return Err(format!("{name} has no commit yet"));
            }
            if found.conflicts() {
                return Err(format!("{name} has conflicts; resolve them with git first"));
            }
            if found.clean() || (!untracked && found.files.iter().all(File::untracked)) {
                return Err(format!("{name} has no changes to stash"));
            }
            let message = if message.trim().is_empty() {
                "Helix Toolbox stash"
            } else {
                message.trim()
            };
            let mut all = vec!["stash", "push", "--message", message];
            if *untracked {
                all.push("--include-untracked");
            }
            (format!("Stash the changes in {name}"), git_words(&all))
        }
        Change::ApplyStash(id) => {
            let Some(stash) = found
                .stashes
                .iter()
                .find(|stash| &stash.id == id || &stash.name == id)
            else {
                return Err(format!("{name} has no stash {id}"));
            };
            if !found.clean() {
                return Err(format!("{name} has changes; commit or stash them first"));
            }
            checks.push(CLEAN.to_string());
            checks.push(format!(
                "git stash list --format=%H | grep -qx {} || {{ echo 'Helix Toolbox: the stash is gone; look again' >&2; exit 1; }}",
                quote(&stash.id)
            ));
            (
                format!(
                    "Apply {} ({}) to {name}, keeping it",
                    stash.name, stash.subject
                ),
                git_words(&["stash", "apply", "--index", &stash.id]),
            )
        }
        Change::Fetch => {
            network = true;
            if found.remotes.is_empty() {
                return Err(format!("{name} has no remote"));
            }
            (
                format!("Fetch every remote of {name}"),
                git_words(&["fetch", "--all", "--prune"]),
            )
        }
        Change::Pull => {
            network = true;
            if found.summary.branch.is_none() {
                return Err(format!("{name} is not on a branch"));
            }
            if found.summary.upstream.is_none() {
                return Err(format!("{name}'s branch has no upstream; publish it first"));
            }
            if !found.clean() {
                return Err(format!("{name} has changes; commit or stash them first"));
            }
            checks.push(CLEAN.to_string());
            (
                format!(
                    "Pull {} into {name}, fast-forward only",
                    found.summary.upstream.as_deref().unwrap_or("the upstream")
                ),
                git_words(&["pull", "--ff-only", "--no-rebase", "--no-autostash"]),
            )
        }
        Change::Push => {
            network = true;
            let Some((remote, merge)) = &found.push_to else {
                return Err(format!("{name}'s branch has no upstream; publish it first"));
            };
            (
                format!(
                    "Push {} to {remote}",
                    found.summary.branch.as_deref().unwrap_or(name)
                ),
                git_words(&[
                    "-c",
                    "push.followTags=false",
                    "push",
                    "--porcelain",
                    "--no-force",
                    "--",
                    remote,
                    &format!("HEAD:{merge}"),
                ]),
            )
        }
        Change::Publish(remote) => {
            network = true;
            let Some(branch) = &found.summary.branch else {
                return Err(format!("{name} is not on a branch"));
            };
            if remote.starts_with('-') || !found.remotes.contains(remote) {
                return Err(format!("{name} has no remote {remote}"));
            }
            (
                format!("Publish {branch} to {remote}"),
                git_words(&[
                    "-c",
                    "push.followTags=false",
                    "push",
                    "--set-upstream",
                    "--porcelain",
                    "--no-force",
                    "--",
                    remote,
                    &format!("HEAD:refs/heads/{branch}"),
                ]),
            )
        }
    };
    let mut script = format!(
        "set -e\ncd -- {}\nexport GIT_TERMINAL_PROMPT=0\n",
        quote(&shown_repo)
    );
    if network {
        script.push_str("export GIT_SSH_COMMAND='ssh -oBatchMode=yes -oConnectTimeout=15'\n");
    }
    for check in &checks {
        script.push_str(check);
        script.push('\n');
    }
    script.push_str(&program);
    let id = format!(
        "git-{}",
        match change {
            Change::Stage(_) | Change::StageAll => "stage",
            Change::Unstage(_) => "unstage",
            Change::Commit(_) => "commit",
            Change::CreateBranch(_) => "branch",
            Change::SwitchBranch(_) => "switch",
            Change::Stash { .. } => "stash",
            Change::ApplyStash(_) => "stash-apply",
            Change::Fetch => "fetch",
            Change::Pull => "pull",
            Change::Push => "push",
            Change::Publish(_) => "publish",
        }
    );
    let mut notes = vec![format!("In {shown_repo}.")];
    if network {
        notes.push("It asks for no password: a remote that needs one, or an SSH key with a passphrase not in an agent, fails instead.".to_string());
    }
    if matches!(change, Change::Commit(_)) {
        notes
            .push("The repository's hooks and commit signing run as they do with git.".to_string());
    }
    Ok(Plan {
        steps: vec![Step::Shell {
            id,
            name: title,
            program: script,
            display: helix_toolbox_core::printable(&format!("cd {shown_repo} && {display}")),
            requires: vec!["git".to_string()],
            changes: Vec::new(),
        }],
        notes,
        requirements: vec!["git".to_string()],
        privileged: false,
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

/// A run-time check that a command still prints what the review saw.
fn expect(command: &str, value: &str, what: &str) -> String {
    format!(
        "[ \"$({command} 2>/dev/null || true)\" = {} ] || {{ echo 'Helix Toolbox: {what} since the review; look again' >&2; exit 1; }}",
        quote(value)
    )
}

const CLEAN: &str = "[ -z \"$(git status --porcelain)\" ] || { echo 'Helix Toolbox: the repository has changes since the review; look again' >&2; exit 1; }";

const NOT_BUSY: &str = "hf_git=$(git rev-parse --absolute-git-dir)\nfor hf_file in MERGE_HEAD CHERRY_PICK_HEAD REVERT_HEAD rebase-merge rebase-apply sequencer; do\n  [ ! -e \"$hf_git/$hf_file\" ] || { echo 'Helix Toolbox: a merge, rebase, cherry-pick or revert is under way' >&2; exit 1; }\ndone";

fn count(number: usize, what: &str) -> String {
    if number == 1 {
        format!("1 {what}")
    } else {
        format!("{number} {what}s")
    }
}

/// A repository by its path, or its name when only one has it.
pub fn find<'a>(repos: &'a [PathBuf], wanted: &str) -> Result<&'a PathBuf, String> {
    let path = Path::new(wanted);
    if path.is_absolute() || wanted.starts_with('.') || wanted.contains('/') {
        let canonical = std::fs::canonicalize(path).map_err(|_| format!("no folder {wanted}"))?;
        return repos
            .iter()
            .find(|repo| **repo == canonical)
            .ok_or_else(|| {
                format!("{wanted} is not among the repositories found; see helix-toolbox repos")
            });
    }
    let named: Vec<&PathBuf> = repos
        .iter()
        .filter(|repo| repo.file_name().is_some_and(|name| name == wanted))
        .collect();
    match named.as_slice() {
        [one] => Ok(one),
        [] => Err(format!(
            "no repository named {wanted}; see helix-toolbox repos"
        )),
        _ => Err(format!(
            "{} repositories are named {wanted}; give its path: {}",
            named.len(),
            named
                .iter()
                .map(|repo| repo.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git_in(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args([
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "-c",
                "commit.gpgsign=false",
                "-c",
                "init.defaultBranch=main",
            ])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    fn repository() -> tempfile::TempDir {
        let folder = tempfile::tempdir().unwrap();
        git_in(folder.path(), &["init", "-q"]);
        std::fs::write(folder.path().join("one.txt"), "one\n").unwrap();
        git_in(folder.path(), &["add", "one.txt"]);
        git_in(folder.path(), &["commit", "-q", "-m", "First"]);
        folder
    }

    /// Run a plan's program as the run would, with the test's identity.
    fn run(plan: &Plan) -> bool {
        let Step::Shell { program, .. } = &plan.steps[0] else {
            panic!("not a shell step")
        };
        Command::new("sh")
            .arg("-c")
            .arg(program)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    #[test]
    fn repositories_are_found_under_the_places_and_not_inside_each_other() {
        let home = tempfile::tempdir().unwrap();
        let projects = home.path().join("projects");
        for repo in [
            "Beta/.git",
            "alpha/.git",
            "deep/a/b/c/.git",
            "node_modules/x/.git",
            "alpha/inner/.git",
        ] {
            std::fs::create_dir_all(projects.join(repo)).unwrap();
        }
        let settings = home.path().join("settings.toml");
        std::fs::write(
            &settings,
            "[repositories]\nroots = [\"~/projects\", \"relative\"]\ndepth = 3\n",
        )
        .unwrap();
        let found = places(&settings, home.path());
        assert_eq!(found.origin, "settings");
        assert_eq!(found.roots, vec![projects.clone()]);
        let names: Vec<String> = discover(&found)
            .iter()
            .map(|repo| {
                repo.strip_prefix(projects.canonicalize().unwrap())
                    .unwrap()
                    .display()
                    .to_string()
            })
            .collect();
        // Too deep, under node_modules, or inside another: left out.
        assert_eq!(names, ["alpha", "Beta"]);
        // Command Center's own settings, when Helix Toolbox has none.
        std::fs::write(&settings, "").unwrap();
        std::fs::create_dir_all(home.path().join(".config/io.helixstack.commandcenter")).unwrap();
        std::fs::write(
            home.path()
                .join(".config/io.helixstack.commandcenter/settings.json"),
            "{\"roots\": [\"~/projects\"], \"scanDepth\": 5}",
        )
        .unwrap();
        let theirs = places(&settings, home.path());
        assert_eq!(
            (theirs.origin.as_str(), theirs.depth),
            ("Command Center", 5)
        );
        assert_eq!(discover(&theirs).len(), 3);
    }

    #[test]
    fn a_repository_is_read_with_its_changes_and_without_running_its_filters() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        // A filter that would leave a mark if git ran it.
        let mark = repo.join("ran");
        git_in(
            &repo,
            &[
                "config",
                "filter.spy.clean",
                &format!("touch {}", mark.display()),
            ],
        );
        std::fs::write(repo.join(".gitattributes"), "*.txt filter=spy\n").unwrap();
        std::fs::write(repo.join("one.txt"), "one, changed\n").unwrap();
        std::fs::write(repo.join("new file.txt"), "new\n").unwrap();
        let found = details(&Here, &repo).unwrap();
        assert!(!mark.exists(), "the repository's filter ran");
        assert_eq!(found.summary.branch.as_deref(), Some("main"));
        assert_eq!(found.summary.last.as_deref(), Some("First"));
        let states: Vec<(&str, &str)> = found
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.state()))
            .collect();
        assert_eq!(
            states,
            [
                ("one.txt", "modified"),
                (".gitattributes", "new"),
                ("new file.txt", "new")
            ]
        );
        let shown = diff(&Here, &repo, &found, Some("one.txt"), false).unwrap();
        assert!(shown.text.contains("+one, changed"), "{}", shown.text);
        assert_eq!(
            diff(&Here, &repo, &found, Some("new file.txt"), false)
                .unwrap()
                .text,
            "+new\n"
        );
        assert!(diff(&Here, &repo, &found, Some("../outside"), false).is_err());
        assert!(!mark.exists(), "the repository's filter ran");
    }

    #[test]
    fn a_filter_whose_name_holds_an_equals_sign_is_not_run_either() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        let mark = repo.join("ran");
        let touch = format!("touch {}; cat", mark.display());
        // `-c filter.a=b.clean=` would be read as the key filter.a.
        git_in(&repo, &["config", "filter.a=b.clean", &touch]);
        git_in(&repo, &["config", "filter.a=b.smudge", &touch]);
        std::fs::write(repo.join(".gitattributes"), "*.txt filter=a=b\n").unwrap();
        std::fs::write(repo.join("one.txt"), "one, changed\n").unwrap();
        // git as it is runs the filter on a diff, so the test can see one.
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["diff", "--quiet"])
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .unwrap();
        assert!(
            mark.exists(),
            "the filter does not run, so the test proves nothing"
        );
        std::fs::remove_file(&mark).unwrap();
        let listed = summaries(&Here, std::slice::from_ref(&repo));
        assert_eq!(listed[0].problem, None);
        let found = details(&Here, &repo).unwrap();
        assert!(found.file("one.txt").is_some());
        for staged in [false, true] {
            diff(&Here, &repo, &found, None, staged).unwrap();
            diff(&Here, &repo, &found, Some("one.txt"), staged).unwrap();
        }
        assert!(!mark.exists(), "a probe ran the repository's filter");
    }

    /// Stands in for git where the settings cannot be listed.
    struct Unlisted;

    impl Git for Unlisted {
        fn run(&self, args: &[String], environment: &[(String, String)]) -> Option<Output> {
            if args.iter().any(|arg| arg == "config") {
                return Some(Output {
                    success: true,
                    stdout: b"core.bare".to_vec(),
                    truncated: true,
                });
            }
            Here.run(args, environment)
        }

        fn takes_settings_from_environment(&self) -> bool {
            true
        }
    }

    /// Stands in for a git older than 2.31, which ignores GIT_CONFIG_COUNT.
    struct Old;

    impl Git for Old {
        fn run(&self, args: &[String], _: &[(String, String)]) -> Option<Output> {
            Here.run(args, &[])
        }
    }

    /// Stands in for an old git that ignores GIT_CONFIG_COUNT and its other
    /// settings variables, under a settings file that sets the key the check
    /// asks for, and says it is the given version.
    struct Spoofed {
        settings: PathBuf,
        version: &'static str,
    }

    impl Git for Spoofed {
        fn run(&self, args: &[String], _: &[(String, String)]) -> Option<Output> {
            if args == ["--version"] {
                return Some(Output {
                    success: true,
                    stdout: self.version.as_bytes().to_vec(),
                    truncated: false,
                });
            }
            Here.run(
                args,
                &[(
                    "GIT_CONFIG_GLOBAL".to_string(),
                    self.settings.display().to_string(),
                )],
            )
        }
    }

    #[test]
    fn a_settings_file_or_a_version_cannot_answer_for_the_environment() {
        let folder = tempfile::tempdir().unwrap();
        let settings = folder.path().join("gitconfig");
        std::fs::write(&settings, "[helix-toolbox]\n\tprobe = yes\n").unwrap();
        for version in ["git version 2.55.0", "git version 2.30.2", "something else"] {
            let spoofed = Spoofed {
                settings: settings.clone(),
                version,
            };
            assert!(!spoofed.takes_settings_from_environment(), "{version}");
        }
        assert_eq!(version("git version 2.31.0\n"), Some((2, 31)));
        assert_eq!(version("git version 2.39.5 (Apple Git-154)"), Some((2, 39)));
        assert_eq!(version("git version 3.0"), Some((3, 0)));
        assert_eq!(version("hub version 2.14.2"), None);
    }

    #[test]
    fn a_git_that_ignores_settings_from_the_environment_reads_nothing() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        assert!(Here.takes_settings_from_environment());
        assert!(!Old.takes_settings_from_environment());
        let problem = summary(&Old, &repo).problem.unwrap();
        assert!(problem.contains("git 2.31 or later"), "{problem}");
        assert!(details(&Old, &repo).is_err());
    }

    #[test]
    fn a_filter_whose_name_is_not_text_keeps_the_repository_closed() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        let mark = repo.join("ran");
        let mut config = std::fs::read(repo.join(".git/config")).unwrap();
        config.extend_from_slice(b"[filter \"a\xffb\"]\n\tclean = touch ");
        config.extend_from_slice(mark.display().to_string().as_bytes());
        config.extend_from_slice(b"; cat\n");
        std::fs::write(repo.join(".git/config"), config).unwrap();
        let attributes = b"*.txt filter=a\xffb\n".to_vec();
        std::fs::write(repo.join(".gitattributes"), attributes).unwrap();
        std::fs::write(repo.join("one.txt"), "one, changed\n").unwrap();
        let problem = summary(&Here, &repo).problem.unwrap();
        assert!(problem.contains("cannot be turned off"), "{problem}");
        assert!(details(&Here, &repo).is_err());
        assert!(!mark.exists(), "a probe ran the repository's filter");
    }

    #[test]
    fn a_repository_whose_settings_cannot_be_listed_is_not_looked_into() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        assert!(summary(&Unlisted, &repo).problem.is_some());
        assert!(details(&Unlisted, &repo).is_err());
        assert!(summary(&Here, &repo).problem.is_none());
    }

    #[test]
    fn staging_and_a_commit_are_planned_and_checked_again_when_they_run() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        std::fs::write(repo.join("one.txt"), "two\n").unwrap();
        let found = details(&Here, &repo).unwrap();
        assert!(plan(&Here, &found, &Change::Stage(vec!["missing".into()])).is_err());
        assert!(plan(&Here, &found, &Change::Commit("Nothing staged".into())).is_err());
        let staged = plan(&Here, &found, &Change::Stage(vec!["one.txt".into()])).unwrap();
        assert!(
            staged.steps[0]
                .display()
                .ends_with("git --literal-pathspecs add --all -- one.txt")
        );
        assert!(run(&staged));
        let found = details(&Here, &repo).unwrap();
        assert_eq!(found.staged(), 1);
        let commit = plan(&Here, &found, &Change::Commit("Second".into())).unwrap();
        // Something else staged after the review: the commit refuses.
        std::fs::write(repo.join("three.txt"), "3\n").unwrap();
        git_in(&repo, &["add", "three.txt"]);
        assert!(!run(&commit));
        let found = details(&Here, &repo).unwrap();
        let commit = plan(&Here, &found, &Change::Commit("Second".into())).unwrap();
        assert!(run(&commit));
        let found = details(&Here, &repo).unwrap();
        assert!(found.clean());
        assert_eq!(found.log[0].subject, "Second");
    }

    #[test]
    fn branches_stashes_and_a_remote_round_trip() {
        let folder = repository();
        let repo = folder.path().canonicalize().unwrap();
        let found = details(&Here, &repo).unwrap();
        assert!(plan(&Here, &found, &Change::CreateBranch("bad..name".into())).is_err());
        assert!(run(&plan(
            &Here,
            &found,
            &Change::CreateBranch("topic".into())
        )
        .unwrap()));
        let found = details(&Here, &repo).unwrap();
        assert_eq!(found.summary.branch.as_deref(), Some("topic"));
        std::fs::write(repo.join("one.txt"), "stashed\n").unwrap();
        let found = details(&Here, &repo).unwrap();
        assert!(plan(&Here, &found, &Change::SwitchBranch("main".into())).is_err());
        assert!(run(&plan(
            &Here,
            &found,
            &Change::Stash {
                message: "Work".into(),
                untracked: false
            }
        )
        .unwrap()));
        let found = details(&Here, &repo).unwrap();
        assert!(found.clean());
        assert_eq!(found.stashes.len(), 1);
        assert!(run(&plan(
            &Here,
            &found,
            &Change::SwitchBranch("main".into())
        )
        .unwrap()));
        let found = details(&Here, &repo).unwrap();
        let id = found.stashes[0].id.clone();
        assert!(run(&plan(&Here, &found, &Change::ApplyStash(id)).unwrap()));
        assert_eq!(
            std::fs::read_to_string(repo.join("one.txt")).unwrap(),
            "stashed\n"
        );
        assert_eq!(
            details(&Here, &repo).unwrap().stashes.len(),
            1,
            "the stash is kept"
        );
        // A bare remote: published, then pushed to and pulled from.
        git_in(&repo, &["checkout", "-q", "--", "one.txt"]);
        let remote = tempfile::tempdir().unwrap();
        git_in(remote.path(), &["init", "-q", "--bare"]);
        git_in(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                &remote.path().display().to_string(),
            ],
        );
        let found = details(&Here, &repo).unwrap();
        assert!(
            plan(&Here, &found, &Change::Push).is_err(),
            "no upstream yet"
        );
        assert!(run(
            &plan(&Here, &found, &Change::Publish("origin".into())).unwrap()
        ));
        let found = details(&Here, &repo).unwrap();
        assert_eq!(found.summary.upstream.as_deref(), Some("origin/main"));
        assert!(run(&plan(&Here, &found, &Change::Fetch).unwrap()));
        assert!(run(&plan(&Here, &found, &Change::Pull).unwrap()));
        assert!(run(&plan(&Here, &found, &Change::Push).unwrap()));
    }

    #[test]
    fn porcelain_status_names_renames_and_conflicts() {
        let files = parse_status(b"R  new.txt\0old.txt\0UU both.txt\0?? fresh\0 M plain\0");
        assert_eq!(files[0].original.as_deref(), Some("old.txt"));
        assert_eq!(files[0].state(), "renamed");
        assert!(files[1].conflict);
        assert!(files[2].untracked() && !files[2].is_staged());
        assert!(files[3].has_unstaged() && !files[3].is_staged());
        assert_eq!(
            without_credentials("https://user:secret@github.com/owner/repo.git"),
            "https://github.com/owner/repo.git"
        );
        assert_eq!(
            without_credentials("git@github.com:owner/repo.git"),
            "git@github.com:owner/repo.git"
        );
    }
}
