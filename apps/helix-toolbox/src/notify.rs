//! `helix-toolbox notifications`: the desktop notification when updates
//! wait, turned on and off, and the check its timer runs
//! (helix_toolbox_engine::notifications).

use anyhow::{Context, Result, bail};
use helix_toolbox_engine::{
    doctor::{self, System},
    inventory::which,
    notifications::{self, Places, Started},
    updates,
};
use helix_toolbox_platform::LinuxInfo;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// How long a notification with a button waits for its click.
const CLICK_WAIT: Duration = Duration::from_secs(30 * 60);

fn places() -> Result<Places> {
    Places::here().context("there is no home folder to keep the timer in")
}

pub fn status() -> Result<()> {
    let places = places()?;
    if notifications::is_on(&places) {
        println!(
            "Update notifications: on, checking {} ({}).",
            notifications::EVERY,
            notifications::TIMER
        );
    } else {
        println!("Update notifications: off. helix-toolbox notifications on turns them on.");
    }
    if let Ok(told) = std::fs::read_to_string(&places.told) {
        let when = std::fs::metadata(&places.told)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|elapsed| format!(", on {}", crate::when(elapsed.as_secs())))
            .unwrap_or_default();
        println!("Last told of: {} updates{when}.", told.lines().count());
    }
    Ok(())
}

pub fn on() -> Result<()> {
    let places = places()?;
    let program = std::env::current_exe().context("cannot tell where this program is")?;
    let window = notifications::window_here();
    match notifications::turn_on(
        &places,
        &program,
        window.as_deref(),
        &notifications::systemctl,
    )
    .map_err(anyhow::Error::msg)?
    {
        Started::Now => println!(
            "Update notifications are on: Helix Toolbox checks {}, and soon after you log in, and tells you when updates are waiting. It never updates anything itself.",
            notifications::EVERY
        ),
        Started::AtLogin => println!(
            "Update notifications are on. systemd's user manager did not answer, so the checks start at your next login."
        ),
    }
    Ok(())
}

pub fn off() -> Result<()> {
    notifications::turn_off(&places()?, &notifications::systemctl).map_err(anyhow::Error::msg)?;
    println!("Update notifications are off.");
    Ok(())
}

/// What the timer runs: look, and tell of what is new.
pub fn check(platform: &LinuxInfo) -> Result<()> {
    let places = places()?;
    let channels = updates::overview(&doctor::Here, platform, &crate::this_places(), true);
    let found = notifications::waiting(&channels);
    if !notifications::is_news(&places, &found) {
        if found.total() == 0 {
            notifications::remember(&places, &found)?;
            println!("Nothing is waiting.");
        } else {
            println!(
                "{} updates are waiting, as told before; nothing new to tell.",
                found.total()
            );
        }
        return Ok(());
    }
    let help = which("notify-send")
        .and_then(|_| doctor::Here.run(&["notify-send", "--help"]))
        .map(|(_, text)| text);
    let Some(notifier) = notifications::notifier(help.as_deref(), which("gdbus").is_some()) else {
        bail!(
            "neither notify-send (libnotify) nor gdbus (GLib) is installed to show the notification"
        );
    };
    let window = notifications::window_here();
    let (title, body) = notifications::message(&found, window.is_some());
    let command = notifications::command(notifier, &title, &body, window.is_some());
    // Told is told, even when the wait for a click is cut short.
    notifications::remember(&places, &found)?;
    println!("{title}");
    if !command.iter().any(|arg| arg == "--wait") {
        let status = Command::new(&command[0])
            .args(&command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", command[0]))?;
        if !status.success() {
            notifications::remember(&places, &notifications::Waiting::default())?;
            bail!("{} could not show the notification", command[0]);
        }
        return Ok(());
    }
    if clicked_within(&command, CLICK_WAIT)?
        && let Some(window) = window
    {
        Command::new(&window)
            .arg("--updates")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("cannot open {}", window.display()))?;
    }
    Ok(())
}

/// Show a notification that waits, and say whether it was clicked before
/// `wait` ran out; it is closed then.
fn clicked_within(command: &[String], wait: Duration) -> Result<bool> {
    let mut child = Command::new(&command[0])
        .args(&command[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot run {}", command[0]))?;
    let mut stdout = child.stdout.take().context("no output to read")?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + wait;
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    let text = reader.join().unwrap_or_default();
    Ok(notifications::clicked(&text))
}
