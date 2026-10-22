//! `helix-toolbox notifications`: the desktop notifications of what the
//! scheduled check finds (updates waiting, and when chosen new health
//! problems and Drift), turned on and off, and the check its timer runs
//! (helix_toolbox_engine::notifications and watch). The check also records
//! the boot, for the Performance screen's comparisons.

use anyhow::{Context, Result, bail};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_engine::{
    doctor::{self, System},
    drift as engine_drift, history,
    inventory::which,
    notifications::{self, Places, Started},
    performance, settings, updates,
    watch::{self, Watch},
};
use helix_toolbox_platform::LinuxInfo;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// How long a notification with a button waits for its click.
const CLICK_WAIT: Duration = Duration::from_secs(30 * 60);

fn places() -> Result<Places> {
    Places::here().context("there is no home folder to keep the timer in")
}

fn settings_path() -> Result<PathBuf> {
    settings::path().context("there is no home folder to keep the choices in")
}

pub fn status() -> Result<()> {
    let places = places()?;
    let watch = watch::read(&places, settings::path().as_deref());
    if !watch.any() {
        println!(
            "Notifications: off. helix-toolbox notifications on tells of updates; health on and drift <baseline> tell of more."
        );
        return Ok(());
    }
    println!(
        "Notifications: on, checking {} ({}).",
        notifications::EVERY,
        notifications::TIMER
    );
    let yes = |on: bool| if on { "yes" } else { "no" };
    println!("  Updates waiting: {}", yes(watch.updates));
    println!("  New health problems: {}", yes(watch.health));
    match &watch.drift {
        Some(slug) => println!("  Drift from the baseline: {slug}"),
        None => println!("  Drift: no"),
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

/// Change what is watched, and the timer with it.
pub fn change(how: impl FnOnce(&mut Watch)) -> Result<()> {
    let places = places()?;
    let path = settings_path()?;
    let mut watch = watch::read(&places, Some(&path));
    how(&mut watch);
    let program = std::env::current_exe().context("cannot tell where this program is")?;
    let window = notifications::window_here();
    let started = watch::apply(
        &places,
        &path,
        &watch,
        &program,
        window.as_deref(),
        &notifications::systemctl,
    )
    .map_err(anyhow::Error::msg)?;
    let what: Vec<String> = [
        watch.updates.then(|| "updates waiting".to_string()),
        watch.health.then(|| "new health problems".to_string()),
        watch
            .drift
            .as_ref()
            .map(|slug| format!("changes since the baseline {slug}")),
    ]
    .into_iter()
    .flatten()
    .collect();
    match started {
        _ if what.is_empty() => println!("Notifications are off."),
        Some(Started::AtLogin) => println!(
            "Notifications are on, for {}. systemd's user manager did not answer, so the checks start at your next login.",
            what.join(", ")
        ),
        _ => println!(
            "Helix Toolbox checks {}, and soon after you log in, and tells you of {}. It never updates, repairs or puts back anything itself.",
            notifications::EVERY,
            what.join(", ")
        ),
    }
    Ok(())
}

pub fn off() -> Result<()> {
    change(|watch| *watch = Watch::default())
}

/// What the timer runs: look at what is watched, and tell of what is new.
pub fn check(catalog: &Catalog, platform: &LinuxInfo) -> Result<()> {
    let places = places()?;
    let watch = watch::read(&places, settings::path().as_deref());
    // Each boot the timer sees is recorded, so boots can be compared before
    // and after a change without the Performance screen being opened.
    if let (Some(state), Ok(boot)) = (history::state_dir(), performance::boot(&doctor::Here)) {
        performance::record(&performance::boots_path(&state), &boot);
    }
    let mut problems = Vec::new();
    // A timer from before the choices, or with them lost, still tells of
    // updates, as it always did.
    if (watch.updates || !watch.any())
        && let Err(error) = check_updates(&places, platform)
    {
        problems.push(error);
    }
    if watch.health
        && let Err(error) = check_health(&places, platform)
    {
        problems.push(error);
    }
    if let Some(slug) = &watch.drift
        && let Err(error) = check_drift(&places, catalog, platform, slug)
    {
        problems.push(error);
    }
    match problems.len() {
        0 => Ok(()),
        1 => Err(problems.remove(0)),
        _ => bail!(
            "{}",
            problems
                .iter()
                .map(|error| error.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        ),
    }
}

fn check_updates(places: &Places, platform: &LinuxInfo) -> Result<()> {
    let channels = updates::overview(&doctor::Here, platform, &crate::this_places(), true);
    let found = notifications::waiting(&channels);
    if !notifications::is_news(places, &found) {
        if found.total() == 0 {
            notifications::remember(places, &found)?;
            println!("Nothing is waiting.");
        } else {
            println!(
                "{} updates are waiting, as told before; nothing new to tell.",
                found.total()
            );
        }
        return Ok(());
    }
    let window = notifications::window_here();
    let (title, body) = notifications::message(&found, window.is_some());
    // Told is told, even when the wait for a click is cut short.
    notifications::remember(places, &found)?;
    if let Err(error) = tell(&title, &body, window, "--updates") {
        notifications::remember(places, &notifications::Waiting::default())?;
        return Err(error);
    }
    Ok(())
}

fn check_health(places: &Places, platform: &LinuxInfo) -> Result<()> {
    let found = doctor::examine(&doctor::Here, platform.manager);
    let told_path = watch::health_told(places);
    let before: BTreeMap<String, String> = watch::load(&told_path);
    let news = watch::health_news(&before, &found);
    watch::store(&told_path, &watch::health_state(&found))?;
    if news.is_empty() {
        println!("No new health problems.");
        return Ok(());
    }
    let window = notifications::window_here();
    let (title, body) = watch::health_message(&news, window.is_some());
    tell(&title, &body, window, "--doctor")
}

fn check_drift(places: &Places, catalog: &Catalog, platform: &LinuxInfo, slug: &str) -> Result<()> {
    let directory = engine_drift::dir().context("cannot find the configuration folder")?;
    let Some((_, baseline)) = engine_drift::find(&directory, slug) else {
        bail!(
            "the baseline {slug} is gone; helix-toolbox notifications drift <baseline> chooses another"
        );
    };
    let report = crate::drift::compare(catalog, platform, &baseline);
    let told_path = watch::drift_told(places);
    let before: BTreeSet<String> = watch::load(&told_path);
    let (news, now) = watch::drift_news(&before, &report);
    watch::store(&told_path, &now)?;
    if news.is_empty() {
        println!("No new changes since {}.", report.against);
        return Ok(());
    }
    let window = notifications::window_here();
    let (title, body) = watch::drift_message(&report.against, &news, window.is_some());
    tell(&title, &body, window, "--drift")
}

/// Show one notification; a click on its button opens the window on
/// `screen` (`--updates`, `--doctor`, `--drift`).
fn tell(title: &str, body: &str, window: Option<PathBuf>, screen: &str) -> Result<()> {
    let help = which("notify-send")
        .and_then(|_| doctor::Here.run(&["notify-send", "--help"]))
        .map(|(_, text)| text);
    let Some(notifier) = notifications::notifier(help.as_deref(), which("gdbus").is_some()) else {
        bail!(
            "neither notify-send (libnotify) nor gdbus (GLib) is installed to show the notification"
        );
    };
    let command = notifications::command(notifier, title, body, window.is_some());
    println!("{title}");
    if !command.iter().any(|arg| arg == "--wait") {
        let status = Command::new(&command[0])
            .args(&command[1..])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .status()
            .with_context(|| format!("cannot run {}", command[0]))?;
        if !status.success() {
            bail!("{} could not show the notification", command[0]);
        }
        return Ok(());
    }
    if clicked_within(&command, CLICK_WAIT)?
        && let Some(window) = window
    {
        Command::new(&window)
            .arg(screen)
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
