//! The service and startup manager: systemd's units on this machine and in
//! the user's own session, and the applications the desktop starts at login
//! (XDG autostart), read without changing anything. Starting, stopping,
//! enabling and disabling are plans like any other, confirmed first, and a
//! unit the machine or the session needs (the login screen, the network,
//! name lookups, the firewall, systemd's own) says what stops working
//! before it is stopped or disabled.

use crate::{Plan, Step, doctor::System, quote};
use helix_toolbox_core::{Risk, printable, valid_unit_name};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Whose units: the machine's, or the user's own session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    System,
    User,
}

/// One systemd unit.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Unit {
    /// "sddm.service".
    pub name: String,
    pub description: String,
    /// The unit file's state: enabled, disabled, static, masked, indirect…;
    /// none for a unit loaded without a file of its own.
    pub enablement: Option<String>,
    /// active, inactive, failed; none when it is not loaded.
    pub active: Option<String>,
    /// running, exited, dead, waiting…
    pub sub: Option<String>,
    pub user: bool,
}

impl Unit {
    /// service, timer or socket.
    pub fn kind(&self) -> &str {
        self.name.rsplit('.').next().unwrap_or_default()
    }

    pub fn enabled(&self) -> bool {
        matches!(
            self.enablement.as_deref(),
            Some("enabled" | "enabled-runtime" | "alias" | "linked" | "linked-runtime")
        )
    }

    pub fn running(&self) -> bool {
        self.active.as_deref() == Some("active")
    }

    pub fn failed(&self) -> bool {
        self.active.as_deref() == Some("failed")
    }

    /// Whether enable and disable apply: it has a unit file that is not
    /// static (started by something else), masked or generated.
    pub fn switchable(&self) -> bool {
        matches!(
            self.enablement.as_deref(),
            Some("enabled" | "disabled" | "enabled-runtime" | "indirect" | "linked" | "alias")
        )
    }
}

/// An application the desktop starts at login.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize)]
pub struct Autostart {
    /// The file's name, its id: "org.kde.discover.notifier.desktop".
    pub file: String,
    pub name: String,
    pub comment: String,
    pub exec: String,
    /// Turned off with Hidden=true or X-GNOME-Autostart-enabled=false.
    pub enabled: bool,
    /// Whether it starts in this desktop (OnlyShowIn and NotShowIn).
    pub here: bool,
    /// Where it comes from: the system's folder, or the user's own.
    pub system: bool,
    /// The user's own file, when there is one (an override of a system one,
    /// or an application of their own).
    pub own: Option<PathBuf>,
}

/// What the manager shows.
#[derive(Clone, Debug, Default, Serialize)]
pub struct Services {
    /// Whether systemd runs this machine; without it there are no units.
    pub systemd: bool,
    /// Whether the user's own systemd answered.
    pub session: bool,
    pub units: Vec<Unit>,
    pub autostart: Vec<Autostart>,
}

/// Read the machine's and the session's units and the autostart entries.
pub fn gather(system: &dyn System, desktop: Option<&str>) -> Services {
    let systemd = system.exists(Path::new("/run/systemd/system"));
    let mut units = Vec::new();
    let mut session = false;
    if systemd {
        units.extend(units_of(system, Scope::System));
        let user = units_of(system, Scope::User);
        session = !user.is_empty();
        units.extend(user);
    }
    Services {
        systemd,
        session,
        units,
        autostart: autostart(system, desktop),
    }
}

fn systemctl(scope: Scope, words: &[&str]) -> Vec<String> {
    let mut command = vec!["systemctl".to_string()];
    if scope == Scope::User {
        command.push("--user".to_string());
    }
    command.extend(words.iter().map(|word| word.to_string()));
    command
}

fn run(system: &dyn System, command: &[String]) -> Option<String> {
    let words: Vec<&str> = command.iter().map(String::as_str).collect();
    system.run(&words).map(|(_, text)| text)
}

/// The units of one scope: those with a unit file (not templates), with how
/// they run, and those running or failed without a file of their own,
/// except the transient ones desktops start for each application.
fn units_of(system: &dyn System, scope: Scope) -> Vec<Unit> {
    const TYPES: &str = "--type=service,timer,socket";
    let files = run(
        system,
        &systemctl(
            scope,
            &["list-unit-files", TYPES, "--no-pager", "--output=json"],
        ),
    )
    .map(|text| parse_files(&text))
    .unwrap_or_default();
    let loaded = run(
        system,
        &systemctl(
            scope,
            &["list-units", "--all", TYPES, "--no-pager", "--output=json"],
        ),
    )
    .map(|text| parse_units(&text))
    .unwrap_or_default();
    let mut units: Vec<Unit> = files
        .into_iter()
        .filter(|(name, _)| !name.contains("@."))
        .map(|(name, state)| Unit {
            name,
            enablement: Some(state),
            user: scope == Scope::User,
            ..Unit::default()
        })
        .collect();
    for (name, active, sub, description) in loaded {
        match units.iter_mut().find(|unit| unit.name == name) {
            Some(unit) => {
                unit.active = Some(active);
                unit.sub = Some(sub);
                unit.description = description;
            }
            None if (active == "active" || active == "failed")
                && !name.starts_with("app-")
                && !name.starts_with("run-")
                && !name.starts_with("session-") =>
            {
                units.push(Unit {
                    name,
                    description,
                    enablement: None,
                    active: Some(active),
                    sub: Some(sub),
                    user: scope == Scope::User,
                })
            }
            None => {}
        }
    }
    units.retain(|unit| valid_unit_name(&unit.name));
    units.sort_by_key(|unit| unit.name.to_lowercase());
    units
}

/// `list-unit-files`: each file and its state, from JSON or, from a systemd
/// too old for it, the plain columns.
pub fn parse_files(text: &str) -> Vec<(String, String)> {
    if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(text) {
        return items
            .iter()
            .filter_map(|item| {
                Some((
                    printable(item["unit_file"].as_str()?),
                    printable(item["state"].as_str()?),
                ))
            })
            .collect();
    }
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let (name, state) = (words.next()?, words.next()?);
            (name.contains('.') && !name.starts_with("UNIT"))
                .then(|| (printable(name), printable(state)))
        })
        .collect()
}

/// `list-units`: each loaded unit, how it is active and its description.
pub fn parse_units(text: &str) -> Vec<(String, String, String, String)> {
    if let Ok(serde_json::Value::Array(items)) = serde_json::from_str(text) {
        return items
            .iter()
            .filter_map(|item| {
                Some((
                    printable(item["unit"].as_str()?),
                    printable(item["active"].as_str()?),
                    printable(item["sub"].as_str().unwrap_or_default()),
                    printable(item["description"].as_str().unwrap_or_default()),
                ))
            })
            .collect();
    }
    text.lines()
        .filter_map(|line| {
            let line = line.trim_start_matches(['●', '*', ' ']);
            let mut words = line.split_whitespace();
            let (name, _load, active, sub) =
                (words.next()?, words.next()?, words.next()?, words.next()?);
            name.contains('.').then(|| {
                (
                    printable(name),
                    printable(active),
                    printable(sub),
                    printable(&words.collect::<Vec<_>>().join(" ")),
                )
            })
        })
        .collect()
}

/// The folders autostart entries are read from: the system's (from
/// `XDG_CONFIG_DIRS`, `/etc/xdg` when unset) and the user's.
pub struct AutostartDirs {
    pub system: Vec<PathBuf>,
    pub user: PathBuf,
}

impl AutostartDirs {
    pub fn here(system: &dyn System) -> Self {
        let home = system.user().2;
        let variable = |name: &str| std::env::var(name).ok().filter(|value| !value.is_empty());
        let config_dirs = variable("XDG_CONFIG_DIRS").unwrap_or_else(|| "/etc/xdg".to_string());
        let config = variable("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"));
        AutostartDirs {
            system: config_dirs
                .split(':')
                .filter(|dir| dir.starts_with('/'))
                .map(|dir| PathBuf::from(dir).join("autostart"))
                .collect(),
            user: config.join("autostart"),
        }
    }
}

/// A desktop entry's keys in its `[Desktop Entry]` group.
fn desktop_entry(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut found = std::collections::BTreeMap::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        if !inside || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            found.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    found
}

/// Whether a desktop entry starts in this desktop.
fn shows_in(keys: &std::collections::BTreeMap<String, String>, desktop: Option<&str>) -> bool {
    let current: Vec<String> = desktop
        .unwrap_or_default()
        .split(':')
        .map(str::to_lowercase)
        .collect();
    let listed = |key: &str| {
        keys.get(key).map(|value| {
            value
                .split(';')
                .any(|name| !name.is_empty() && current.contains(&name.to_lowercase()))
        })
    };
    listed("OnlyShowIn").unwrap_or(true) && !listed("NotShowIn").unwrap_or(false)
}

/// A file name Helix Toolbox can write an override under.
pub fn valid_autostart_file(file: &str) -> bool {
    file.ends_with(".desktop")
        && file.len() <= 200
        && !file.starts_with(['.', '-'])
        && file
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._@+-".contains(&byte))
}

/// The autostart entries, the user's file over the system's of the same name.
pub fn autostart(system: &dyn System, desktop: Option<&str>) -> Vec<Autostart> {
    autostart_in(system, &AutostartDirs::here(system), desktop)
}

pub fn autostart_in(
    system: &dyn System,
    dirs: &AutostartDirs,
    desktop: Option<&str>,
) -> Vec<Autostart> {
    let mut found: Vec<Autostart> = Vec::new();
    let read = |dir: &Path, user: bool, found: &mut Vec<Autostart>| {
        for file in system.list(dir) {
            if !file.ends_with(".desktop") {
                continue;
            }
            let path = dir.join(&file);
            let Some(text) = system.read(&path) else {
                continue;
            };
            let keys = desktop_entry(&text);
            let hidden = keys.get("Hidden").is_some_and(|value| value == "true");
            let off = keys
                .get("X-GNOME-Autostart-enabled")
                .is_some_and(|value| value == "false");
            let item = Autostart {
                file: printable(&file),
                name: printable(keys.get("Name").map(String::as_str).unwrap_or(&file)),
                comment: printable(keys.get("Comment").map(String::as_str).unwrap_or_default()),
                exec: printable(keys.get("Exec").map(String::as_str).unwrap_or_default()),
                enabled: !hidden && !off,
                here: shows_in(&keys, desktop),
                system: !user,
                own: user.then(|| path.clone()),
            };
            match found.iter_mut().find(|other| other.file == item.file) {
                // The user's file wins; a bare override keeps the system's name.
                Some(other) if user => {
                    other.enabled = item.enabled;
                    if keys.contains_key("OnlyShowIn") || keys.contains_key("NotShowIn") {
                        other.here = item.here;
                    }
                    other.own = item.own;
                    if keys.contains_key("Exec") {
                        other.exec = item.exec;
                    }
                }
                Some(_) => {}
                None => found.push(item),
            }
        }
    };
    for dir in &dirs.system {
        read(dir, false, &mut found);
    }
    read(&dirs.user, true, &mut found);
    found.sort_by_key(|item| item.name.to_lowercase());
    found
}

/// What stopping or disabling a unit costs, when the machine or the session
/// needs it; none for the others.
pub fn consequence(name: &str, user: bool, ssh: bool) -> Option<&'static str> {
    let base = name
        .trim_end_matches(".service")
        .trim_end_matches(".socket");
    if user {
        return match base {
            "pipewire" | "pipewire-pulse" | "wireplumber" | "pulseaudio" => {
                Some("Sound stops in your session until it runs again.")
            }
            name if name.starts_with("xdg-desktop-portal") => Some(
                "Applications lose their file pickers, screen sharing and other portals until it runs again.",
            ),
            name if name.starts_with("plasma-") || name.starts_with("org.gnome.") => {
                Some("It is part of your desktop session, which may stop working.")
            }
            _ => None,
        };
    }
    match base {
        "display-manager" | "sddm" | "gdm" | "gdm3" | "lightdm" | "lxdm" | "ly" | "greetd" => Some(
            "It runs the login screen: stopped, your desktop session ends now; disabled, the next boot stops at a text console, where you log in and enable it again.",
        ),
        "NetworkManager" | "systemd-networkd" | "wpa_supplicant" | "iwd" | "connman" | "dhcpcd" => {
            Some(
                "It connects this machine to the network: without it the network goes, and updates and anything online stop working.",
            )
        }
        "systemd-resolved" => {
            Some("It answers name lookups: without it websites and updates fail to resolve.")
        }
        "firewalld" | "ufw" | "nftables" | "iptables" | "ip6tables" => Some(
            "It is the firewall: without it this machine's ports are open to the network it is on.",
        ),
        "sshd" | "ssh" if ssh => Some(
            "You are connected through SSH: stopping it can cut this session, and disabling it locks you out after the next boot.",
        ),
        "dbus" | "dbus-broker" | "polkit" => Some(
            "The desktop and systemd talk through it: stopping it can end your session or stop the machine from working.",
        ),
        name if name.starts_with("systemd-") || name.starts_with("getty@") => Some(
            "It is part of systemd or the text consoles: stopping or disabling it can stop the machine from starting or logging you in.",
        ),
        _ => None,
    }
}

/// What a plan does to a unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Verb {
    Start,
    Stop,
    Restart,
    Enable,
    Disable,
}

impl Verb {
    pub fn word(self) -> &'static str {
        match self {
            Verb::Start => "start",
            Verb::Stop => "stop",
            Verb::Restart => "restart",
            Verb::Enable => "enable",
            Verb::Disable => "disable",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        [
            Verb::Start,
            Verb::Stop,
            Verb::Restart,
            Verb::Enable,
            Verb::Disable,
        ]
        .into_iter()
        .find(|verb| verb.word() == word)
    }

    /// Whether it can take away something the unit gives.
    fn costs(self) -> bool {
        matches!(self, Verb::Stop | Verb::Restart | Verb::Disable)
    }
}

/// What needs a unit, from systemd: the units that want or require it.
pub fn needed_by(system: &dyn System, name: &str, user: bool) -> Vec<String> {
    if !valid_unit_name(name) {
        return Vec::new();
    }
    let scope = if user { Scope::User } else { Scope::System };
    run(
        system,
        &systemctl(
            scope,
            &[
                "list-dependencies",
                "--reverse",
                "--plain",
                "--no-pager",
                "--no-legend",
                name,
            ],
        ),
    )
    .map(|text| {
        text.lines()
            .skip(1)
            .map(|line| printable(line.trim()))
            .filter(|line| valid_unit_name(line))
            .collect()
    })
    .unwrap_or_default()
}

/// A plan that starts, stops, restarts, enables or disables one unit, with
/// what it costs first when the machine or the session needs the unit.
pub fn plan(
    name: &str,
    user: bool,
    verb: Verb,
    needed: &[String],
    ssh: bool,
) -> Result<Plan, String> {
    if !valid_unit_name(name) {
        return Err(format!("{} is not a systemd unit name", printable(name)));
    }
    let mut words = vec!["systemctl"];
    if user {
        words.push("--user");
    } else {
        words.insert(0, "sudo");
    }
    words.push(verb.word());
    words.push(name);
    let cost = verb.costs().then(|| consequence(name, user, ssh)).flatten();
    let mut warnings: Vec<String> = cost.iter().map(|cost| cost.to_string()).collect();
    if verb.costs() && !needed.is_empty() {
        let shown: Vec<&str> = needed.iter().take(6).map(String::as_str).collect();
        let more = match needed.len() {
            0..=6 => String::new(),
            count => format!(" and {} more", count - 6),
        };
        warnings.push(format!("It is needed by {}{more}.", shown.join(", ")));
    }
    let risk = match (cost.is_some(), user) {
        (true, _) => Risk::High,
        (false, false) => Risk::Medium,
        (false, true) => Risk::Low,
    };
    let note = match verb {
        Verb::Start | Verb::Stop | Verb::Restart => {
            "Now only: what starts it at boot or login is not changed."
        }
        Verb::Enable => "From the next boot or login; it is not started now.",
        Verb::Disable => "From the next boot or login; it is not stopped now.",
    };
    let step = Step::Shell {
        id: format!("{}-{name}", verb.word()),
        name: format!(
            "{} {}{name}",
            match verb {
                Verb::Start => "Start",
                Verb::Stop => "Stop",
                Verb::Restart => "Restart",
                Verb::Enable => "Enable",
                Verb::Disable => "Disable",
            },
            if user { "your " } else { "" }
        ),
        program: words
            .iter()
            .map(|word| quote(word))
            .collect::<Vec<_>>()
            .join(" "),
        display: words.join(" "),
        requires: if user {
            vec!["systemctl".to_string()]
        } else {
            vec!["sudo".to_string(), "systemctl".to_string()]
        },
        changes: Vec::new(),
    };
    Ok(Plan {
        requirements: match &step {
            Step::Shell { requires, .. } => requires.clone(),
            _ => Vec::new(),
        },
        steps: vec![step],
        notes: vec![note.to_string()],
        privileged: !user,
        risk,
        depends: vec![Vec::new()],
        warnings,
        ..Plan::default()
    })
}

/// A plan that turns an autostart entry on or off for this user: off writes
/// (or changes) the user's own file with Hidden=true, which hides the
/// system's; on removes that override, or takes Hidden out of the user's own
/// application.
pub fn plan_autostart(
    item: &Autostart,
    on: bool,
    dirs: &AutostartDirs,
    system: &dyn System,
) -> Result<Plan, String> {
    if !valid_autostart_file(&item.file) {
        return Err(format!(
            "{} has a file name Helix Toolbox does not write",
            item.file
        ));
    }
    let path = dirs.user.join(&item.file);
    let shown = path.display().to_string();
    let own = item.own.as_ref().and_then(|own| system.read(own));
    // An override of a system entry is the user's file and nothing more.
    let program = match (on, item.system, own) {
        (false, _, Some(text)) => {
            let mut lines: Vec<String> = text
                .lines()
                .filter(|line| !line.trim_start().starts_with("Hidden="))
                .map(str::to_string)
                .collect();
            let at = lines
                .iter()
                .position(|line| line.trim() == "[Desktop Entry]")
                .map_or(lines.len(), |at| at + 1);
            lines.insert(at, "Hidden=true".to_string());
            write_file(&path, &(lines.join("\n") + "\n"))
        }
        (false, _, None) => write_file(
            &path,
            &format!(
                "[Desktop Entry]\nType=Application\nName={}\nHidden=true\n",
                item.name.replace('\n', " ")
            ),
        ),
        (true, true, Some(_)) => format!("rm -f -- {}", quote(&shown)),
        (true, false, Some(text)) => write_file(
            &path,
            &(text
                .lines()
                .filter(|line| {
                    let line = line.trim_start();
                    !line.starts_with("Hidden=") && line != "X-GNOME-Autostart-enabled=false"
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"),
        ),
        (true, _, None) => return Err(format!("{} is already on", item.name)),
    };
    let step = Step::Shell {
        id: format!("autostart-{}", item.file.trim_end_matches(".desktop")),
        name: format!(
            "{} {} at login",
            if on { "Start" } else { "Do not start" },
            item.name
        ),
        program,
        display: if on {
            format!("take Hidden=true out of {shown}")
        } else {
            format!("write Hidden=true into {shown}")
        },
        requires: Vec::new(),
        changes: vec![crate::ShellChange {
            path: path.clone(),
            from: None,
            text: if on {
                "the override that keeps it from starting goes".to_string()
            } else {
                "Hidden=true, so the desktop does not start it at login".to_string()
            },
        }],
    };
    Ok(Plan {
        steps: vec![step],
        notes: vec!["From your next login; nothing running now is stopped or started.".to_string()],
        risk: Risk::Low,
        depends: vec![Vec::new()],
        ..Plan::default()
    })
}

/// A shell program that writes a file whole, its folder made first.
fn write_file(path: &Path, text: &str) -> String {
    let shown = path.display().to_string();
    let folder = path
        .parent()
        .map(|parent| parent.display().to_string())
        .unwrap_or_default();
    format!(
        "mkdir -p -- {} && printf '%s' {} > {}",
        quote(&folder),
        quote(text),
        quote(&shown)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Fake {
        files: HashMap<PathBuf, String>,
        commands: HashMap<String, String>,
        folders: Vec<PathBuf>,
    }

    impl System for Fake {
        fn read(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn exists(&self, path: &Path) -> bool {
            self.files.contains_key(path) || self.folders.iter().any(|folder| folder == path)
        }
        fn list(&self, path: &Path) -> Vec<String> {
            let mut names: Vec<String> = self
                .files
                .keys()
                .filter(|file| file.parent() == Some(path))
                .filter_map(|file| Some(file.file_name()?.to_string_lossy().into_owned()))
                .collect();
            names.sort();
            names
        }
        fn link(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn run(&self, command: &[&str]) -> Option<(bool, String)> {
            self.commands
                .get(&command.join(" "))
                .map(|text| (true, text.clone()))
        }
        fn resolve(&self, _: &str) -> Result<Vec<String>, String> {
            Err("offline".into())
        }
        fn space(&self, _: &Path) -> Option<(u64, u64)> {
            None
        }
        fn owner(&self, _: &Path) -> Option<u32> {
            None
        }
        fn is_link(&self, _: &Path) -> bool {
            false
        }
        fn canonical(&self, _: &Path) -> Option<PathBuf> {
            None
        }
        fn user(&self) -> (u32, u32, PathBuf) {
            (1000, 1000, PathBuf::from("/home/me"))
        }
        fn on_path(&self, _: &str) -> bool {
            true
        }
        fn processes(&self) -> Vec<String> {
            Vec::new()
        }
    }

    const FILES: &str = r#"[{"unit_file":"sddm.service","state":"enabled","preset":"disabled"},{"unit_file":"fstrim.timer","state":"disabled","preset":"disabled"},{"unit_file":"getty@.service","state":"enabled","preset":"enabled"},{"unit_file":"systemd-journald.service","state":"static","preset":null}]"#;
    const UNITS: &str = r#"[{"unit":"sddm.service","load":"loaded","active":"active","sub":"running","description":"Simple Desktop Display Manager"},{"unit":"getty@tty1.service","load":"loaded","active":"active","sub":"running","description":"Getty on tty1"},{"unit":"broken.service","load":"loaded","active":"failed","sub":"failed","description":"Broken"},{"unit":"run-u12.service","load":"loaded","active":"active","sub":"running","description":"transient"}]"#;

    fn machine() -> Fake {
        let mut fake = Fake {
            folders: vec![PathBuf::from("/run/systemd/system")],
            ..Fake::default()
        };
        let types = "--type=service,timer,socket";
        fake.commands.insert(
            format!("systemctl list-unit-files {types} --no-pager --output=json"),
            FILES.into(),
        );
        fake.commands.insert(
            format!("systemctl list-units --all {types} --no-pager --output=json"),
            UNITS.into(),
        );
        fake.commands.insert(
            format!("systemctl --user list-unit-files {types} --no-pager --output=json"),
            r#"[{"unit_file":"pipewire.service","state":"indirect","preset":"enabled"}]"#.into(),
        );
        fake.commands.insert(
            format!("systemctl --user list-units --all {types} --no-pager --output=json"),
            r#"[{"unit":"pipewire.service","load":"loaded","active":"active","sub":"running","description":"PipeWire"},{"unit":"app-firefox@1.service","load":"loaded","active":"active","sub":"running","description":"Firefox"}]"#.into(),
        );
        fake
    }

    #[test]
    fn units_come_from_their_files_and_what_runs() {
        let found = gather(&machine(), Some("KDE"));
        assert!(found.systemd && found.session);
        let names: Vec<&str> = found.units.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "broken.service",
                "fstrim.timer",
                "getty@tty1.service",
                "sddm.service",
                "systemd-journald.service",
                "pipewire.service"
            ],
            "the machine's units, then the session's"
        );
        let sddm = found
            .units
            .iter()
            .find(|unit| unit.name == "sddm.service")
            .unwrap();
        assert!(sddm.enabled() && sddm.running() && sddm.switchable());
        assert_eq!(sddm.description, "Simple Desktop Display Manager");
        let journald = found
            .units
            .iter()
            .find(|unit| unit.name == "systemd-journald.service")
            .unwrap();
        assert!(!journald.switchable(), "static units are started by others");
        assert!(found.units.iter().any(|unit| unit.failed()));
        assert!(
            found
                .units
                .iter()
                .any(|unit| unit.user && unit.name == "pipewire.service")
        );
        let none = gather(&Fake::default(), None);
        assert!(!none.systemd && none.units.is_empty());
    }

    #[test]
    fn older_systemd_answers_in_columns() {
        let files = "UNIT FILE STATE PRESET\nsddm.service enabled disabled\nfstrim.timer disabled disabled\n\n2 unit files listed.\n";
        assert_eq!(
            parse_files(files)[0],
            ("sddm.service".to_string(), "enabled".to_string())
        );
        assert_eq!(parse_files(files).len(), 2);
        let units = "● broken.service loaded failed failed Broken thing\n  sddm.service loaded active running Simple Desktop Display Manager\n";
        let parsed = parse_units(units);
        assert_eq!(parsed[0].0, "broken.service");
        assert_eq!(parsed[1].3, "Simple Desktop Display Manager");
    }

    #[test]
    fn stopping_what_the_machine_needs_says_what_it_costs() {
        let needed = vec!["graphical.target".to_string()];
        let stop = plan("sddm.service", false, Verb::Disable, &needed, false).unwrap();
        assert_eq!(stop.risk, Risk::High);
        assert!(stop.privileged);
        assert!(stop.warnings[0].contains("login screen"));
        assert!(stop.warnings[1].contains("graphical.target"));
        assert_eq!(
            stop.steps[0].display(),
            "sudo systemctl disable sddm.service"
        );
        let start = plan("sddm.service", false, Verb::Start, &needed, false).unwrap();
        assert!(start.warnings.is_empty(), "starting costs nothing");
        let timer = plan("fstrim.timer", false, Verb::Enable, &[], false).unwrap();
        assert_eq!(timer.risk, Risk::Medium);
        let mine = plan("pipewire.service", true, Verb::Stop, &[], false).unwrap();
        assert!(!mine.privileged);
        assert!(mine.warnings[0].contains("Sound"));
        assert_eq!(
            mine.steps[0].display(),
            "systemctl --user stop pipewire.service"
        );
        assert!(consequence("sshd.service", false, true).is_some());
        assert!(consequence("sshd.service", false, false).is_none());
        assert!(plan("x; rm -rf /", false, Verb::Stop, &[], false).is_err());
    }

    fn autostart_machine() -> Fake {
        let mut fake = Fake::default();
        fake.files.insert(
            PathBuf::from("/etc/xdg/autostart/discover.desktop"),
            "[Desktop Entry]\nName=Discover\nExec=discover --notifier\nOnlyShowIn=KDE;\n".into(),
        );
        fake.files.insert(
            PathBuf::from("/etc/xdg/autostart/tracker.desktop"),
            "[Desktop Entry]\nName=Tracker\nExec=tracker\nNotShowIn=KDE;\n".into(),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/autostart/tracker.desktop"),
            "[Desktop Entry]\nHidden=true\n".into(),
        );
        fake.files.insert(
            PathBuf::from("/home/me/.config/autostart/notes.desktop"),
            "[Desktop Entry]\nName=Notes\nExec=notes\nX-GNOME-Autostart-enabled=false\n".into(),
        );
        fake
    }

    fn dirs() -> AutostartDirs {
        AutostartDirs {
            system: vec![PathBuf::from("/etc/xdg/autostart")],
            user: PathBuf::from("/home/me/.config/autostart"),
        }
    }

    #[test]
    fn autostart_reads_the_users_files_over_the_systems() {
        let found = autostart_in(&autostart_machine(), &dirs(), Some("KDE"));
        let names: Vec<&str> = found.iter().map(|item| item.name.as_str()).collect();
        assert_eq!(names, ["Discover", "Notes", "Tracker"]);
        assert!(found[0].enabled && found[0].here && found[0].system && found[0].own.is_none());
        assert!(!found[1].enabled && !found[1].system);
        assert!(!found[2].enabled && !found[2].here && found[2].own.is_some());
        assert_eq!(
            found[2].exec, "tracker",
            "a bare override keeps the system's command"
        );
    }

    #[test]
    fn autostart_is_turned_off_and_on_with_an_override_of_the_users_own() {
        let machine = autostart_machine();
        let found = autostart_in(&machine, &dirs(), Some("KDE"));
        let off = plan_autostart(&found[0], false, &dirs(), &machine).unwrap();
        let Step::Shell {
            program, changes, ..
        } = &off.steps[0]
        else {
            panic!()
        };
        assert!(
            program.contains("Hidden=true")
                && program.contains("/home/me/.config/autostart/discover.desktop")
        );
        assert_eq!(
            changes[0].path,
            PathBuf::from("/home/me/.config/autostart/discover.desktop")
        );
        assert!(!off.privileged);
        // On again: the override of a system entry goes.
        let on = plan_autostart(&found[2], true, &dirs(), &machine).unwrap();
        assert!(on.steps[0].display().contains("take Hidden=true out"));
        let Step::Shell { program, .. } = &on.steps[0] else {
            panic!()
        };
        assert!(program.starts_with("rm -f -- '/home/me/.config/autostart/tracker.desktop'"));
        // The user's own application keeps its file, without the switch.
        let notes = plan_autostart(&found[1], true, &dirs(), &machine).unwrap();
        let Step::Shell { program, .. } = &notes.steps[0] else {
            panic!()
        };
        assert!(
            program.contains("Exec=notes") && !program.contains("X-GNOME-Autostart-enabled=false")
        );
        assert!(
            plan_autostart(&found[0], true, &dirs(), &machine).is_err(),
            "already on"
        );
        let odd = Autostart {
            file: "a b.desktop".into(),
            ..found[0].clone()
        };
        assert!(plan_autostart(&odd, false, &dirs(), &machine).is_err());
    }
}
