//! Telling the user when updates wait, without being asked: a systemd user
//! timer runs `helix-toolbox notifications check` a few times a day. The
//! check reads the same overview as `helix-toolbox updates --online` and
//! shows a desktop notification when what waits differs from what it last
//! told of. It only reads and tells; nothing is updated.
//!
//! Turning it on writes the timer and its service into the user's systemd
//! folder and enables the timer; turning it off removes them. Both are
//! files of the user's own, so neither needs a password.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use crate::history;
use crate::updates::Channel;

pub const TIMER: &str = "helix-toolbox-updates.timer";
pub const SERVICE: &str = "helix-toolbox-updates.service";
/// How often the timer checks, after a first check soon after logging in.
pub const EVERY: &str = "every 6 hours";
/// The variable the service names the window in, for a click to open it.
pub const WINDOW_VARIABLE: &str = "HELIX_TOOLBOX_WINDOW";

/// Where the timer and what was last told are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Places {
    /// The user's systemd units: `$XDG_CONFIG_HOME/systemd/user`.
    pub units: PathBuf,
    /// What the last notification told of:
    /// `$XDG_STATE_HOME/helix-toolbox/notified.txt`.
    pub told: PathBuf,
}

impl Places {
    pub fn here() -> Option<Self> {
        let config = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        Some(Places {
            units: config.join("systemd/user"),
            told: history::state_dir()?.join("notified.txt"),
        })
    }

    /// The link that enables the timer, as `systemctl --user enable` makes it.
    pub fn enabled_link(&self) -> PathBuf {
        self.units.join("timers.target.wants").join(TIMER)
    }
}

/// The window a click opens: the one the service names, else
/// `helix-toolbox-gui` on the PATH.
pub fn window_here() -> Option<PathBuf> {
    env::var_os(WINDOW_VARIABLE)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute() && path.is_file())
        .or_else(|| crate::inventory::which("helix-toolbox-gui"))
}

/// Whether the timer is enabled.
pub fn is_on(places: &Places) -> bool {
    places.enabled_link().symlink_metadata().is_ok()
}

/// A path or value quoted for a unit file: systemd reads `%` and `$` itself.
fn quoted(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
        .replace('$', "$$");
    format!("\"{escaped}\"")
}

/// The service the timer starts: `program notifications check`, with the
/// window a click opens, if there is one.
pub fn service_unit(program: &Path, window: Option<&Path>) -> String {
    let mut text = String::from(
        "# Written by Helix Toolbox: helix-toolbox notifications on.\n\
         # helix-toolbox notifications off removes it.\n\
         [Unit]\n\
         Description=Helix Toolbox: tell me when updates are waiting\n\
         Documentation=https://github.com/Commanderx-code/Helix-toolbox/blob/main/docs/GUIDE.md\n\
         \n\
         [Service]\n\
         Type=oneshot\n",
    );
    text.push_str(&format!(
        "ExecStart={} notifications check\n",
        quoted(&program.to_string_lossy())
    ));
    if let Some(window) = window {
        text.push_str(&format!(
            "Environment={}\n",
            quoted(&format!("{WINDOW_VARIABLE}={}", window.to_string_lossy()))
        ));
    }
    text.push_str(
        "# The check waits a while for a click on the notification; the window\n\
         # a click opens outlives it.\n\
         KillMode=process\n\
         TimeoutStartSec=45min\n\
         Nice=10\n",
    );
    text
}

/// The timer: soon after logging in, then every six hours.
pub fn timer_unit() -> String {
    "# Written by Helix Toolbox: helix-toolbox notifications on.\n\
     # helix-toolbox notifications off removes it.\n\
     [Unit]\n\
     Description=Helix Toolbox: check for waiting updates every 6 hours\n\
     \n\
     [Timer]\n\
     OnStartupSec=10min\n\
     OnUnitActiveSec=6h\n\
     RandomizedDelaySec=10min\n\
     \n\
     [Install]\n\
     WantedBy=timers.target\n"
        .to_string()
}

/// What turning on did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Started {
    /// The timer runs now.
    Now,
    /// The files are in place, but systemd's user manager did not answer;
    /// the timer starts at the next login.
    AtLogin,
}

/// Write the units, enable the timer and start it. `program` is the command
/// line the service runs; `window`, the window a click opens. `systemctl`
/// runs `systemctl --user` with these arguments and says whether it worked;
/// None when there is no systemctl at all.
pub fn turn_on(
    places: &Places,
    program: &Path,
    window: Option<&Path>,
    systemctl: &dyn Fn(&[&str]) -> Option<bool>,
) -> Result<Started, String> {
    if !program.is_absolute() {
        return Err(format!(
            "the command line is not installed where a timer can run it ({})",
            program.display()
        ));
    }
    // Ask first: a machine without systemd gets no files it cannot use.
    if systemctl(&["--version"]).is_none() {
        return Err("this machine has no systemd, which runs the check".to_string());
    }
    let wants = places.units.join("timers.target.wants");
    fs::create_dir_all(&wants)
        .map_err(|error| format!("cannot make {}: {error}", wants.display()))?;
    for (name, text) in [
        (SERVICE, service_unit(program, window)),
        (TIMER, timer_unit()),
    ] {
        let path = places.units.join(name);
        fs::write(&path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    let link = places.enabled_link();
    if link.symlink_metadata().is_ok() {
        fs::remove_file(&link)
            .map_err(|error| format!("cannot replace {}: {error}", link.display()))?;
    }
    std::os::unix::fs::symlink(places.units.join(TIMER), &link)
        .map_err(|error| format!("cannot enable the timer: {error}"))?;
    let answered =
        systemctl(&["daemon-reload"]) == Some(true) && systemctl(&["start", TIMER]) == Some(true);
    Ok(if answered {
        Started::Now
    } else {
        Started::AtLogin
    })
}

/// `systemctl --user` with these arguments on this machine: whether it
/// worked, or None when there is no systemctl to run.
pub fn systemctl(args: &[&str]) -> Option<bool> {
    std::process::Command::new("systemctl")
        .arg("--user")
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()
        .map(|status| status.success())
}

/// Stop the timer and remove its files and what was last told.
pub fn turn_off(
    places: &Places,
    systemctl: &dyn Fn(&[&str]) -> Option<bool>,
) -> Result<(), String> {
    // A timer that is not running, or no user manager, is no reason to stop.
    let _ = systemctl(&["stop", TIMER]);
    for path in [
        places.enabled_link(),
        places.units.join(TIMER),
        places.units.join(SERVICE),
        places.told.clone(),
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

/// What waits, counted by channel.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Waiting {
    /// Each channel with updates, and how many.
    pub channels: Vec<(String, usize)>,
    /// One line per update, sorted: what was told, to compare with.
    pub lines: Vec<String>,
}

impl Waiting {
    pub fn total(&self) -> usize {
        self.channels.iter().map(|(_, count)| count).sum()
    }
}

/// What waits in an overview; channels that cannot count are left out.
pub fn waiting(channels: &[Channel]) -> Waiting {
    let mut found = Waiting::default();
    for channel in channels {
        let Some(pending) = channel
            .pending
            .as_ref()
            .filter(|pending| !pending.is_empty())
        else {
            continue;
        };
        found.channels.push((channel.name.clone(), pending.len()));
        for update in pending {
            found.lines.push(format!(
                "{} {} {}",
                channel.id,
                update.name,
                update.to.as_deref().unwrap_or("-")
            ));
        }
    }
    found.lines.sort();
    found
}

/// Whether what waits is news: something waits, and it is not what the
/// last notification told of.
pub fn is_news(places: &Places, waiting: &Waiting) -> bool {
    if waiting.total() == 0 {
        return false;
    }
    let told = fs::read_to_string(&places.told).unwrap_or_default();
    told.lines().collect::<Vec<_>>() != waiting.lines
}

/// Remember what was told; nothing waiting forgets it, so the next update
/// is told of again.
pub fn remember(places: &Places, waiting: &Waiting) -> std::io::Result<()> {
    if waiting.total() == 0 {
        return match fs::remove_file(&places.told) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    if let Some(parent) = places.told.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut text = waiting.lines.join("\n");
    text.push('\n');
    fs::write(&places.told, text)
}

/// The notification's title and text.
pub fn message(waiting: &Waiting, window: bool) -> (String, String) {
    let total = waiting.total();
    let title = if total == 1 {
        "1 update is waiting".to_string()
    } else {
        format!("{total} updates are waiting")
    };
    let counts = waiting
        .channels
        .iter()
        .map(|(name, count)| format!("{name}: {count}"))
        .collect::<Vec<_>>()
        .join(", ");
    let see = if window {
        "Show them in Helix Toolbox's Updates, or run helix-toolbox updates."
    } else {
        "Run helix-toolbox updates to see them."
    };
    (title, format!("{counts}.\n{see}"))
}

/// How this desktop is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notifier {
    /// libnotify's notify-send; `actions` when it can offer a button and
    /// wait for the click (libnotify 0.7.10 and later).
    NotifySend { actions: bool },
    /// GLib's gdbus, straight to the notification service; no button.
    Gdbus,
}

/// The way to notify here: notify-send, else gdbus. `help` is
/// `notify-send --help`'s output, when notify-send is on the PATH.
pub fn notifier(help: Option<&str>, gdbus: bool) -> Option<Notifier> {
    match help {
        Some(help) => Some(Notifier::NotifySend {
            actions: help.contains("--action") && help.contains("--wait"),
        }),
        None if gdbus => Some(Notifier::Gdbus),
        None => None,
    }
}

/// The action a click on the notification answers with.
pub const SHOW: &str = "show";

/// A GVariant text string, as gdbus reads its arguments.
fn variant_string(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

/// The command that shows the notification. With `button`, it offers "Show
/// updates" and waits: it prints [`SHOW`] when that, or the notification
/// itself, is clicked.
pub fn command(notifier: Notifier, title: &str, body: &str, button: bool) -> Vec<String> {
    match notifier {
        Notifier::NotifySend { actions } => {
            let mut command = vec![
                "notify-send".to_string(),
                "--app-name=Helix Toolbox".to_string(),
                "--icon=helix-toolbox".to_string(),
                "--category=system".to_string(),
            ];
            if button && actions {
                command.push("--action=default=Show updates".to_string());
                command.push(format!("--action={SHOW}=Show updates"));
                command.push("--wait".to_string());
            }
            command.push("--".to_string());
            command.push(title.to_string());
            command.push(body.to_string());
            command
        }
        Notifier::Gdbus => [
            "gdbus",
            "call",
            "--session",
            "--dest=org.freedesktop.Notifications",
            "--object-path=/org/freedesktop/Notifications",
            "--method=org.freedesktop.Notifications.Notify",
            "'Helix Toolbox'",
            "0",
            "'helix-toolbox'",
        ]
        .into_iter()
        .map(String::from)
        .chain([
            variant_string(title),
            variant_string(body),
            "[]".to_string(),
            "{}".to_string(),
            "-1".to_string(),
        ])
        .collect(),
    }
}

/// Whether notify-send's answer is a click to show the updates.
pub fn clicked(output: &str) -> bool {
    output
        .lines()
        .any(|line| matches!(line.trim(), SHOW | "default"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::updates::Pending;
    use std::cell::RefCell;

    fn places(root: &Path) -> Places {
        Places {
            units: root.join("config/systemd/user"),
            told: root.join("state/helix-toolbox/notified.txt"),
        }
    }

    fn channel(id: &str, name: &str, pending: Option<&[(&str, &str)]>) -> Channel {
        Channel {
            id: id.to_string(),
            name: name.to_string(),
            pending: pending.map(|pending| {
                pending
                    .iter()
                    .map(|(name, to)| Pending {
                        name: name.to_string(),
                        from: None,
                        to: Some(to.to_string()),
                    })
                    .collect()
            }),
            note: String::new(),
            commands: Vec::new(),
            requires: Vec::new(),
            caveat: None,
        }
    }

    #[test]
    fn turning_on_writes_and_enables_the_timer_and_off_removes_it() {
        let directory = tempfile::tempdir().unwrap();
        let places = places(directory.path());
        let asked = RefCell::new(Vec::new());
        let systemctl = |args: &[&str]| {
            asked.borrow_mut().push(args.join(" "));
            Some(true)
        };
        assert!(!is_on(&places));
        let started = turn_on(
            &places,
            Path::new("/usr/bin/helix-toolbox"),
            Some(Path::new("/opt/Helix Toolbox/helix-toolbox-gui.AppImage")),
            &systemctl,
        )
        .unwrap();
        assert_eq!(started, Started::Now);
        assert!(is_on(&places));
        assert_eq!(
            fs::read_link(places.enabled_link()).unwrap(),
            places.units.join(TIMER)
        );
        let service = fs::read_to_string(places.units.join(SERVICE)).unwrap();
        assert!(
            service.contains("ExecStart=\"/usr/bin/helix-toolbox\" notifications check\n"),
            "{service}"
        );
        assert!(
            service.contains(
                "Environment=\"HELIX_TOOLBOX_WINDOW=/opt/Helix Toolbox/helix-toolbox-gui.AppImage\"\n"
            ),
            "{service}"
        );
        assert!(service.contains("KillMode=process"), "{service}");
        let timer = fs::read_to_string(places.units.join(TIMER)).unwrap();
        assert!(timer.contains("OnUnitActiveSec=6h"), "{timer}");
        assert!(timer.contains("WantedBy=timers.target"), "{timer}");
        assert_eq!(
            asked.borrow().as_slice(),
            [
                "--version",
                "daemon-reload",
                "start helix-toolbox-updates.timer"
            ]
        );
        // On again replaces the files, as after an update moved the program.
        turn_on(
            &places,
            Path::new("/usr/local/bin/helix-toolbox"),
            None,
            &systemctl,
        )
        .unwrap();
        let service = fs::read_to_string(places.units.join(SERVICE)).unwrap();
        assert!(
            service.contains("\"/usr/local/bin/helix-toolbox\""),
            "{service}"
        );
        assert!(!service.contains("Environment="), "{service}");

        remember(
            &places,
            &waiting(&[channel("pacman", "pacman", Some(&[("git", "2")]))]),
        )
        .unwrap();
        asked.borrow_mut().clear();
        turn_off(&places, &systemctl).unwrap();
        assert!(!is_on(&places));
        assert!(!places.units.join(SERVICE).exists());
        assert!(!places.units.join(TIMER).exists());
        assert!(!places.told.exists());
        assert_eq!(
            asked.borrow().as_slice(),
            ["stop helix-toolbox-updates.timer", "daemon-reload"]
        );
        // Off when already off is fine.
        turn_off(&places, &systemctl).unwrap();
    }

    #[test]
    fn without_systemd_nothing_is_written_and_a_silent_manager_waits_for_login() {
        let directory = tempfile::tempdir().unwrap();
        let places = places(directory.path());
        let program = Path::new("/usr/bin/helix-toolbox");
        let error = turn_on(&places, program, None, &|_| None).unwrap_err();
        assert!(error.contains("no systemd"), "{error}");
        assert!(!places.units.exists());
        let error =
            turn_on(&places, Path::new("helix-toolbox"), None, &|_| Some(true)).unwrap_err();
        assert!(error.contains("not installed"), "{error}");
        // systemctl is there but the user manager does not answer, as over
        // some remote logins: the timer is enabled for the next login.
        let started = turn_on(&places, program, None, &|args| Some(args == ["--version"])).unwrap();
        assert_eq!(started, Started::AtLogin);
        assert!(is_on(&places));
    }

    #[test]
    fn unit_values_are_quoted_for_systemd() {
        let service = service_unit(Path::new("/home/me/100%/$bin/\"x\"/helix-toolbox"), None);
        assert!(
            service.contains(
                "ExecStart=\"/home/me/100%%/$$bin/\\\"x\\\"/helix-toolbox\" notifications check"
            ),
            "{service}"
        );
    }

    #[test]
    fn only_new_updates_are_news_and_nothing_waiting_forgets() {
        let directory = tempfile::tempdir().unwrap();
        let places = places(directory.path());
        let channels = [
            channel(
                "pacman",
                "pacman",
                Some(&[("linux", "6.18"), ("git", "2.51")]),
            ),
            channel(
                "flatpak-user",
                "Flatpak (yours)",
                Some(&[("org.gimp.GIMP", "3.0")]),
            ),
            channel("aur", "AUR (paru)", None),
            channel("nix-profile", "Nix profile", Some(&[])),
        ];
        let found = waiting(&channels);
        assert_eq!(found.total(), 3);
        assert_eq!(
            found.channels,
            [
                ("pacman".to_string(), 2),
                ("Flatpak (yours)".to_string(), 1)
            ]
        );
        assert!(is_news(&places, &found));
        remember(&places, &found).unwrap();
        assert!(!is_news(&places, &found), "told once is enough");
        // A newer version of one is news again.
        let newer = waiting(&[channel("pacman", "pacman", Some(&[("git", "2.52")]))]);
        assert!(is_news(&places, &newer));
        // Nothing waiting is never news, and forgets what was told.
        let none = waiting(&[channel("pacman", "pacman", Some(&[]))]);
        assert!(!is_news(&places, &none));
        remember(&places, &none).unwrap();
        assert!(!places.told.exists());
        assert!(is_news(&places, &found));
    }

    #[test]
    fn the_message_counts_by_channel() {
        let found = waiting(&[
            channel(
                "pacman",
                "pacman",
                Some(&[("linux", "6.18"), ("git", "2.51")]),
            ),
            channel(
                "flatpak-user",
                "Flatpak (yours)",
                Some(&[("org.gimp.GIMP", "3.0")]),
            ),
        ]);
        let (title, body) = message(&found, true);
        assert_eq!(title, "3 updates are waiting");
        assert_eq!(
            body,
            "pacman: 2, Flatpak (yours): 1.\nShow them in Helix Toolbox's Updates, or run helix-toolbox updates."
        );
        let one = waiting(&[channel("apt", "APT", Some(&[("curl", "8")]))]);
        let (title, body) = message(&one, false);
        assert_eq!(title, "1 update is waiting");
        assert!(
            body.ends_with("Run helix-toolbox updates to see them."),
            "{body}"
        );
    }

    #[test]
    fn notify_send_offers_the_button_only_where_it_can_wait() {
        let new = notifier(Some("  -A, --action=[NAME=]Text...\n  -w, --wait\n"), true).unwrap();
        assert_eq!(new, Notifier::NotifySend { actions: true });
        let command = command(new, "3 updates are waiting", "pacman: 3.", true);
        assert!(
            command.contains(&"--action=show=Show updates".to_string()),
            "{command:?}"
        );
        assert!(command.contains(&"--wait".to_string()), "{command:?}");
        assert_eq!(
            command[command.len() - 3..],
            ["--", "3 updates are waiting", "pacman: 3."]
        );
        // Older libnotify, or no window to open: a plain notification.
        let old = notifier(Some("  -u, --urgency=LEVEL\n"), true).unwrap();
        assert!(!super::command(old, "t", "b", true).contains(&"--wait".to_string()));
        assert!(!super::command(new, "t", "b", false).contains(&"--wait".to_string()));
        assert!(clicked("show\n"));
        assert!(clicked("default\n"));
        assert!(!clicked(""));
    }

    #[test]
    fn gdbus_is_the_fallback_with_its_strings_quoted() {
        assert_eq!(notifier(None, true), Some(Notifier::Gdbus));
        assert_eq!(notifier(None, false), None);
        let command = command(Notifier::Gdbus, "1 update is waiting", "it's\\here", true);
        assert_eq!(command[0], "gdbus");
        assert!(
            command.contains(&"'1 update is waiting'".to_string()),
            "{command:?}"
        );
        assert!(
            command.contains(&"'it\\'s\\\\here'".to_string()),
            "{command:?}"
        );
        assert_eq!(command.last().unwrap(), "-1");
    }
}
