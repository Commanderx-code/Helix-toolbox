//! Ghostty's and Fastfetch's configuration, edited in place: which file is
//! the one to change (the settings' `[configs]`, Command Center's, the
//! dotfiles a Home Manager link comes from, else the live file), its
//! contents with a revision, checks before saving (Ghostty's own
//! `+validate-config`; Fastfetch's JSONC read here), a few common fields
//! changed without editing text, and saving as a plan that keeps the
//! current file as a configuration backup beside it and refuses to write
//! over a file that changed since it was read.

use crate::{Plan, ShellChange, Step, backups, quote};
use helix_toolbox_core::{Risk, printable};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::Read,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The largest file edited here.
pub const LIMIT: usize = 512_000;

/// The revision of a file that is not there yet.
pub const ABSENT: &str = "absent";

/// The configurations edited here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Ghostty,
    Fastfetch,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Ghostty, Kind::Fastfetch];

    pub fn id(self) -> &'static str {
        match self {
            Kind::Ghostty => "ghostty",
            Kind::Fastfetch => "fastfetch",
        }
    }

    pub fn from_id(id: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Ghostty => "Ghostty",
            Kind::Fastfetch => "Fastfetch",
        }
    }

    /// The key naming its source under `[configs]` in settings.toml.
    pub fn setting(self) -> &'static str {
        match self {
            Kind::Ghostty => "ghostty_source",
            Kind::Fastfetch => "fastfetch_source",
        }
    }

    fn command_center_key(self) -> &'static str {
        match self {
            Kind::Ghostty => "ghosttySource",
            Kind::Fastfetch => "fastfetchSource",
        }
    }

    /// Where a dotfiles folder keeps it, as Command Center looked for it.
    fn in_dotfiles(self) -> &'static str {
        match self {
            Kind::Ghostty => "configs/ghostty/config",
            Kind::Fastfetch => "configs/fastfetch/config.jsonc",
        }
    }

    /// The fields changed without editing text.
    pub fn fields(self) -> &'static [Field] {
        match self {
            Kind::Ghostty => &GHOSTTY_FIELDS,
            Kind::Fastfetch => &FASTFETCH_FIELDS,
        }
    }
}

/// A field changed without editing text.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    /// What a value looks like.
    pub hint: &'static str,
}

const GHOSTTY_FIELDS: [Field; 8] = [
    Field {
        key: "font-family",
        label: "Font",
        hint: "a font's name, such as JetBrains Mono; the first font-family, others stay as fallbacks",
    },
    Field {
        key: "font-size",
        label: "Font size",
        hint: "points, such as 12",
    },
    Field {
        key: "theme",
        label: "Theme",
        hint: "a theme's name; ghostty +list-themes lists them",
    },
    Field {
        key: "window-padding-x",
        label: "Padding, left and right",
        hint: "pixels, such as 8",
    },
    Field {
        key: "window-padding-y",
        label: "Padding, top and bottom",
        hint: "pixels, such as 8",
    },
    Field {
        key: "background-opacity",
        label: "Background opacity",
        hint: "from 0 to 1, such as 0.9",
    },
    Field {
        key: "cursor-style",
        label: "Cursor",
        hint: "block, bar, underline or block_hollow",
    },
    Field {
        key: "cursor-style-blink",
        label: "Cursor blinks",
        hint: "true or false",
    },
];

const FASTFETCH_FIELDS: [Field; 3] = [
    Field {
        key: "logo.source",
        label: "Logo",
        hint: "a distribution's name, such as arch, or a file's path",
    },
    Field {
        key: "logo.type",
        label: "Logo type",
        hint: "auto, builtin, small, file, kitty, sixel, chafa or none",
    },
    Field {
        key: "modules",
        label: "Modules",
        hint: "their names in order, separated by commas, such as title, os, kernel, memory",
    },
];

/// Where the file to edit was named.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Origin {
    /// `[configs]` in settings.toml.
    Settings,
    /// Command Center's settings.
    CommandCenter,
    /// A dotfiles folder, since Home Manager links the live file.
    Dotfiles,
    /// The live file itself.
    Live,
}

impl Origin {
    pub fn label(self) -> &'static str {
        match self {
            Origin::Settings => "settings.toml",
            Origin::CommandCenter => "Command Center's settings",
            Origin::Dotfiles => "your dotfiles",
            Origin::Live => "the live file",
        }
    }
}

/// The file to edit, checked.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Located {
    pub kind: Kind,
    /// The file edited, with every link resolved.
    pub path: PathBuf,
    /// Where the program reads it.
    pub live: PathBuf,
    pub origin: Origin,
    /// The live file is a Home Manager link into the Nix store.
    pub managed: bool,
    /// The Home Manager flake the file belongs to: run Home Manager after
    /// saving to make it live.
    pub flake: Option<PathBuf>,
}

/// The facts a file is located from, so tests can supply them.
pub struct Facts<'a> {
    pub home: &'a Path,
    /// `$XDG_CONFIG_HOME`, or `~/.config`.
    pub config_home: &'a Path,
    /// The `[configs]` table of settings.toml, when there is one.
    pub settings: Option<&'a toml::Table>,
    /// Command Center's `settings.json`, when there is one.
    pub command_center: Option<&'a str>,
    /// Where Nix keeps what Home Manager links to.
    pub store: &'a Path,
}

/// `~/` made the home folder.
fn expand(home: &Path, value: &str) -> PathBuf {
    match value.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None if value == "~" => home.to_path_buf(),
        None => PathBuf::from(value),
    }
}

/// A path with every link resolved, as far as it exists; the rest added
/// as it is.
fn resolved(path: &Path) -> PathBuf {
    let mut rest: Vec<std::ffi::OsString> = Vec::new();
    let mut existing = path.to_path_buf();
    loop {
        if let Ok(real) = existing.canonicalize() {
            let mut real = real;
            for part in rest.iter().rev() {
                real.push(part);
            }
            return real;
        }
        match (existing.file_name(), existing.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name.to_os_string());
                existing = parent.to_path_buf();
            }
            _ => return path.to_path_buf(),
        }
    }
}

/// The file to edit for `kind`, or why there is none.
pub fn locate(kind: Kind, facts: &Facts) -> Result<Located, String> {
    let live = match kind {
        // Ghostty 1.2 reads config.ghostty, and config when that is missing.
        Kind::Ghostty if facts.config_home.join("ghostty/config.ghostty").exists() => {
            facts.config_home.join("ghostty/config.ghostty")
        }
        Kind::Ghostty => facts.config_home.join("ghostty/config"),
        Kind::Fastfetch => facts.config_home.join("fastfetch/config.jsonc"),
    };
    let managed = resolved(&live).starts_with(facts.store);
    let setting = facts
        .settings
        .and_then(|table| table.get(kind.setting())?.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let command_center: Option<serde_json::Value> = facts
        .command_center
        .and_then(|text| serde_json::from_str(text).ok());
    let center = |key: &str| -> Option<String> {
        command_center
            .as_ref()?
            .get("integrations")?
            .get(key)?
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(String::from)
    };
    let (path, origin) = if let Some(value) = setting {
        (expand(facts.home, value), Origin::Settings)
    } else if let Some(value) = center(kind.command_center_key()) {
        (expand(facts.home, &value), Origin::CommandCenter)
    } else if managed {
        // The file the link comes from, in a dotfiles folder.
        let folders = [
            center("dotfilesPath").map(|value| expand(facts.home, &value)),
            Some(facts.home.join("dotfiles")),
            Some(facts.home.join("github/projects/dotfiles")),
            Some(facts.home.join("github/projects/Commander-os")),
        ];
        match folders
            .into_iter()
            .flatten()
            .map(|folder| folder.join(kind.in_dotfiles()))
            .find(|file| file.is_file())
        {
            Some(file) => (file, Origin::Dotfiles),
            None => {
                return Err(format!(
                    "{} is linked by Home Manager into the Nix store, so it cannot be edited there; set {} under [configs] in settings.toml to the file in your dotfiles it comes from",
                    live.display(),
                    kind.setting()
                ));
            }
        }
    } else {
        (live.clone(), Origin::Live)
    };
    let path = check(&path, facts)?;
    Ok(Located {
        kind,
        flake: flake(&path, facts.home),
        path,
        live,
        origin,
        managed,
    })
}

/// The path made real and checked: a full path without `..`, not in the
/// Nix store, inside the home folder, and a file when it is there.
fn check(path: &Path, facts: &Facts) -> Result<PathBuf, String> {
    let shown = path.display();
    if !path.is_absolute() {
        return Err(format!("{shown} is not a full path"));
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(format!("{shown} goes up a folder with .."));
    }
    let real = resolved(path);
    if real.starts_with(facts.store) {
        return Err(format!(
            "{shown} is managed by Home Manager (it is in the Nix store); name its source in your dotfiles under [configs] in settings.toml"
        ));
    }
    // The configuration folder is the user's own too, also where it is
    // set outside the home folder.
    let own = [facts.home, facts.config_home].map(|folder| {
        folder
            .canonicalize()
            .unwrap_or_else(|_| folder.to_path_buf())
    });
    if !own.iter().any(|folder| real.starts_with(folder)) {
        return Err(format!("{shown} is outside your home folder"));
    }
    if real.exists() {
        let metadata = fs::metadata(&real).map_err(|error| format!("{shown}: {error}"))?;
        if !metadata.is_file() {
            return Err(format!("{shown} is not a file"));
        }
        if metadata.len() > LIMIT as u64 {
            return Err(format!("{shown} is larger than {} KB", LIMIT / 1000));
        }
    }
    Ok(real)
}

/// The folder of the Home Manager flake a file is in, looking up to the
/// home folder.
fn flake(path: &Path, home: &Path) -> Option<PathBuf> {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut folder = path.parent()?;
    while folder.starts_with(&home) && folder != home {
        for candidate in [
            folder.join("flake.nix"),
            folder.join("home-manager/flake.nix"),
        ] {
            if candidate.is_file() {
                return candidate.parent().map(Path::to_path_buf);
            }
        }
        folder = folder.parent()?;
    }
    None
}

fn config_home() -> Option<PathBuf> {
    env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

/// The file to edit on this machine.
pub fn here(kind: Kind) -> Result<Located, String> {
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("there is no home folder")?;
    let config = config_home().ok_or("there is no configuration folder")?;
    let table = crate::settings::path()
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| text.parse::<toml::Table>().ok())
        .and_then(|table| table.get("configs")?.as_table().cloned());
    let command_center =
        crate::restic::command_center_settings().and_then(|path| fs::read_to_string(path).ok());
    locate(
        kind,
        &Facts {
            home: &home,
            config_home: &config,
            settings: table.as_ref(),
            command_center: command_center.as_deref(),
            store: Path::new("/nix/store"),
        },
    )
}

/// The folders the edited files are in, so their backups are found with
/// the others.
pub fn backup_folders() -> Vec<PathBuf> {
    Kind::ALL
        .into_iter()
        .filter_map(|kind| here(kind).ok())
        .filter_map(|located| located.path.parent().map(Path::to_path_buf))
        .collect()
}

/// A file's contents and revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Loaded {
    pub content: String,
    /// The SHA-256 of its bytes, or `absent`.
    pub revision: String,
    pub exists: bool,
}

pub fn revision(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Read the file; a missing one is empty.
pub fn read(located: &Located) -> Result<Loaded, String> {
    let bytes = match fs::read(&located.path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Loaded {
                content: String::new(),
                revision: ABSENT.to_string(),
                exists: false,
            });
        }
        Err(error) => return Err(format!("{}: {error}", located.path.display())),
    };
    if bytes.len() > LIMIT {
        return Err(format!(
            "{} is larger than {} KB",
            located.path.display(),
            LIMIT / 1000
        ));
    }
    let revision = revision(&bytes);
    let content = String::from_utf8(bytes)
        .map_err(|_| format!("{} is not UTF-8 text", located.path.display()))?;
    if content.contains('\0') {
        return Err(format!("{} holds a NUL byte", located.path.display()));
    }
    Ok(Loaded {
        content,
        revision,
        exists: true,
    })
}

/// What the checks say of a file's text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(tag = "state", content = "message", rename_all = "lowercase")]
pub enum Checked {
    Valid,
    /// It could not be checked here, and why.
    Unchecked(String),
    Invalid(String),
}

impl Checked {
    pub fn label(&self) -> &'static str {
        match self {
            Checked::Valid => "Valid",
            Checked::Unchecked(_) => "Not checked",
            Checked::Invalid(_) => "Invalid",
        }
    }
}

/// Ghostty's check of a file: its success and what it printed; None when
/// Ghostty is not installed.
pub type Ghostty<'a> = &'a dyn Fn(&Path) -> Option<(bool, String)>;

/// Check a file's text: its size and bytes, then Fastfetch's JSONC here or
/// Ghostty's own check of a copy in `folder`, a private folder
/// (`private_folder`); without one, Ghostty's is not checked.
pub fn validate(kind: Kind, text: &str, ghostty: Ghostty, folder: Option<&Path>) -> Checked {
    if text.len() > LIMIT {
        return Checked::Invalid(format!("it is larger than {} KB", LIMIT / 1000));
    }
    if text.contains('\0') {
        return Checked::Invalid("it holds a NUL byte".to_string());
    }
    match kind {
        Kind::Fastfetch => match fastfetch_problem(text) {
            Some(why) => Checked::Invalid(why),
            None => Checked::Valid,
        },
        Kind::Ghostty => {
            let Some(folder) = folder else {
                return Checked::Unchecked(
                    "there is no private folder to check a copy in".to_string(),
                );
            };
            if let Err(why) = prepare(folder) {
                return Checked::Unchecked(why);
            }
            let copy = folder.join(format!("ghostty-{}-{}.conf", std::process::id(), nanos()));
            if write_private(&copy, text.as_bytes()).is_err() {
                return Checked::Unchecked(format!("{} cannot be written", copy.display()));
            }
            let answer = ghostty(&copy);
            let _ = fs::remove_file(&copy);
            match answer {
                None => Checked::Unchecked(
                    "Ghostty is not installed here, so it could not be checked".to_string(),
                ),
                Some((true, _)) => Checked::Valid,
                Some((false, said)) => Checked::Invalid(format!(
                    "Ghostty rejected it: {}",
                    printable(said.trim()).replace(&copy.display().to_string(), "the file")
                )),
            }
        }
    }
}

/// `ghostty +validate-config` of a file, within 15 seconds.
pub fn ghostty_here(file: &Path) -> Option<(bool, String)> {
    if !helix_toolbox_platform::binary_exists("ghostty") {
        return None;
    }
    let mut child = Command::new("ghostty")
        .arg("+validate-config")
        .arg(format!("--config-file={}", file.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let started = Instant::now();
    let status = loop {
        if let Ok(Some(status)) = child.try_wait() {
            break status;
        }
        if started.elapsed() > Duration::from_secs(15) {
            let _ = child.kill();
            let _ = child.wait();
            return Some((
                false,
                "Ghostty did not answer within 15 seconds".to_string(),
            ));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut said = String::new();
    if let Some(mut out) = child.stdout.take() {
        let _ = out.read_to_string(&mut said);
    }
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut said);
    }
    Some((status.success(), said))
}

/// Helix Toolbox's own folder for copies of configurations, beside the
/// history.
pub fn private_folder() -> Option<PathBuf> {
    Some(crate::history::state_dir()?.join("configs"))
}

/// A folder for copies made ready: there, a folder of this user's own and
/// not a link (one another user made, or a link, could read or swap the
/// copies), closed to other users, and with copies older than a day
/// removed.
fn prepare(folder: &Path) -> Result<(), String> {
    let shown = folder.display();
    fs::create_dir_all(folder).map_err(|error| format!("{shown}: {error}"))?;
    let metadata = fs::symlink_metadata(folder).map_err(|error| format!("{shown}: {error}"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(format!(
            "{shown} is a link or not a folder, so no copy is made there"
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != nix::unistd::getuid().as_raw() {
            return Err(format!(
                "{shown} belongs to another user, so no copy is made there"
            ));
        }
        fs::set_permissions(folder, fs::Permissions::from_mode(0o700))
            .map_err(|error| format!("{shown}: {error}"))?;
    }
    if let Ok(listing) = fs::read_dir(folder) {
        for item in listing.flatten() {
            let old = item
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > Duration::from_secs(86_400));
            if old {
                let _ = fs::remove_file(item.path());
            }
        }
    }
    Ok(())
}

/// Now, in nanoseconds, to keep copies' names apart.
fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    use std::io::Write;
    options.open(path)?.write_all(bytes)
}

/// A private copy of a file's text in `folder`, for an editor or a plan to
/// read.
pub fn private_copy(
    kind: Kind,
    purpose: &str,
    text: &str,
    folder: &Path,
) -> Result<PathBuf, String> {
    prepare(folder)?;
    let extension = match kind {
        Kind::Ghostty => "conf",
        Kind::Fastfetch => "jsonc",
    };
    let copy = folder.join(format!(
        "{purpose}-{}-{}-{}.{extension}",
        kind.id(),
        std::process::id(),
        nanos()
    ));
    write_private(&copy, text.as_bytes())
        .map_err(|error| format!("{}: {error}", copy.display()))?;
    Ok(copy)
}

/// The current value of each field.
pub fn values(kind: Kind, text: &str) -> Vec<(&'static str, Option<String>)> {
    kind.fields()
        .iter()
        .map(|field| {
            let value = match kind {
                Kind::Ghostty => ghostty_value(text, field.key),
                Kind::Fastfetch => fastfetch_value(text, field.key),
            };
            (field.key, value)
        })
        .collect()
}

/// A field changed: the new text.
pub fn set(kind: Kind, text: &str, key: &str, value: &str) -> Result<String, String> {
    if !kind.fields().iter().any(|field| field.key == key) {
        return Err(format!(
            "{} has no field {key} here; these can be set: {}",
            kind.label(),
            kind.fields()
                .iter()
                .map(|field| field.key)
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let value = value.trim();
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err("the value must be one line of 256 characters at most".to_string());
    }
    match kind {
        Kind::Ghostty => Ok(ghostty_set(text, key, value)),
        Kind::Fastfetch => fastfetch_set(text, key, value),
    }
}

// ----- Ghostty: `key = value` lines -----

/// The lines that set `key`, by index, with their values.
fn ghostty_lines<'a>(text: &'a str, key: &str) -> Vec<(usize, &'a str)> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            let line = line.trim();
            if line.starts_with('#') {
                return None;
            }
            let (name, value) = line.split_once('=')?;
            (name.trim() == key).then(|| (index, value.trim()))
        })
        .collect()
}

/// The value Ghostty uses: the last line's; the first font-family is the
/// font, the others its fallbacks.
fn ghostty_value(text: &str, key: &str) -> Option<String> {
    let lines = ghostty_lines(text, key);
    let found = if key == "font-family" {
        lines.first()
    } else {
        lines.last()
    }?;
    Some(found.1.trim_matches('"').to_string())
}

fn ghostty_set(text: &str, key: &str, value: &str) -> String {
    let lines = ghostty_lines(text, key);
    let target = if key == "font-family" {
        lines.first()
    } else {
        lines.last()
    };
    let written = format!("{key} = {value}");
    match target {
        Some((index, _)) => {
            let mut out: Vec<&str> = text.lines().collect();
            out[*index] = &written;
            let mut joined = out.join("\n");
            if text.ends_with('\n') {
                joined.push('\n');
            }
            joined
        }
        None => {
            let mut joined = text.to_string();
            if !joined.is_empty() && !joined.ends_with('\n') {
                joined.push('\n');
            }
            joined.push_str(&written);
            joined.push('\n');
            joined
        }
    }
}

// ----- Fastfetch: JSONC, read with where each value is -----

/// A value in JSONC text, with where it starts and ends.
#[derive(Clone, Debug)]
enum Node {
    Object {
        start: usize,
        end: usize,
        members: Vec<(String, usize, Node)>,
    },
    Array {
        start: usize,
        end: usize,
        items: Vec<Node>,
    },
    Text {
        start: usize,
        end: usize,
        value: String,
    },
    Other {
        start: usize,
        end: usize,
    },
}

impl Node {
    fn span(&self) -> (usize, usize) {
        match self {
            Node::Object { start, end, .. }
            | Node::Array { start, end, .. }
            | Node::Text { start, end, .. }
            | Node::Other { start, end } => (*start, *end),
        }
    }

    fn member(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Object { members, .. } => members
                .iter()
                .find(|(name, _, _)| name == key)
                .map(|(_, _, node)| node),
            _ => None,
        }
    }

    /// A module's name: the string, or its object's `type`.
    fn module(&self) -> Option<&str> {
        match self {
            Node::Text { value, .. } => Some(value),
            Node::Object { .. } => match self.member("type")? {
                Node::Text { value, .. } => Some(value),
                _ => None,
            },
            _ => None,
        }
    }
}

struct Reader<'a> {
    text: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn fail<T>(&self, what: &str) -> Result<T, String> {
        let line = self.text[..self.at.min(self.text.len())]
            .iter()
            .filter(|byte| **byte == b'\n')
            .count()
            + 1;
        Err(format!("line {line}: {what}"))
    }

    /// Past spaces and comments.
    fn skip(&mut self) -> Result<(), String> {
        loop {
            match self.text.get(self.at..self.at + 2) {
                Some(b"//") => {
                    while self.at < self.text.len() && self.text[self.at] != b'\n' {
                        self.at += 1;
                    }
                }
                Some(b"/*") => {
                    let rest = &self.text[self.at + 2..];
                    match rest.windows(2).position(|pair| pair == b"*/") {
                        Some(end) => self.at += 2 + end + 2,
                        None => return self.fail("a comment is not closed"),
                    }
                }
                _ => match self.text.get(self.at) {
                    Some(byte) if byte.is_ascii_whitespace() => self.at += 1,
                    _ => return Ok(()),
                },
            }
        }
    }

    fn value(&mut self, depth: usize) -> Result<Node, String> {
        if depth > 64 {
            return self.fail("it nests too deep");
        }
        self.skip()?;
        let start = self.at;
        match self.text.get(self.at) {
            None => self.fail("it ends where a value should be"),
            Some(b'{') => {
                self.at += 1;
                let mut members = Vec::new();
                loop {
                    self.skip()?;
                    match self.text.get(self.at) {
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(Node::Object {
                                start,
                                end: self.at,
                                members,
                            });
                        }
                        Some(b'"') => {
                            let key_start = self.at;
                            let key = self.string()?;
                            self.skip()?;
                            if self.text.get(self.at) != Some(&b':') {
                                return self.fail("a key is not followed by :");
                            }
                            self.at += 1;
                            let node = self.value(depth + 1)?;
                            members.push((key, key_start, node));
                            self.skip()?;
                            match self.text.get(self.at) {
                                Some(b',') => self.at += 1,
                                Some(b'}') => {}
                                _ => return self.fail("a , or } is missing in an object"),
                            }
                        }
                        _ => return self.fail("an object's key must be in double quotes"),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                let mut items = Vec::new();
                loop {
                    self.skip()?;
                    if self.text.get(self.at) == Some(&b']') {
                        self.at += 1;
                        return Ok(Node::Array {
                            start,
                            end: self.at,
                            items,
                        });
                    }
                    items.push(self.value(depth + 1)?);
                    self.skip()?;
                    match self.text.get(self.at) {
                        Some(b',') => self.at += 1,
                        Some(b']') => {}
                        _ => return self.fail("a , or ] is missing in a list"),
                    }
                }
            }
            Some(b'"') => {
                let value = self.string()?;
                Ok(Node::Text {
                    start,
                    end: self.at,
                    value,
                })
            }
            Some(_) => {
                while self
                    .text
                    .get(self.at)
                    .is_some_and(|byte| byte.is_ascii_alphanumeric() || b"+-.".contains(byte))
                {
                    self.at += 1;
                }
                let word = std::str::from_utf8(&self.text[start..self.at]).unwrap_or_default();
                let number = word.parse::<f64>().is_ok()
                    && word
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || b"+-.eE".contains(&byte));
                if matches!(word, "true" | "false" | "null") || number {
                    Ok(Node::Other {
                        start,
                        end: self.at,
                    })
                } else {
                    self.fail("a value is not a string, number, list, object, true, false or null")
                }
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        let start = self.at;
        self.at += 1;
        loop {
            match self.text.get(self.at) {
                None | Some(b'\n') => return self.fail("a string is not closed"),
                Some(b'\\') => self.at += 2,
                Some(b'"') => {
                    self.at += 1;
                    let raw = std::str::from_utf8(&self.text[start..self.at])
                        .map_err(|_| "a string is not UTF-8".to_string())?;
                    return serde_json::from_str::<String>(raw)
                        .or_else(|_| self.fail("a string has a bad escape"));
                }
                Some(_) => self.at += 1,
            }
        }
    }
}

fn parse(text: &str) -> Result<Node, String> {
    let mut reader = Reader {
        text: text.as_bytes(),
        at: 0,
    };
    let root = reader.value(0)?;
    reader.skip()?;
    if reader.at != reader.text.len() {
        return reader.fail("there is more after the end");
    }
    Ok(root)
}

/// What is wrong with a Fastfetch configuration, when anything is.
fn fastfetch_problem(text: &str) -> Option<String> {
    if text.trim().is_empty() {
        return None;
    }
    let root = match parse(text) {
        Ok(root) => root,
        Err(why) => return Some(why),
    };
    if !matches!(root, Node::Object { .. }) {
        return Some("it must be one object, { … }".to_string());
    }
    match root.member("modules") {
        None => None,
        Some(Node::Array { items, .. }) => items
            .iter()
            .position(|item| item.module().is_none())
            .map(|index| {
                format!(
                    "module {} is neither a name nor an object with a \"type\"",
                    index + 1
                )
            }),
        Some(_) => Some("\"modules\" must be a list".to_string()),
    }
}

fn fastfetch_value(text: &str, key: &str) -> Option<String> {
    let root = parse(text).ok()?;
    match key {
        "modules" => match root.member("modules")? {
            Node::Array { items, .. } => Some(
                items
                    .iter()
                    .filter_map(Node::module)
                    .collect::<Vec<_>>()
                    .join(", "),
            ),
            _ => None,
        },
        _ => {
            let (_, inner) = key.split_once('.')?;
            match root.member("logo")? {
                // "logo": "arch" names the source.
                Node::Text { value, .. } if inner == "source" => Some(value.clone()),
                logo => match logo.member(inner)? {
                    Node::Text { value, .. } => Some(value.clone()),
                    _ => None,
                },
            }
        }
    }
}

/// The spaces a line starts with, up to a position.
fn indent_at(text: &str, at: usize) -> &str {
    let line = text[..at].rfind('\n').map_or(0, |found| found + 1);
    let rest = &text[line..at];
    &rest[..rest.len() - rest.trim_start().len()]
}

fn replace(text: &str, (start, end): (usize, usize), with: &str) -> String {
    format!("{}{with}{}", &text[..start], &text[end..])
}

/// A member added to an object, in the style of the ones there.
fn insert_member(text: &str, object: &Node, key: &str, value: &str) -> String {
    let Node::Object {
        start,
        end,
        members,
    } = object
    else {
        return text.to_string();
    };
    let member = format!(
        "{}: {value}",
        serde_json::to_string(key).unwrap_or_default()
    );
    match members.first() {
        Some((_, key_start, _)) => {
            let indent = indent_at(text, *key_start);
            let at = *key_start;
            format!("{}{member},\n{indent}{}", &text[..at], &text[at..])
        }
        None => {
            let outer = indent_at(text, *start);
            replace(
                text,
                (*start, *end),
                &format!("{{\n{outer}    {member}\n{outer}}}"),
            )
        }
    }
}

fn fastfetch_set(text: &str, key: &str, value: &str) -> Result<String, String> {
    let text = if text.trim().is_empty() { "{}\n" } else { text };
    if let Some(why) = fastfetch_problem(text) {
        return Err(format!("the file must be valid first: {why}"));
    }
    let root = parse(text)?;
    if key == "modules" {
        let names: Vec<&str> = value
            .split([',', ' '])
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();
        if names.is_empty() {
            return Err("name at least one module".to_string());
        }
        if let Some(bad) = names.iter().find(|name| {
            !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte))
        }) {
            return Err(format!("{bad} is not a module's name"));
        }
        let Some(Node::Array { start, items, .. }) = root.member("modules") else {
            let list = format!(
                "[{}]",
                names
                    .iter()
                    .map(|name| format!("\"{name}\""))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            return Ok(insert_member(text, &root, "modules", &list));
        };
        // The modules kept keep their options; new ones are their names.
        let mut used = vec![false; items.len()];
        let mut written = Vec::new();
        for name in &names {
            let found = items
                .iter()
                .enumerate()
                .find(|(index, item)| !used[*index] && item.module() == Some(name));
            match found {
                Some((index, item)) => {
                    used[index] = true;
                    let (from, to) = item.span();
                    written.push(text[from..to].to_string());
                }
                None => written.push(format!("\"{name}\"")),
            }
        }
        let outer = indent_at(text, *start);
        let inner = items
            .first()
            .map(|item| indent_at(text, item.span().0).to_string())
            .filter(|indent| indent.len() > outer.len())
            .unwrap_or_else(|| format!("{outer}    "));
        let list = format!(
            "[\n{inner}{}\n{outer}]",
            written.join(&format!(",\n{inner}"))
        );
        let span = root.member("modules").map(Node::span).unwrap_or_default();
        return Ok(replace(text, span, &list));
    }
    let inner = key.strip_prefix("logo.").ok_or("not a field")?;
    let encoded = serde_json::to_string(value).unwrap_or_default();
    match root.member("logo") {
        None => Ok(insert_member(
            text,
            &root,
            "logo",
            &format!("{{ \"{inner}\": {encoded} }}"),
        )),
        Some(Node::Text {
            start,
            end,
            value: old,
        }) => Ok(if inner == "source" {
            replace(text, (*start, *end), &encoded)
        } else {
            replace(
                text,
                (*start, *end),
                &format!(
                    "{{ \"source\": {}, \"{inner}\": {encoded} }}",
                    serde_json::to_string(old).unwrap_or_default()
                ),
            )
        }),
        Some(logo @ Node::Object { .. }) => Ok(match logo.member(inner) {
            Some(node) => replace(text, node.span(), &encoded),
            None => insert_member(text, logo, inner, &encoded),
        }),
        Some(_) => Err("\"logo\" is neither a name nor an object".to_string()),
    }
}

// ----- Saving and backups -----

/// The configuration backups kept beside the file, newest first.
pub fn history(located: &Located) -> Vec<backups::Backup> {
    let (Some(folder), Some(name)) = (located.path.parent(), located.path.file_name()) else {
        return Vec::new();
    };
    let name = name.to_string_lossy();
    let mut found: Vec<backups::Backup> = backups::discover(&backups::Roots {
        user: Vec::new(),
        home: None,
        system: Vec::new(),
        beside: vec![folder.to_path_buf()],
    })
    .into_iter()
    .filter(|backup| {
        backup
            .original
            .file_name()
            .map(|found| found.to_string_lossy())
            == Some(name.clone())
            && !backup.directory
    })
    .collect();
    found.sort_by(|a, b| b.taken.cmp(&a.taken));
    found
}

/// The plan that saves new text over the file: run, it checks the file is
/// still as it was read (`revision`), keeps it as a configuration backup
/// beside it, and writes the new text from a private copy in `folder`,
/// keeping the file's permissions. `checked` is what the checks said of the
/// new text.
pub fn plan_save(
    located: &Located,
    revision: &str,
    text: &str,
    checked: &Checked,
    folder: &Path,
) -> Result<Plan, String> {
    if let Checked::Invalid(why) = checked {
        return Err(format!("not saved: {why}"));
    }
    let now = read(located)?;
    if now.revision != revision {
        return Err(format!(
            "{} changed since it was read; read it again",
            located.path.display()
        ));
    }
    if now.content == text {
        return Err("nothing changed".to_string());
    }
    let staged = private_copy(located.kind, "staged", text, folder)?;
    let file = quote(&located.path.to_string_lossy());
    let copy = quote(&staged.to_string_lossy());
    let expected = quote(revision);
    let absent = quote(ABSENT);
    // The shell's process id keeps two saves in one second apart.
    let program = format!(
        "file={file}\n\
         staged={copy}\n\
         if [ -e \"$file\" ]; then\n  \
         actual=$(sha256sum -- \"$file\") || exit 1\n  \
         actual=${{actual%% *}}\n\
         else\n  \
         actual={absent}\n\
         fi\n\
         if [ \"$actual\" != {expected} ]; then\n  \
         printf 'Helix Toolbox: %s changed since it was read; nothing was written\\n' \"$file\" >&2\n  \
         exit 1\n\
         fi\n\
         new=\"$file.helix-toolbox-new-$$\"\n\
         if [ -e \"$file\" ]; then\n  \
         hf_stamp=$(date +%Y%m%d%H%M%S)-$$\n  \
         cp -p -- \"$file\" \"$file.helix-toolbox-backup-$hf_stamp\" || exit 1\n  \
         printf 'Helix Toolbox: the previous one is kept as %s\\n' \"$file.helix-toolbox-backup-$hf_stamp\"\n  \
         cp -p -- \"$file\" \"$new\" || exit 1\n  \
         cat -- \"$staged\" > \"$new\" || {{ rm -f -- \"$new\"; exit 1; }}\n\
         else\n  \
         mkdir -p -- \"$(dirname -- \"$file\")\" || exit 1\n  \
         cp -- \"$staged\" \"$new\" || exit 1\n\
         fi\n\
         mv -f -- \"$new\" \"$file\" || {{ rm -f -- \"$new\"; exit 1; }}\n\
         rm -f -- \"$staged\"\n\
         printf 'Helix Toolbox: saved %s\\n' \"$file\""
    );
    let mut planned = Plan {
        steps: vec![Step::Shell {
            id: format!("config-{}", located.kind.id()),
            name: format!("Save {}'s configuration", located.kind.label()),
            program,
            display: printable(&format!(
                "the new text written over {} (checked unchanged since it was read; the current one kept as a backup first)",
                located.path.display()
            )),
            requires: vec!["sha256sum".to_string()],
            changes: vec![ShellChange {
                path: located.path.clone(),
                from: Some(staged),
                text: "the new text; the current one is kept as a configuration backup first"
                    .to_string(),
            }],
        }],
        requirements: vec!["sha256sum".to_string()],
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    };
    if let Checked::Unchecked(why) = checked {
        planned.warnings.push(format!("{why}."));
    }
    if let Some(flake) = &located.flake {
        planned.notes.push(format!(
            "It is part of the Home Manager flake in {}: run Home Manager (home-manager switch) afterward to make it live.",
            flake.display()
        ));
    }
    Ok(planned)
}

/// The lines that differ, as `-` and `+` lines with a little around them.
pub fn diff(path: &Path, before: &str, after: &str) -> String {
    match crate::preview::diff(&path.display().to_string(), before, after) {
        Some(found) => found.unified(),
        None => "(too long to compare line by line)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &Path) -> Option<(bool, String)> {
        None
    }

    struct Place {
        _dir: tempfile::TempDir,
        home: PathBuf,
        config: PathBuf,
        store: PathBuf,
    }

    fn place() -> Place {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let home = root.join("home");
        let config = home.join(".config");
        let store = root.join("nix/store");
        fs::create_dir_all(&config).unwrap();
        fs::create_dir_all(&store).unwrap();
        Place {
            _dir: dir,
            home,
            config,
            store,
        }
    }

    fn facts<'a>(
        place: &'a Place,
        settings: Option<&'a toml::Table>,
        center: Option<&'a str>,
    ) -> Facts<'a> {
        Facts {
            home: &place.home,
            config_home: &place.config,
            settings,
            command_center: center,
            store: &place.store,
        }
    }

    #[test]
    fn the_file_to_edit_is_the_settings_then_the_dotfiles_a_link_comes_from() {
        let place = place();
        // Nothing named: the live file, even before it is there.
        let found = locate(Kind::Ghostty, &facts(&place, None, None)).unwrap();
        assert_eq!(found.origin, Origin::Live);
        assert_eq!(found.path, place.config.join("ghostty/config"));
        // A Home Manager link: its source in the dotfiles, with the flake.
        let link = place.config.join("fastfetch/config.jsonc");
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        fs::write(place.store.join("config.jsonc"), "{}").unwrap();
        std::os::unix::fs::symlink(place.store.join("config.jsonc"), &link).unwrap();
        let refused = locate(Kind::Fastfetch, &facts(&place, None, None)).unwrap_err();
        assert!(
            refused.contains("fastfetch_source under [configs]"),
            "{refused}"
        );
        let source = place.home.join("dotfiles/configs/fastfetch/config.jsonc");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, "{}").unwrap();
        fs::write(place.home.join("dotfiles/flake.nix"), "{}").unwrap();
        let found = locate(Kind::Fastfetch, &facts(&place, None, None)).unwrap();
        assert_eq!((found.origin, found.managed), (Origin::Dotfiles, true));
        assert_eq!(found.path, source);
        assert_eq!(found.flake, Some(place.home.join("dotfiles")));
        // Command Center's, then settings.toml's, come first.
        let center = r#"{"integrations":{"fastfetchSource":"~/cc.jsonc"}}"#;
        let found = locate(Kind::Fastfetch, &facts(&place, None, Some(center))).unwrap();
        assert_eq!(
            (found.origin, found.path),
            (Origin::CommandCenter, place.home.join("cc.jsonc"))
        );
        let settings: toml::Table = "fastfetch_source = \"~/mine.jsonc\"".parse().unwrap();
        let found = locate(
            Kind::Fastfetch,
            &facts(&place, Some(&settings), Some(center)),
        )
        .unwrap();
        assert_eq!(found.origin, Origin::Settings);
        // Never the store, outside the home folder or up a folder.
        for bad in [
            format!("fastfetch_source = \"{}\"", link.display()),
            "fastfetch_source = \"/etc/fastfetch.jsonc\"".to_string(),
            "fastfetch_source = \"~/a/../b.jsonc\"".to_string(),
            "fastfetch_source = \"relative.jsonc\"".to_string(),
        ] {
            let settings: toml::Table = bad.parse().unwrap();
            assert!(
                locate(Kind::Fastfetch, &facts(&place, Some(&settings), None)).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn ghostty_fields_change_their_lines_and_the_rest_stays() {
        let text = "# mine\nfont-family = Iosevka\nfont-family = Noto Color Emoji\nfont-size = 11\nfont-size = 12\n";
        assert_eq!(
            ghostty_value(text, "font-family").as_deref(),
            Some("Iosevka")
        );
        assert_eq!(ghostty_value(text, "font-size").as_deref(), Some("12"));
        let changed = set(Kind::Ghostty, text, "font-size", "14").unwrap();
        assert_eq!(
            changed,
            "# mine\nfont-family = Iosevka\nfont-family = Noto Color Emoji\nfont-size = 11\nfont-size = 14\n"
        );
        // The font is the first font-family; the fallbacks stay.
        let changed = set(Kind::Ghostty, &changed, "font-family", "JetBrains Mono").unwrap();
        assert!(changed.contains("font-family = JetBrains Mono\nfont-family = Noto Color Emoji\n"));
        let added = set(Kind::Ghostty, "theme = x", "cursor-style", "bar").unwrap();
        assert_eq!(added, "theme = x\ncursor-style = bar\n");
        assert!(set(Kind::Ghostty, text, "command", "rm").is_err());
        assert!(set(Kind::Ghostty, text, "theme", "a\nb").is_err());
    }

    #[test]
    fn fastfetch_jsonc_is_read_with_comments_and_trailing_commas() {
        let text = "// mine\n{\n    \"$schema\": \"x\", /* a note */\n    \"logo\": { \"source\": \"arch\" },\n    \"modules\": [\n        \"title\",\n        { \"type\": \"os\", \"format\": \"{1}\" }, // the system\n        \"memory\",\n    ],\n}\n";
        assert_eq!(validate(Kind::Fastfetch, text, &none, None), Checked::Valid);
        assert_eq!(
            fastfetch_value(text, "modules").as_deref(),
            Some("title, os, memory")
        );
        assert_eq!(
            fastfetch_value(text, "logo.source").as_deref(),
            Some("arch")
        );
        for (bad, why) in [
            ("{ \"a\": 1 ", "line 1"),
            ("[1, 2]", "one object"),
            ("{ \"modules\": [ { \"format\": \"x\" } ] }", "module 1"),
            ("{ \"modules\": \"os\" }", "must be a list"),
            ("{ a: 1 }", "double quotes"),
            ("{ \"a\": \"x\n\" }", "not closed"),
        ] {
            match validate(Kind::Fastfetch, bad, &none, None) {
                Checked::Invalid(said) => assert!(said.contains(why), "{bad}: {said}"),
                other => panic!("{bad}: {other:?}"),
            }
        }
    }

    #[test]
    fn fastfetch_fields_keep_the_comments_and_the_module_options() {
        let text = "{\n    // the logo\n    \"logo\": { \"source\": \"arch\" },\n    \"modules\": [\n        \"title\",\n        { \"type\": \"os\", \"format\": \"{1}\" },\n        \"memory\"\n    ]\n}\n";
        let changed = set(Kind::Fastfetch, text, "logo.source", "garuda").unwrap();
        assert!(
            changed.contains("// the logo\n    \"logo\": { \"source\": \"garuda\" }"),
            "{changed}"
        );
        let changed = set(Kind::Fastfetch, &changed, "logo.type", "small").unwrap();
        assert!(changed.contains("{ \"type\": \"small\",\n"), "{changed}");
        assert_eq!(
            fastfetch_value(&changed, "logo.type").as_deref(),
            Some("small")
        );
        let changed = set(Kind::Fastfetch, &changed, "modules", "os, kernel, title").unwrap();
        assert!(changed.contains("[\n        { \"type\": \"os\", \"format\": \"{1}\" },\n        \"kernel\",\n        \"title\"\n    ]"), "{changed}");
        assert_eq!(
            validate(Kind::Fastfetch, &changed, &none, None),
            Checked::Valid
        );
        // An empty file, and a logo given as a name.
        let made = set(Kind::Fastfetch, "", "modules", "os").unwrap();
        assert_eq!(fastfetch_value(&made, "modules").as_deref(), Some("os"));
        let named = set(
            Kind::Fastfetch,
            "{ \"logo\": \"arch\" }",
            "logo.type",
            "small",
        )
        .unwrap();
        assert_eq!(
            fastfetch_value(&named, "logo.source").as_deref(),
            Some("arch")
        );
        assert_eq!(
            fastfetch_value(&named, "logo.type").as_deref(),
            Some("small")
        );
        assert!(set(Kind::Fastfetch, "{ bad", "logo.type", "x").is_err());
        assert!(set(Kind::Fastfetch, text, "modules", "os; rm").is_err());
    }

    #[test]
    fn ghostty_checks_a_copy_and_says_when_it_cannot() {
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("configs");
        let rejects = |file: &Path| {
            let text = fs::read_to_string(file).unwrap();
            Some(if text.contains("bogus") {
                (false, format!("{}: unknown field: bogus", file.display()))
            } else {
                (true, String::new())
            })
        };
        assert_eq!(
            validate(Kind::Ghostty, "theme = x\n", &rejects, Some(&folder)),
            Checked::Valid
        );
        match validate(Kind::Ghostty, "bogus = 1\n", &rejects, Some(&folder)) {
            Checked::Invalid(why) => {
                assert_eq!(why, "Ghostty rejected it: the file: unknown field: bogus")
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            validate(Kind::Ghostty, "x = 1", &none, Some(&folder)),
            Checked::Unchecked(_)
        ));
        // No copy is left behind.
        let left = fs::read_dir(&folder).unwrap().count();
        assert_eq!(left, 0);
    }

    #[test]
    fn copies_are_never_made_in_a_link_or_a_folder_of_another_user() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = dir.path().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        let link = dir.path().join("configs");
        std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
        let refused = private_copy(Kind::Ghostty, "edit", "x = 1\n", &link).unwrap_err();
        assert!(refused.contains("is a link"), "{refused}");
        let rejects = |_: &Path| Some((true, String::new()));
        assert!(matches!(
            validate(Kind::Ghostty, "x = 1\n", &rejects, Some(&link)),
            Checked::Unchecked(_)
        ));
        assert!(matches!(
            validate(Kind::Ghostty, "x = 1\n", &rejects, None),
            Checked::Unchecked(_)
        ));
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
        // A folder of this user's own is closed to the others.
        let own = dir.path().join("own");
        let copy = private_copy(Kind::Ghostty, "edit", "x = 1\n", &own).unwrap();
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(&own).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn saving_is_a_plan_that_checks_the_file_and_keeps_a_backup() {
        let place = place();
        let state = tempfile::tempdir().unwrap();
        let folder = state.path().join("configs");
        let file = place.config.join("ghostty/config");
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, "font-size = 11\n").unwrap();
        let located = locate(Kind::Ghostty, &facts(&place, None, None)).unwrap();
        let loaded = read(&located).unwrap();
        let text = set(Kind::Ghostty, &loaded.content, "font-size", "13").unwrap();
        assert!(
            plan_save(
                &located,
                &loaded.revision,
                &text,
                &Checked::Invalid("no".into()),
                &folder
            )
            .is_err()
        );
        assert!(
            plan_save(&located, "0000", &text, &Checked::Valid, &folder)
                .unwrap_err()
                .contains("changed since")
        );
        let planned = plan_save(
            &located,
            &loaded.revision,
            &text,
            &Checked::Unchecked("not checked".into()),
            &folder,
        )
        .unwrap();
        assert_eq!(planned.warnings, ["not checked."]);
        let Step::Shell {
            program, changes, ..
        } = &planned.steps[0]
        else {
            panic!("{planned:?}");
        };
        assert_eq!(changes[0].path, file);
        // Run, it writes the new text and keeps the old as a backup.
        let ran = Command::new("sh").arg("-c").arg(program).output().unwrap();
        assert!(
            ran.status.success(),
            "{}",
            String::from_utf8_lossy(&ran.stderr)
        );
        assert_eq!(fs::read_to_string(&file).unwrap(), "font-size = 13\n");
        let kept = history(&located);
        assert_eq!(kept.len(), 1);
        assert_eq!(
            fs::read_to_string(&kept[0].backup).unwrap(),
            "font-size = 11\n"
        );
        assert!(
            !changes[0].from.as_ref().unwrap().exists(),
            "the private copy is removed"
        );
        // Run again, the file is no longer as it was read: nothing written.
        let again = Command::new("sh").arg("-c").arg(program).output().unwrap();
        assert!(!again.status.success());
        assert_eq!(fs::read_to_string(&file).unwrap(), "font-size = 13\n");
        assert_eq!(history(&located).len(), 1);
    }
}
