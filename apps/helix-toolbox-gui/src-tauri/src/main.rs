//! Helix Toolbox in a window. The frontend asks these commands for the
//! catalog and how entries stand, has the engine plan and check what the
//! user picked, and runs it with the engine's execution events, drawn in a
//! terminal panel. Every change to the machine is the engine's, as in the
//! terminal interface.

mod views;

use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, printable};
use helix_toolbox_engine::{
    Plan, backups, bundle, configs,
    confirmation::{self, Prepared},
    desktop, doctor, drift,
    execution::{Execution, Options},
    hardware, hardware_report, history, home_manager,
    inventory::{Inventory, which},
    migration::{self, Standing},
    notifications,
    overview::Overview,
    performance, plan, plan_removal, preflight, preview, profiles, recovery,
    reliability::{self, Evidence},
    repositories, restic, services,
    settings::{self, RunChoices, WindowTheme},
    setup, storage, trust, updates, verification, watch, workspaces,
};
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, RwLock},
};
use tauri::{AppHandle, Manager, State, ipc::Channel};
use tauri_plugin_dialog::DialogExt;
use views::{
    AreaView, BackupView, BundleContentsView, BundleView, ChangesView, ChannelView, DiagnosisView,
    PreparedView, RecordView, StatusView,
};

const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the window works with.
struct Gui {
    /// Written only when a profile is saved, which adds it to Profiles.
    catalog: RwLock<Catalog>,
    platform: LinuxInfo,
    /// The command-line program, which native steps run again.
    this: PathBuf,
    /// Where `settings.toml` is, when there is a home.
    settings: Option<PathBuf>,
    favorites: Mutex<Vec<String>>,
    /// The plan last confirmed, waiting for its run.
    pending: Mutex<Option<Plan>>,
    /// The run in the terminal panel, if any.
    running: Mutex<Option<Execution>>,
    /// What the last status check read, for planning updates and drift.
    inventory: Mutex<Option<Inventory>>,
    /// What the screens last showed, for the actions on their rows.
    diagnoses: Mutex<Vec<doctor::Diagnosis>>,
    channels: Mutex<Vec<updates::Channel>>,
    backups: Mutex<Vec<backups::Backup>>,
    hardware: Mutex<Option<hardware::Hardware>>,
    /// The screen to open on: `--updates`, `--doctor` or `--drift`, as a
    /// click on a notification starts the window.
    opens: Option<&'static str>,
}

/// The command line beside this program, else on the PATH.
fn command_line(this: Option<&Path>) -> PathBuf {
    let beside = this
        .and_then(Path::parent)
        .map(|directory| directory.join("helix-toolbox"));
    beside
        .filter(|path| path.is_file())
        .or_else(|| which("helix-toolbox"))
        .unwrap_or_else(|| PathBuf::from("helix-toolbox"))
}

/// `--catalog-dir DIR` reads a catalog from a directory, as the command
/// line's option does; tests and catalog work use it.
fn catalog_dir(args: &[String]) -> Option<PathBuf> {
    args.iter()
        .position(|arg| arg == "--catalog-dir")
        .and_then(|index| args.get(index + 1))
        .map(PathBuf::from)
}

fn load_catalog(directory: Option<&Path>) -> Result<Catalog, String> {
    let mut catalog = match directory {
        Some(directory) => Catalog::from_directory(directory, true),
        None => Catalog::builtin(true),
    }
    .map_err(|error| format!("cannot load the catalog: {error}"))?;
    // The user's own profiles join the Profiles area, as on the command line.
    if let Some(directory) = profiles::dir() {
        for problem in profiles::add_to(&mut catalog, &directory) {
            eprintln!("Profile skipped: {problem}");
        }
    }
    Ok(catalog)
}

/// Run `work` off the window's thread, with the window's state.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&Gui) -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let gui = app.state::<Gui>();
        work(&gui)
    })
    .await
    .map_err(|error| error.to_string())
}

/// Facts about this program and the machine, for the header and dashboard.
#[derive(Serialize)]
struct About {
    version: &'static str,
    overview: Overview,
    /// Whether the command line was found, which native steps need.
    command_line: bool,
    /// The first start on this machine: the setup wizard opens by itself.
    first_run: bool,
    theme: WindowTheme,
    /// The screen to open on (updates, doctor or drift): the window was
    /// started from a notification.
    opens: Option<&'static str>,
}

#[tauri::command]
fn about(gui: State<Gui>) -> About {
    About {
        version: VERSION,
        overview: Overview::gather(&gui.platform),
        command_line: gui.this.is_absolute(),
        first_run: gui
            .settings
            .as_deref()
            .is_some_and(|path| !settings::set_up(path)),
        theme: theme(&gui),
        opens: gui.opens,
    }
}

/// The window's theme, as saved.
fn theme(gui: &Gui) -> WindowTheme {
    gui.settings
        .as_deref()
        .map(settings::window_theme)
        .unwrap_or_default()
}

#[tauri::command]
fn areas(gui: State<Gui>) -> Vec<AreaView> {
    views::areas(&gui.catalog.read().unwrap())
}

/// How each package entry stands, by id. It asks the package managers, so
/// it runs off the window's thread; the snapshot it reads is kept for
/// planning updates and comparing drift.
#[tauri::command]
async fn statuses(app: AppHandle) -> Result<HashMap<String, StatusView>, String> {
    blocking(app, |gui| {
        let catalog = gui.catalog.read().unwrap();
        let inventory = Inventory::gather(&gui.platform);
        let statuses = catalog
            .entries()
            .into_iter()
            .filter(|entry| entry.identity().is_some())
            .filter_map(|entry| {
                inventory
                    .status(entry, &gui.platform, &which)
                    .map(|status| (entry.id.clone(), views::status(&status)))
            })
            .collect();
        *gui.inventory.lock().unwrap() = Some(inventory);
        statuses
    })
    .await
}

#[tauri::command]
fn favorites(gui: State<Gui>) -> Vec<String> {
    gui.favorites.lock().unwrap().clone()
}

/// Pin an entry or unpin it; the pins as they are now.
#[tauri::command]
fn pin(gui: State<Gui>, id: String, pinned: bool) -> Result<Vec<String>, String> {
    let mut favorites = gui.favorites.lock().unwrap();
    favorites.retain(|other| *other != id);
    if pinned {
        favorites.push(id);
    }
    if let Some(path) = &gui.settings {
        settings::save_favorites(&settings::favorites_path(path), &favorites)
            .map_err(|error| format!("the favorites could not be saved: {error}"))?;
    }
    Ok(favorites.clone())
}

/// A plan of these entries, as the catalog knows them.
fn plan_entries(gui: &Gui, catalog: &Catalog, ids: &[String]) -> Result<Plan, String> {
    let lookup = |id: &str| catalog.find(id);
    let mut entries = Vec::new();
    for id in ids {
        entries.push(lookup(id).ok_or_else(|| format!("There is no entry {id} in the catalog."))?);
    }
    plan(entries, &gui.platform, &lookup).map_err(|error| error.to_string())
}

/// Get a plan ready to confirm, as the terminal interface does before it
/// asks: the snapshot, the checks before a run, the lines to show. A plan
/// ready to run waits for `run`.
fn prepare_plan(gui: &Gui, catalog: &Catalog, planned: Plan) -> PreparedView {
    let run_choices = gui
        .settings
        .as_deref()
        .map(settings::run_choices)
        .unwrap_or_default();
    let choices = confirmation::Choices {
        snapshot: backups::Snapshots::here().filter(|_| run_choices.snapshots),
        keep_going: run_choices.keep_going,
    };
    let prepared = confirmation::prepare(
        planned,
        &gui.platform,
        &|id| catalog.find(id),
        choices,
        &preflight::Facts::real(),
        &Evidence::load(catalog.root()),
    );
    let view = views::prepared(&prepared);
    *gui.pending.lock().unwrap() = match prepared {
        Prepared::Ready(ready) => Some(ready.plan),
        _ => None,
    };
    view
}

/// A plan that could not be made.
fn cannot(gui: &Gui, why: String) -> PreparedView {
    gui.pending.lock().unwrap().take();
    PreparedView::Error { lines: vec![why] }
}

/// Plan these entries and check the plan.
#[tauri::command]
async fn prepare(app: AppHandle, ids: Vec<String>) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        match plan_entries(gui, &catalog, &ids) {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// The plan waiting for confirmation.
fn waiting(gui: &Gui) -> Result<Plan, String> {
    gui.pending
        .lock()
        .unwrap()
        .clone()
        .ok_or_else(|| "Nothing is waiting to run; review the plan again.".to_string())
}

/// What a plan is expected to change on this machine; it only reads.
fn preview_changes(gui: &Gui, catalog: &Catalog, planned: &Plan) -> ChangesView {
    let scripts = catalog.root().join("scripts");
    let render = |arguments: &[String]| preview::render_with(&scripts, arguments);
    let pinned = |name: &str| preview::pinned_source(&scripts, name);
    let steps = preview::preview(
        planned,
        &|id| catalog.find(id),
        &gui.platform,
        &preview::Look {
            system: &doctor::Here,
            render: &render,
            pinned: &pinned,
        },
    );
    let download = preview::sources_for(planned)
        .into_iter()
        .any(|name| matches!(pinned(name), preview::Pinned::Missing { .. }));
    views::changes(&steps, download)
}

/// The change preview of the plan waiting for confirmation.
#[tauri::command]
async fn changes(app: AppHandle) -> Result<ChangesView, String> {
    blocking(app, |gui| {
        let planned = waiting(gui)?;
        let catalog = gui.catalog.read().unwrap();
        Ok(preview_changes(gui, &catalog, &planned))
    })
    .await?
}

/// Download the pinned sources the waiting plan reads, which changes
/// nothing else, then preview its changes again.
#[tauri::command]
async fn download_sources(app: AppHandle) -> Result<ChangesView, String> {
    blocking(app, |gui| {
        let planned = waiting(gui)?;
        let catalog = gui.catalog.read().unwrap();
        let scripts = catalog.root().join("scripts");
        for name in preview::sources_for(&planned) {
            preview::fetch_source(&scripts, name)
                .map_err(|why| format!("{name} could not be downloaded: {why}"))?;
        }
        Ok(preview_changes(gui, &catalog, &planned))
    })
    .await?
}

/// Forget a plan the user did not confirm.
#[tauri::command]
fn cancel(gui: State<Gui>) {
    gui.pending.lock().unwrap().take();
}

/// Run the confirmed plan in a terminal of this size, sending its events
/// to `events` until the terminal closes.
#[tauri::command]
fn run(gui: State<Gui>, rows: u16, cols: u16, events: Channel<Value>) -> Result<(), String> {
    let mut running = gui.running.lock().unwrap();
    if running.as_ref().is_some_and(|run| !run.is_finished()) {
        return Err("A run is still going; it has to end first.".to_string());
    }
    let planned = gui
        .pending
        .lock()
        .unwrap()
        .take()
        .ok_or("Nothing is waiting to run; review the plan again.")?;
    // The run before, if the window did not close it, is recorded first.
    if let Some(before) = running.take() {
        let _ = before.record(None);
    }
    let options = Options {
        rows: rows.max(2),
        cols: cols.max(20),
        ..Options::default()
    };
    let lib_dir = gui.catalog.read().unwrap().lib_dir();
    let (execution, received) = Execution::start(&planned, &lib_dir, &gui.this, options)
        .map_err(|error| error.to_string())?;
    *running = Some(execution);
    std::thread::spawn(move || {
        for event in received {
            if events.send(views::event(&event)).is_err() {
                // The window went away; the run goes on and is recorded.
                break;
            }
        }
    });
    Ok(())
}

/// What the user typed in the terminal panel.
#[tauri::command]
fn write(gui: State<Gui>, data: String) {
    if let Some(run) = gui.running.lock().unwrap().as_ref() {
        run.write(data.as_bytes());
    }
}

#[tauri::command]
fn resize(gui: State<Gui>, rows: u16, cols: u16) {
    if let Some(run) = gui.running.lock().unwrap().as_ref() {
        run.resize(rows.max(2), cols.max(20));
    }
}

/// Hang the run up; a second call, while anything is left, kills it.
#[tauri::command]
fn stop(gui: State<Gui>) {
    if let Some(run) = gui.running.lock().unwrap().as_ref() {
        run.stop();
    }
}

/// The terminal panel closed: record the run in the history.
#[tauri::command]
fn close_run(gui: State<Gui>) -> Result<(), String> {
    let Some(run) = gui.running.lock().unwrap().take() else {
        return Ok(());
    };
    if !run.is_finished() {
        run.stop();
    }
    run.record(None)
        .map_err(|error| format!("the run could not be recorded: {error}"))
}

#[tauri::command]
fn runs() -> Result<Vec<RecordView>, String> {
    history::read(200)
        .map(|records| records.iter().rev().map(views::record).collect())
        .map_err(|error| format!("the history could not be read: {error}"))
}

/// Save the Install Plan as a profile, which joins the Profiles area; what
/// to tell the user.
#[tauri::command]
fn save_profile(gui: State<Gui>, name: String, ids: Vec<String>) -> Result<String, String> {
    let mut catalog = gui.catalog.write().unwrap();
    let entries: Vec<Entry> = ids
        .iter()
        .filter_map(|id| catalog.find(id).cloned())
        .collect();
    if entries.is_empty() {
        return Err("The Install Plan is empty.".to_string());
    }
    let profile = profiles::from_queue(&name, &entries);
    let directory = profiles::dir().ok_or("cannot find the configuration folder")?;
    let path = profiles::save(&directory, &profile, &catalog).map_err(|error| error.to_string())?;
    profiles::add_to(&mut catalog, &directory);
    Ok(format!(
        "{} ({} entries) is saved as {}. It is under Profiles, Your profiles. To share it: helix-toolbox profile export \"{}\".",
        profile.name,
        profile.entries.len(),
        path.display(),
        profile.name
    ))
}

/// One of the user's own profiles, by its catalog id: its file and what it
/// holds.
fn own_profile(catalog: &Catalog, id: &str) -> Result<(PathBuf, profiles::Profile), String> {
    let directory = profiles::dir().ok_or("cannot find the configuration folder")?;
    let slug = id
        .strip_prefix(profiles::PREFIX)
        .and_then(profiles::slug)
        .filter(|slug| id == format!("{}{slug}", profiles::PREFIX))
        .ok_or_else(|| format!("{} is not one of your profiles", printable(id)))?;
    let path = directory.join(format!("{slug}.toml"));
    let profile = profiles::read(&path, catalog).map_err(|error| error.to_string())?;
    Ok((path, profile))
}

/// The file chooser, over the window: a profile file to read, or where to
/// write one. None when it was closed.
fn choose_profile_file(app: &AppHandle, save_as: Option<String>) -> Option<PathBuf> {
    let title = match save_as {
        Some(_) => "Export the profile",
        None => "Import a profile",
    };
    choose_toml(app, "Helix Toolbox profile", title, save_as)
}

/// A TOML file chosen in the desktop's file chooser, starting in the home
/// folder: one to write under `save_as`, or one to read. None when the
/// chooser was closed.
fn choose_toml(
    app: &AppHandle,
    filter: &str,
    title: &str,
    save_as: Option<String>,
) -> Option<PathBuf> {
    let mut chooser = app
        .dialog()
        .file()
        .add_filter(filter, &["toml"])
        .set_title(title);
    if let Some(window) = app.get_webview_window("main") {
        chooser = chooser.set_parent(&window);
    }
    if let Some(home) = std::env::var_os("HOME") {
        chooser = chooser.set_directory(home);
    }
    let chosen = match save_as {
        Some(name) => chooser.set_file_name(name).blocking_save_file(),
        None => chooser.blocking_pick_file(),
    };
    chosen.and_then(|chosen| chosen.into_path().ok())
}

/// Add a profile file to the user's profiles, as `helix-toolbox profile
/// import` does: from `path`, or a file chosen in the file chooser. None
/// when the chooser was closed.
#[tauri::command]
async fn import_profile(app: AppHandle, path: Option<String>) -> Result<Option<String>, String> {
    let handle = app.clone();
    blocking(app, move |gui| {
        let Some(path) = path
            .map(PathBuf::from)
            .or_else(|| choose_profile_file(&handle, None))
        else {
            return Ok(None);
        };
        let mut catalog = gui.catalog.write().unwrap();
        let profile = profiles::read(&path, &catalog).map_err(|error| error.to_string())?;
        let directory = profiles::dir().ok_or("cannot find the configuration folder")?;
        let saved =
            profiles::save(&directory, &profile, &catalog).map_err(|error| error.to_string())?;
        profiles::add_to(&mut catalog, &directory);
        Ok(Some(format!(
            "{} ({} entries) is imported as {}. It is under Profiles, Your profiles.",
            printable(&profile.name),
            profile.entries.len(),
            saved.display()
        )))
    })
    .await?
}

/// Write one of the user's profiles to a file to share, as `helix-toolbox
/// profile export` does: to `path`, or where the file chooser says. None
/// when the chooser was closed.
#[tauri::command]
async fn export_profile(
    app: AppHandle,
    id: String,
    path: Option<String>,
) -> Result<Option<String>, String> {
    let handle = app.clone();
    blocking(app, move |gui| {
        let (own, profile) = own_profile(&gui.catalog.read().unwrap(), &id)?;
        let file_name = own.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let Some(path) = path
            .map(PathBuf::from)
            .or_else(|| choose_profile_file(&handle, Some(file_name)))
        else {
            return Ok(None);
        };
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A profile is written to a .toml file.".to_string());
        }
        profiles::write(&path, &profile).map_err(|error| error.to_string())?;
        Ok(Some(format!(
            "{} is exported to {}. On another machine: Import a profile in Profiles, or helix-toolbox profile import.",
            printable(&profile.name),
            path.display()
        )))
    })
    .await?
}

/// Delete one of the user's profiles, as `helix-toolbox profile delete` does.
#[tauri::command]
fn delete_profile(gui: State<Gui>, id: String) -> Result<String, String> {
    let mut catalog = gui.catalog.write().unwrap();
    let (path, profile) = own_profile(&catalog, &id)?;
    std::fs::remove_file(&path)
        .map_err(|error| format!("cannot delete {}: {error}", path.display()))?;
    if let Some(directory) = profiles::dir() {
        profiles::add_to(&mut catalog, &directory);
    }
    Ok(format!("{} is deleted.", printable(&profile.name)))
}

// Reporting a problem

/// How the window was installed, from where it runs, as the command line's
/// report says it of itself.
fn window_installed_from(exe: &Path) -> &'static str {
    if std::env::var_os("APPIMAGE").is_some() {
        "AppImage"
    } else if exe.starts_with("/usr/bin") || exe.starts_with("/usr/sbin") {
        "package"
    } else if exe.components().any(|part| part.as_os_str() == "target") {
        "source build"
    } else {
        "other"
    }
}

/// What a bug report needs, to paste into an issue: `helix-toolbox report`,
/// then the window's own facts. It only reads, and shows the home folder
/// as `~`.
#[tauri::command]
async fn bug_report(app: AppHandle) -> Result<String, String> {
    blocking(app, |gui| {
        if !gui.this.is_absolute() {
            return Err(
                "The helix-toolbox command was not found beside this program or on the PATH; the report comes from it."
                    .to_string(),
            );
        }
        let output = std::process::Command::new(&gui.this)
            .arg("report")
            .output()
            .map_err(|error| format!("{} report could not run: {error}", gui.this.display()))?;
        if !output.status.success() {
            return Err(format!(
                "{} report failed: {}",
                gui.this.display(),
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let variable = |name: &str| {
            std::env::var(name)
                .ok()
                .filter(|value| !value.is_empty())
                .map(|value| printable(&value))
                .unwrap_or_else(|| "unknown".to_string())
        };
        let exe = std::env::current_exe().unwrap_or_default();
        let theme = serde_json::to_value(theme(gui))
            .ok()
            .and_then(|value| value.as_str().map(str::to_string))
            .unwrap_or_default();
        let window = format!(
            "\n### The window\n\n- Helix Toolbox window: {}\n- Installed from: {}\n- WebKitGTK: {}\n- Desktop: {}, {} session\n- Theme: {theme}\n",
            env!("CARGO_PKG_VERSION"),
            window_installed_from(&exe),
            tauri::webview_version().unwrap_or_else(|_| "unknown".to_string()),
            variable("XDG_CURRENT_DESKTOP"),
            variable("XDG_SESSION_TYPE"),
        );
        Ok(without_home(&format!(
            "{}{window}",
            String::from_utf8_lossy(&output.stdout).trim_end()
        )))
    })
    .await?
}

/// Put text on the desktop's clipboard, through GTK on the window's thread:
/// a page may write there only from a real click.
#[tauri::command]
fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.run_on_main_thread(move || {
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_text(&text);
    })
    .map_err(|error| error.to_string())
}

/// The home folder as `~`, as in the command line's report.
fn without_home(text: &str) -> String {
    match std::env::var("HOME")
        .ok()
        .map(|home| home.trim_end_matches('/').to_string())
        .filter(|home| home.len() > 1)
    {
        Some(home) => text.replace(&home, "~"),
        None => text.to_string(),
    }
}

// Hardware reports

#[derive(Serialize)]
struct HardwareReportView {
    text: String,
    /// The new issue, filled in from the report.
    url: String,
}

/// A report that a run worked on this machine: the run at `at` (in Unix
/// seconds), or else the last successful run of `entry`; of the run's
/// entries, only `entry` when it is given.
#[tauri::command]
async fn hardware_report(
    app: AppHandle,
    at: Option<u64>,
    entry: Option<String>,
) -> Result<HardwareReportView, String> {
    blocking(app, move |gui| {
        let runs = history::read(1000)
            .map_err(|error| format!("the history could not be read: {error}"))?;
        let record = match (at, &entry) {
            (Some(at), _) => runs.iter().find(|record| record.unix_timestamp == at),
            (None, Some(id)) => hardware_report::last_success(&runs, id),
            (None, None) => None,
        }
        .ok_or("That run is not in this machine's history.")?;
        let machine = hardware_report::machine(&doctor::Here, &gui.platform, &desktop_here());
        let report = hardware_report::prepare(
            record,
            entry.as_deref(),
            machine,
            env!("CARGO_PKG_VERSION"),
            "",
        )?;
        Ok(HardwareReportView {
            text: report.text(),
            url: report.issue_url(),
        })
    })
    .await?
}

/// Open a hardware report's issue in the desktop's browser.
#[tauri::command]
fn open_issue(url: String) -> Result<(), String> {
    if !url.starts_with(&format!("{}?", hardware_report::ISSUES)) {
        return Err("That is not a link to a Helix Toolbox issue.".to_string());
    }
    let mut command = std::process::Command::new("xdg-open");
    command
        .arg(&url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if let Some(appdir) = std::env::var_os("APPDIR").filter(|appdir| appdir.len() > 1) {
        let appdir = appdir.to_string_lossy().into_owned();
        for (name, value) in std::env::vars_os() {
            match outside(&value.to_string_lossy(), &appdir) {
                Outside::Keep => {}
                Outside::Set(value) => {
                    command.env(name, value);
                }
                Outside::Remove => {
                    command.env_remove(name);
                }
            }
        }
        // The AppImage's start sets its own theme for its own GTK.
        for name in ["APPDIR", "APPIMAGE", "ARGV0", "OWD", "GTK_THEME"] {
            command.env_remove(name);
        }
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("xdg-open could not run: {error}"))?;
    // Reaped when it exits.
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
enum Outside {
    Keep,
    Set(String),
    Remove,
}

/// A variable as a program of the desktop's should see it, outside the
/// AppImage the window runs from: paths into it taken out of a list, and
/// a single path into it removed.
fn outside(value: &str, appdir: &str) -> Outside {
    if !value.contains(appdir) {
        return Outside::Keep;
    }
    let kept: Vec<&str> = value
        .split(':')
        .filter(|part| !part.is_empty() && !part.starts_with(appdir))
        .collect();
    if kept.is_empty() || !value.contains(':') {
        Outside::Remove
    } else {
        Outside::Set(kept.join(":"))
    }
}

// Doctor

/// The health checks, run on this machine.
#[tauri::command]
async fn doctor(app: AppHandle) -> Result<Vec<DiagnosisView>, String> {
    blocking(app, |gui| {
        let found = doctor::examine(&doctor::Here, gui.platform.manager);
        let views = found.iter().map(views::diagnosis).collect();
        *gui.diagnoses.lock().unwrap() = found;
        views
    })
    .await
}

/// A check's repair, or the entry that addresses it, ready to confirm.
#[tauri::command]
async fn prepare_repair(app: AppHandle, id: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let diagnosis = gui
            .diagnoses
            .lock()
            .unwrap()
            .iter()
            .find(|diagnosis| diagnosis.id == id)
            .cloned();
        let Some(diagnosis) = diagnosis else {
            return cannot(
                gui,
                "That check is no longer shown; run the checks again.".into(),
            );
        };
        let catalog = gui.catalog.read().unwrap();
        if let Some(repair) = &diagnosis.repair {
            prepare_plan(gui, &catalog, repair.plan(&diagnosis.id))
        } else if let Some(entry) = diagnosis.entry.as_deref() {
            match plan_entries(gui, &catalog, &[entry.to_string()]) {
                Ok(planned) => prepare_plan(gui, &catalog, planned),
                Err(why) => cannot(gui, why),
            }
        } else {
            cannot(gui, "This check has no repair to run.".into())
        }
    })
    .await
}

// Updates

/// Where this copy of Helix Toolbox is, for the Updates screen's own row.
fn places(gui: &Gui) -> updates::Places {
    // Run as an AppImage, the window knows its file from the variable the
    // AppImage runtime sets, and its update replaces that file.
    updates::Places::here()
        .with_this(VERSION, gui.this.clone())
        .with_window(std::env::var_os("APPIMAGE").map(std::path::PathBuf::from))
}

#[derive(Serialize)]
struct UpdatesView {
    online: bool,
    channels: Vec<ChannelView>,
}

/// What every package tool has waiting; `online` asks the network.
#[tauri::command]
async fn updates(app: AppHandle, online: bool) -> Result<UpdatesView, String> {
    blocking(app, move |gui| {
        let channels = updates::overview(&doctor::Here, &gui.platform, &places(gui), online);
        let views = channels.iter().map(views::channel).collect();
        *gui.channels.lock().unwrap() = channels;
        UpdatesView {
            online,
            channels: views,
        }
    })
    .await
}

/// Update these channels, one after another, ready to confirm.
#[tauri::command]
async fn prepare_channels(app: AppHandle, ids: Vec<String>) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let channels = gui.channels.lock().unwrap();
        let chosen: Vec<&updates::Channel> = channels
            .iter()
            .filter(|channel| ids.contains(&channel.id) && !channel.commands.is_empty())
            .collect();
        if chosen.is_empty() {
            return cannot(gui, "There is nothing to run for those channels.".into());
        }
        let planned = updates::plan(&chosen);
        let catalog = gui.catalog.read().unwrap();
        prepare_plan(gui, &catalog, planned)
    })
    .await
}

/// Update these package entries, from the last status check.
#[tauri::command]
async fn prepare_update(app: AppHandle, ids: Vec<String>) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let inventory = gui.inventory.lock().unwrap();
        let Some(inventory) = inventory.as_ref() else {
            return cannot(
                gui,
                "Helix Toolbox is still checking what is installed; try again in a moment.".into(),
            );
        };
        let catalog = gui.catalog.read().unwrap();
        let entries: Vec<&Entry> = ids.iter().filter_map(|id| catalog.find(id)).collect();
        let planned = inventory.plan_update(entries, &gui.platform, &which);
        prepare_plan(gui, &catalog, planned)
    })
    .await
}

/// Remove these package entries, through the package manager or Flatpak
/// that installed them, as the terminal interface's `r` does.
#[tauri::command]
async fn prepare_remove(app: AppHandle, ids: Vec<String>) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let Some(entries) = ids
            .iter()
            .map(|id| catalog.find(id))
            .collect::<Option<Vec<&Entry>>>()
        else {
            return cannot(gui, "That entry is not in the catalog.".into());
        };
        match plan_removal(entries, &gui.platform) {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(error) => cannot(gui, error.to_string()),
        }
    })
    .await
}

// Backups

#[derive(Serialize)]
struct BackupsView {
    /// The snapshot tool set up here, and how to restore with it.
    snapshots: Option<SnapshotsView>,
    backups: Vec<BackupView>,
}

#[derive(Serialize)]
struct SnapshotsView {
    tool: &'static str,
    restore: &'static str,
}

/// The backups found beside configuration files, newest first.
#[tauri::command]
async fn list_backups(app: AppHandle) -> Result<BackupsView, String> {
    blocking(app, |gui| {
        let found = backups::discover(&backups::Roots::here());
        let views = found
            .iter()
            .enumerate()
            .map(|(index, backup)| views::backup(index, backup))
            .collect();
        *gui.backups.lock().unwrap() = found;
        BackupsView {
            snapshots: backups::Snapshots::here().map(|tool| SnapshotsView {
                tool: tool.label(),
                restore: tool.how_to_restore(),
            }),
            backups: views,
        }
    })
    .await
}

/// Put a backup back, ready to confirm; the current file is moved aside
/// as its own backup first.
#[tauri::command]
async fn prepare_restore(app: AppHandle, index: usize) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let backup = gui.backups.lock().unwrap().get(index).cloned();
        let Some(backup) = backup else {
            return cannot(gui, "That backup is no longer listed; look again.".into());
        };
        let catalog = gui.catalog.read().unwrap();
        prepare_plan(gui, &catalog, backups::plan_restore(&backup))
    })
    .await
}

// Drift

/// What this machine can be compared with.
#[derive(Serialize)]
struct TargetView {
    /// `baseline:<slug>` or `profile:<id>`.
    key: String,
    kind: &'static str,
    name: String,
    detail: String,
}

#[tauri::command]
fn drift_targets(gui: State<Gui>) -> Vec<TargetView> {
    let mut targets: Vec<TargetView> = drift::dir()
        .map(|directory| drift::load_all(&directory).0)
        .unwrap_or_default()
        .into_iter()
        .map(|(slug, baseline)| TargetView {
            key: format!("baseline:{slug}"),
            kind: "baseline",
            name: helix_toolbox_core::printable(&baseline.name),
            detail: format!(
                "saved {} on {}: {} entries, {} configuration paths",
                helix_toolbox_engine::date(baseline.taken),
                helix_toolbox_core::printable(&baseline.system),
                baseline.entries.len(),
                baseline.files.len()
            ),
        })
        .collect();
    let catalog = gui.catalog.read().unwrap();
    if let Some(tab) = catalog.tabs.iter().find(|tab| tab.id == "profiles") {
        for entry in tab.entries() {
            if let Action::Profile { entries } = &entry.action {
                targets.push(TargetView {
                    key: format!("profile:{}", entry.id),
                    kind: "profile",
                    name: entry.name.clone(),
                    detail: format!("a profile of {} entries", entries.len()),
                });
            }
        }
    }
    targets
}

/// What a comparison reads from this machine, beyond the inventory.
struct Here {
    home: PathBuf,
    records: Vec<history::Record>,
    settings: std::collections::BTreeMap<String, String>,
    scripts: PathBuf,
}

impl Here {
    fn gather(catalog: &Catalog) -> Self {
        Here {
            home: std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default(),
            records: history::read(1000).unwrap_or_default(),
            settings: drift::settings_here(&doctor::Here),
            scripts: catalog.root().join("scripts"),
        }
    }

    fn sources<'a>(
        &'a self,
        gui: &'a Gui,
        catalog: &'a Catalog,
        inventory: &'a Inventory,
        settings: &'a dyn Fn() -> std::collections::BTreeMap<String, String>,
        render: &'a dyn Fn(&[String]) -> Result<String, String>,
    ) -> drift::Sources<'a> {
        drift::Sources {
            catalog,
            platform: &gui.platform,
            inventory,
            on_path: &which,
            system: &doctor::Here,
            home: &self.home,
            history: &self.records,
            settings,
            render,
        }
    }
}

#[derive(Serialize)]
struct DriftView {
    report: drift::Report,
    /// Entries that would put back what is missing, which this machine can
    /// plan; `prepare` runs them.
    restore: Vec<String>,
    unavailable: Vec<String>,
}

/// Compare this machine with a target, from the last status check.
#[tauri::command]
async fn drift_check(app: AppHandle, key: String) -> Result<DriftView, String> {
    blocking(app, move |gui| {
        let inventory = gui.inventory.lock().unwrap();
        let inventory = inventory
            .as_ref()
            .ok_or("Helix Toolbox is still checking what is installed; try again in a moment.")?;
        let catalog = gui.catalog.read().unwrap();
        let here = Here::gather(&catalog);
        let settings = || here.settings.clone();
        let render = |arguments: &[String]| {
            helix_toolbox_engine::preview::render_with(&here.scripts, arguments)
        };
        let sources = here.sources(gui, &catalog, inventory, &settings, &render);
        let report = match key.split_once(':') {
            Some(("baseline", slug)) => {
                let directory = drift::dir().ok_or("cannot find the configuration folder")?;
                let (_, baseline) = drift::load_all(&directory)
                    .0
                    .into_iter()
                    .find(|(known, _)| known == slug)
                    .ok_or("That baseline is no longer saved.")?;
                drift::compare(&baseline, &sources)
            }
            Some(("profile", id)) => {
                let entry = catalog
                    .find(id)
                    .ok_or("That profile is no longer listed.")?;
                let Action::Profile { entries } = &entry.action else {
                    return Err("That is not a profile.".to_string());
                };
                drift::compare_profile(&entry.name, entries, &sources)
            }
            _ => return Err("Choose a baseline or a profile.".to_string()),
        };
        let (restore, unavailable) = drift::available(&report.restore(), &catalog, &gui.platform);
        Ok(DriftView {
            report,
            restore,
            unavailable,
        })
    })
    .await?
}

/// Record this machine as a baseline; what to tell the user.
#[tauri::command]
async fn save_baseline(app: AppHandle, name: String) -> Result<String, String> {
    blocking(app, move |gui| {
        let inventory = gui.inventory.lock().unwrap();
        let inventory = inventory
            .as_ref()
            .ok_or("Helix Toolbox is still checking what is installed; try again in a moment.")?;
        let catalog = gui.catalog.read().unwrap();
        let here = Here::gather(&catalog);
        let settings = || here.settings.clone();
        let render = |arguments: &[String]| {
            helix_toolbox_engine::preview::render_with(&here.scripts, arguments)
        };
        let sources = here.sources(gui, &catalog, inventory, &settings, &render);
        let baseline = drift::capture(&name, history::now(), &sources, &[]);
        let directory = drift::dir().ok_or("cannot find the configuration folder")?;
        let path = drift::save(&directory, &baseline).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} is saved as {}: {} entries, {} configuration paths, {} settings and {} runs. To compare another machine with it: helix-toolbox drift export \"{}\", then helix-toolbox drift check <file> there.",
            baseline.name,
            path.display(),
            baseline.entries.len(),
            baseline.files.len(),
            baseline.settings.len(),
            baseline.applied.len(),
            baseline.name
        ))
    })
    .await?
}

// Reliability

#[derive(Serialize)]
struct ReliabilityView {
    /// What the record covers, in a sentence.
    summary: String,
    problems: Vec<String>,
    rows: Vec<ReliabilityRow>,
}

#[derive(Serialize)]
struct ReliabilityRow {
    id: String,
    name: String,
    level: &'static str,
    explain: &'static str,
    assessment: reliability::Assessment,
}

/// How far each entry has been tested, critical ones first.
#[tauri::command]
async fn reliability(app: AppHandle) -> Result<ReliabilityView, String> {
    blocking(app, |gui| {
        let catalog = gui.catalog.read().unwrap();
        let evidence = Evidence::load(catalog.root());
        let records = history::read(1000).unwrap_or_default();
        let rows = reliability::rows(&catalog, &evidence, &records)
            .into_iter()
            .map(|(entry, assessment)| ReliabilityRow {
                id: entry.id.clone(),
                name: entry.name.clone(),
                level: assessment.level.label(),
                explain: assessment.level.explain(),
                assessment,
            })
            .collect();
        ReliabilityView {
            summary: evidence.summary(),
            problems: evidence.problems.clone(),
            rows,
        }
    })
    .await
}

/// Where an entry's installer and packages come from, as text.
#[tauri::command]
async fn trust(app: AppHandle, id: String) -> Result<String, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let entry = catalog
            .find(&id)
            .ok_or_else(|| format!("There is no entry {id} in the catalog."))?;
        let build = trust::Build {
            version: VERSION.to_string(),
            release: false,
            commit: None,
            install: places(gui)
                .this
                .map(|this| this.install)
                .unwrap_or(updates::Install::Other),
        };
        let found = trust::inspect(
            entry,
            catalog.root(),
            &gui.platform,
            &build,
            &doctor::Here,
            &Evidence::load(catalog.root()),
            &history::read(1000).unwrap_or_default(),
        );
        Ok(trust::text(&found))
    })
    .await?
}

/// Check an entry now, read-only, as `helix-toolbox verify` does: what it
/// installs and its checks of files, settings and services.
#[tauri::command]
async fn check_entry(app: AppHandle, id: String) -> Result<String, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let entry = catalog
            .find(&id)
            .ok_or_else(|| format!("There is no entry {id} in the catalog."))?;
        let records = history::read(1000).unwrap_or_default();
        let rebooted = verification::rebooted_since(&doctor::Here, &records, &entry.id);
        let checks = verification::check_entry(
            entry,
            &gui.platform,
            rebooted,
            &helix_toolbox_engine::Probe::holds,
            &helix_toolbox_platform::binary_exists,
            &doctor::Here,
        );
        if checks.is_empty() {
            return Ok(format!(
                "{} declares no checks and installs nothing Helix Toolbox can find, so there is nothing to check.",
                entry.name
            ));
        }
        let failed = checks.iter().filter(|check| !check.passed).count();
        let mut text = match failed {
            0 => "Everything that can be checked here is as it should be.\n\n".to_string(),
            1 => "One check found it not as it should be.\n\n".to_string(),
            count => format!("{count} checks found it not as it should be.\n\n"),
        };
        for check in &checks {
            text.push_str(&format!("{}: {}\n  {}\n", check.label(), check.name, check.detail));
        }
        Ok(text)
    })
    .await?
}

// Services and startup

/// The machine's and the session's units and what starts at login.
#[tauri::command]
async fn read_services(app: AppHandle) -> Result<services::Services, String> {
    blocking(app, |_| {
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        services::gather(&doctor::Here, desktop.as_deref())
    })
    .await
}

#[derive(Serialize)]
struct ServiceDetailsView {
    needed_by: Vec<String>,
    /// What stopping or disabling it costs, when the machine or the
    /// session needs it.
    consequence: Option<&'static str>,
}

#[tauri::command]
async fn service_details(
    app: AppHandle,
    name: String,
    user: bool,
) -> Result<ServiceDetailsView, String> {
    blocking(app, move |_| ServiceDetailsView {
        needed_by: services::needed_by(&doctor::Here, &name, user),
        consequence: services::consequence(
            &name,
            user,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ),
    })
    .await
}

/// Plan starting, stopping, restarting, enabling or disabling a unit.
#[tauri::command]
async fn prepare_service(
    app: AppHandle,
    name: String,
    user: bool,
    verb: String,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let Some(verb) = services::Verb::parse(&verb) else {
            return cannot(
                gui,
                format!("{} is not something a unit can do.", printable(&verb)),
            );
        };
        let needed = services::needed_by(&doctor::Here, &name, user);
        match services::plan(
            &name,
            user,
            verb,
            &needed,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ) {
            Ok(planned) => {
                let catalog = gui.catalog.read().unwrap();
                prepare_plan(gui, &catalog, planned)
            }
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// Plan turning an application started at login off, or on again.
#[tauri::command]
async fn prepare_autostart(app: AppHandle, file: String, on: bool) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let dirs = services::AutostartDirs::here(&doctor::Here);
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
        let found = services::autostart_in(&doctor::Here, &dirs, desktop.as_deref());
        let Some(item) = found.iter().find(|item| item.file == file) else {
            return cannot(
                gui,
                format!("There is no autostart file {}.", printable(&file)),
            );
        };
        match services::plan_autostart(item, on, &dirs, &doctor::Here) {
            Ok(planned) => {
                let catalog = gui.catalog.read().unwrap();
                prepare_plan(gui, &catalog, planned)
            }
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Storage

/// Where the space goes and what can be cleaned.
#[tauri::command]
async fn read_storage(app: AppHandle) -> Result<storage::Storage, String> {
    blocking(app, |gui| {
        storage::gather(&doctor::Here, gui.platform.manager)
    })
    .await
}

/// Plan cleaning one item, as it is found now.
#[tauri::command]
async fn prepare_clean(app: AppHandle, id: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let found = storage::gather(&doctor::Here, gui.platform.manager);
        let Some(item) = found.items.iter().find(|item| item.id == id) else {
            return cannot(
                gui,
                format!("{} is not something Helix Toolbox cleans.", printable(&id)),
            );
        };
        match storage::plan(item, gui.platform.manager, &doctor::Here) {
            Ok(planned) => {
                let catalog = gui.catalog.read().unwrap();
                prepare_plan(gui, &catalog, planned)
            }
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Performance

/// The boot, the machine now, and the boots recorded so far; this boot is
/// recorded on the way.
#[derive(Serialize)]
struct PerformanceView {
    found: performance::Performance,
    boots: Vec<performance::Boot>,
}

#[tauri::command]
async fn read_performance(app: AppHandle) -> Result<PerformanceView, String> {
    blocking(app, |_| {
        let state = history::state_dir();
        let history = history::read(1000).unwrap_or_default();
        let found = performance::gather(&doctor::Here, state.as_deref(), &history);
        let boots = state
            .map(|state| performance::load(&performance::boots_path(&state)))
            .unwrap_or_default();
        PerformanceView { found, boots }
    })
    .await
}

/// One recorded boot beside a later one, with the runs in between.
#[tauri::command]
async fn compare_boots(
    app: AppHandle,
    before: String,
    after: String,
) -> Result<performance::Comparison, String> {
    blocking(app, move |_| {
        let boots = history::state_dir()
            .map(|state| performance::load(&performance::boots_path(&state)))
            .unwrap_or_default();
        let find = |id: &str| boots.iter().find(|boot| boot.id == id);
        let (Some(before), Some(after)) = (find(&before), find(&after)) else {
            return Err("That boot is no longer recorded.".to_string());
        };
        let history = history::read(1000).unwrap_or_default();
        Ok(performance::compare(before, after, &history))
    })
    .await?
}

// Recovery

#[derive(Serialize)]
struct RecoveryView {
    checks: Vec<DiagnosisView>,
    /// The snapshot tool set up, whose snapshots can be checked.
    snapshots: Option<&'static str>,
}

#[tauri::command]
async fn read_recovery(app: AppHandle) -> Result<RecoveryView, String> {
    blocking(app, |gui| {
        let found_backups = backups::discover(&backups::Roots::here());
        let baselines = drift::dir()
            .map(|directory| drift::load_all(&directory).0)
            .unwrap_or_default();
        let found = recovery::examine(
            &doctor::Here,
            &recovery::Known {
                manager: gui.platform.manager,
                backups: &found_backups,
                baselines: &baselines,
                now: history::now(),
                restic: restic::repository_here(),
            },
        );
        RecoveryView {
            checks: found.iter().map(views::diagnosis).collect(),
            snapshots: recovery::snapshot_tool(&doctor::Here).map(|tool| tool.label()),
        }
    })
    .await
}

/// The snapshot check: it lists the snapshots as root and looks inside the
/// newest, changing nothing.
#[tauri::command]
async fn prepare_snapshot_check(app: AppHandle) -> Result<PreparedView, String> {
    blocking(app, |gui| {
        let Some(tool) = recovery::snapshot_tool(&doctor::Here) else {
            return cannot(
                gui,
                "Neither Snapper (with a root configuration) nor Timeshift is set up.".into(),
            );
        };
        let catalog = gui.catalog.read().unwrap();
        prepare_plan(gui, &catalog, recovery::plan_check(tool))
    })
    .await
}

// Migration

/// What a migration of this machine holds, as `helix-toolbox migrate
/// export` writes it.
#[derive(Serialize)]
struct MigrationExportView {
    migration: migration::Migration,
    /// The dated name the file chooser suggests.
    file_name: String,
    /// Whether the packages the user installed could be told apart.
    packages_known: bool,
}

#[tauri::command]
async fn read_migration_export(app: AppHandle) -> Result<MigrationExportView, String> {
    blocking(app, |gui| {
        let catalog = gui.catalog.read().unwrap();
        let found = migration::gather(&catalog.entries(), &gui.platform, history::now());
        MigrationExportView {
            file_name: migration::file_name(&found),
            migration: found,
            packages_known: migration::chosen(gui.platform.manager).is_some(),
        }
    })
    .await
}

/// Write this machine's migration to `path`, or where the file chooser
/// says, as `helix-toolbox migrate export` does. None when the chooser was
/// closed.
#[tauri::command]
async fn export_migration(app: AppHandle, path: Option<String>) -> Result<Option<String>, String> {
    let handle = app.clone();
    blocking(app, move |gui| {
        let found = {
            let catalog = gui.catalog.read().unwrap();
            migration::gather(&catalog.entries(), &gui.platform, history::now())
        };
        let Some(path) = path.map(PathBuf::from).or_else(|| {
            choose_toml(
                &handle,
                "Helix Toolbox migration",
                "Export this machine",
                Some(migration::file_name(&found)),
            )
        }) else {
            return Ok(None);
        };
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A migration is written to a .toml file.".to_string());
        }
        migration::write(&path, &found).map_err(|error| error.to_string())?;
        Ok(Some(format!(
            "{} entries, {} Flatpaks and {} packages of yours are written to {}. On the other machine: Open… in Migration, or helix-toolbox migrate show.",
            found.entries.len(),
            found.flatpaks.len(),
            found.packages.len(),
            path.display()
        )))
    })
    .await?
}

/// A migration file read on this machine, as `helix-toolbox migrate show`.
#[derive(Serialize)]
struct TranslationView {
    path: String,
    #[serde(flatten)]
    translation: migration::Translation,
    install: usize,
    here: usize,
    unavailable: usize,
    yours: usize,
}

/// Read a migration file from `path`, or one chosen in the file chooser.
/// None when the chooser was closed.
#[tauri::command]
async fn open_migration(
    app: AppHandle,
    path: Option<String>,
) -> Result<Option<TranslationView>, String> {
    let handle = app.clone();
    blocking(app, move |gui| {
        let Some(path) = path
            .map(PathBuf::from)
            .or_else(|| choose_toml(&handle, "Helix Toolbox migration", "Open a migration", None))
        else {
            return Ok(None);
        };
        let catalog = gui.catalog.read().unwrap();
        let translation = migration::read_here(&path, &catalog.entries(), &gui.platform)
            .map_err(|error| error.to_string())?;
        Ok(Some(TranslationView {
            path: path.display().to_string(),
            install: translation.count(Standing::Install),
            here: translation.count(Standing::Here),
            unavailable: translation.count(Standing::Unavailable),
            yours: translation.count(Standing::Yours),
            translation,
        }))
    })
    .await?
}

/// The plan that installs what a migration file marks Install, as
/// `helix-toolbox migrate apply` makes it.
#[tauri::command]
async fn prepare_migration(app: AppHandle, path: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let entries = catalog.entries();
        let planned = migration::read_here(Path::new(&path), &entries, &gui.platform)
            .map_err(|error| error.to_string())
            .and_then(|translation| {
                migration::plan_here(&translation, &entries, &gui.platform)
                    .map_err(|error| error.to_string())
            });
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Restic

/// The repository and its snapshots, as `helix-toolbox restic` shows them.
#[derive(Serialize)]
struct ResticView {
    configured: bool,
    repository: Option<String>,
    origin: Option<&'static str>,
    password: Option<String>,
    paths: Vec<String>,
    excludes: Vec<String>,
    /// Why the settings cannot be used, or restic could not read the
    /// repository.
    problem: Option<String>,
    snapshots: Vec<restic::Snapshot>,
    /// How often the timer backs up, when it is on.
    timer: Option<&'static str>,
    /// Whether this restic restores without overwriting (0.17 and later).
    restores: bool,
    baselines: Vec<restic::Baseline>,
    home: String,
}

fn restic_ready() -> Result<restic::Config, String> {
    match restic::here() {
        restic::Found::Ready(config) => Ok(config),
        restic::Found::Problem(why) => Err(why),
        restic::Found::Missing => Err("No restic repository is set up.".to_string()),
    }
}

fn restic_home() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "there is no home folder".to_string())
}

/// `~/` made the home folder.
fn typed_path(home: &Path, text: &str) -> PathBuf {
    match text.trim().strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(text.trim()),
    }
}

#[tauri::command]
async fn read_restic(app: AppHandle) -> Result<ResticView, String> {
    blocking(app, |_| {
        let found = restic::here();
        let mut view = ResticView {
            configured: false,
            repository: None,
            origin: None,
            password: None,
            paths: Vec::new(),
            excludes: Vec::new(),
            problem: None,
            snapshots: Vec::new(),
            timer: restic::units_here()
                .and_then(|units| restic::timer(&units))
                .map(restic::Every::label),
            restores: false,
            baselines: history::state_dir()
                .map(|state| restic::baselines(&state))
                .unwrap_or_default(),
            home: std::env::var("HOME").unwrap_or_default(),
        };
        match found {
            restic::Found::Ready(config) => {
                view.configured = true;
                view.repository = Some(config.repository.clone());
                view.origin = Some(config.origin.label());
                view.password = Some(config.password.describe());
                view.paths = config
                    .paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect();
                view.excludes = config.excludes.clone();
                match restic::snapshots(&restic::Here, &config) {
                    Ok(listed) => view.snapshots = listed,
                    Err(why) => {
                        view.problem = Some(format!("restic could not read the repository: {why}"))
                    }
                }
                view.restores = restic::restores_safely(&restic::Here);
            }
            restic::Found::Problem(why) => view.problem = Some(why),
            restic::Found::Missing => {}
        }
        view
    })
    .await
}

/// What a folder of a snapshot holds.
#[tauri::command]
async fn restic_files(
    app: AppHandle,
    snapshot: String,
    folder: String,
) -> Result<Vec<restic::Node>, String> {
    blocking(app, move |_| {
        restic::files(&restic::Here, &restic_ready()?, &snapshot, &folder)
    })
    .await?
}

/// Every version of the files a pattern matches.
#[tauri::command]
async fn restic_history(
    app: AppHandle,
    pattern: String,
    ignore_case: bool,
) -> Result<Vec<restic::History>, String> {
    blocking(app, move |_| {
        restic::history(
            &restic::Here,
            &restic_ready()?,
            &restic_home()?,
            &pattern,
            ignore_case,
        )
    })
    .await?
}

/// A restic plan: check, back up, restore into a new folder, or test a
/// restore against a baseline.
#[tauri::command]
async fn prepare_restic(
    app: AppHandle,
    action: String,
    snapshot: Option<String>,
    target: Option<String>,
    include: Option<String>,
    baseline: Option<String>,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = (|| -> Result<helix_toolbox_engine::Plan, String> {
            let config = restic_ready()?;
            let home = restic_home()?;
            match action.as_str() {
                "check" => Ok(restic::plan_check(&config)),
                "backup" => Ok(restic::plan_backup(&config)),
                "restore" => {
                    if !restic::restores_safely(&restic::Here) {
                        return Err("This restic cannot restore without overwriting; restic 0.17 or later can.".into());
                    }
                    let target = typed_path(&home, target.as_deref().unwrap_or_default());
                    let target = restic::check_target(&home, &target)?;
                    restic::plan_restore(
                        &config,
                        snapshot.as_deref().unwrap_or("latest"),
                        &target,
                        include.as_deref().filter(|path| !path.is_empty()),
                    )
                }
                "test" => {
                    let state = history::state_dir().ok_or("cannot find the state folder")?;
                    let all = restic::baselines(&state);
                    let chosen = restic::find_baseline(&all, baseline.as_deref().unwrap_or("1"))
                        .ok_or("That baseline is not there.")?;
                    restic::plan_test(&config, chosen, snapshot.as_deref().unwrap_or("latest"))
                }
                other => Err(format!("{} is not a restic plan", printable(other))),
            }
        })();
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// Record a file's SHA-256 as a baseline, as `helix-toolbox restic baseline`.
#[tauri::command]
async fn record_baseline(app: AppHandle, path: String) -> Result<restic::Baseline, String> {
    blocking(app, move |_| {
        let home = restic_home()?;
        let state = history::state_dir().ok_or("cannot find the state folder")?;
        restic::record(&state, &home, &typed_path(&home, &path), history::now())
    })
    .await?
}

/// Turn the timed backup on or off, as `helix-toolbox restic timer`.
#[tauri::command]
async fn set_restic_timer(app: AppHandle, on: bool, weekly: bool) -> Result<String, String> {
    blocking(app, move |gui| {
        let units = restic::units_here().ok_or("there is no home folder to keep the timer in")?;
        if !on {
            restic::timer_off(&units, &notifications::systemctl)?;
            return Ok("The timed backup is off.".to_string());
        }
        let every = if weekly {
            restic::Every::Week
        } else {
            restic::Every::Day
        };
        let started = restic::timer_on(
            &units,
            &gui.this,
            &restic_ready()?,
            every,
            &notifications::systemctl,
        )?;
        Ok(format!(
            "The backup runs {}{}.",
            every.label(),
            match started {
                notifications::Started::Now => "",
                notifications::Started::AtLogin => ", from the next login",
            }
        ))
    })
    .await?
}

// Configs

/// A field of a configuration, with its value now.
#[derive(Serialize)]
struct ConfigFieldView {
    key: &'static str,
    label: &'static str,
    hint: &'static str,
    value: Option<String>,
}

/// A backup kept beside a configuration.
#[derive(Serialize)]
struct ConfigBackupView {
    backup: PathBuf,
    taken: Option<String>,
}

/// One configuration as `helix-toolbox config <program> --json` shows it,
/// with its text to edit.
#[derive(Serialize)]
struct ConfigView {
    kind: configs::Kind,
    label: &'static str,
    /// Why it cannot be edited, when it cannot.
    problem: Option<String>,
    path: Option<PathBuf>,
    live: Option<PathBuf>,
    origin: Option<&'static str>,
    managed: bool,
    flake: Option<PathBuf>,
    exists: bool,
    revision: Option<String>,
    content: String,
    checked: Option<configs::Checked>,
    fields: Vec<ConfigFieldView>,
    backups: Vec<ConfigBackupView>,
}

/// Helix Toolbox's private folder for copies of configurations; never a
/// shared one.
fn copies_folder() -> Result<PathBuf, String> {
    configs::private_folder().ok_or_else(|| {
        "There is no state folder for Helix Toolbox's private copies; nothing is saved.".to_string()
    })
}

fn config_kind(kind: &str) -> Result<configs::Kind, String> {
    configs::Kind::from_id(kind)
        .ok_or_else(|| format!("{} is not a configuration here", printable(kind)))
}

fn fields_of(kind: configs::Kind, text: &str) -> Vec<ConfigFieldView> {
    let values = configs::values(kind, text);
    kind.fields()
        .iter()
        .map(|field| ConfigFieldView {
            key: field.key,
            label: field.label,
            hint: field.hint,
            value: values
                .iter()
                .find(|(key, _)| *key == field.key)
                .and_then(|(_, value)| value.clone()),
        })
        .collect()
}

fn config_view(kind: configs::Kind) -> ConfigView {
    let mut view = ConfigView {
        kind,
        label: kind.label(),
        problem: None,
        path: None,
        live: None,
        origin: None,
        managed: false,
        flake: None,
        exists: false,
        revision: None,
        content: String::new(),
        checked: None,
        fields: Vec::new(),
        backups: Vec::new(),
    };
    let located = match configs::here(kind) {
        Ok(located) => located,
        Err(why) => {
            view.problem = Some(why);
            return view;
        }
    };
    view.path = Some(located.path.clone());
    view.live = Some(located.live.clone());
    view.origin = Some(located.origin.label());
    view.managed = located.managed;
    view.flake = located.flake.clone();
    view.backups = configs::history(&located)
        .into_iter()
        .map(|backup| ConfigBackupView {
            backup: backup.backup,
            taken: backup.taken,
        })
        .collect();
    match configs::read(&located) {
        Ok(loaded) => {
            view.checked = Some(configs::validate(
                kind,
                &loaded.content,
                &configs::ghostty_here,
                copies_folder().ok().as_deref(),
            ));
            view.fields = fields_of(kind, &loaded.content);
            view.exists = loaded.exists;
            view.revision = Some(loaded.revision);
            view.content = loaded.content;
        }
        Err(why) => view.problem = Some(why),
    }
    view
}

/// Ghostty's and Fastfetch's configuration, as `helix-toolbox config` reads
/// them.
#[tauri::command]
async fn read_configs(app: AppHandle) -> Result<Vec<ConfigView>, String> {
    blocking(app, |_| {
        configs::Kind::ALL.into_iter().map(config_view).collect()
    })
    .await
}

/// What the checks say of a draft, before it is saved.
#[tauri::command]
async fn validate_config(
    app: AppHandle,
    kind: String,
    text: String,
) -> Result<configs::Checked, String> {
    blocking(app, move |_| {
        let kind = config_kind(&kind)?;
        Ok(configs::validate(
            kind,
            &text,
            &configs::ghostty_here,
            copies_folder().ok().as_deref(),
        ))
    })
    .await?
}

/// A field changed in a draft: the new text and the fields' values, not
/// saved yet.
#[derive(Serialize)]
struct ConfigDraftView {
    content: String,
    fields: Vec<ConfigFieldView>,
}

#[tauri::command]
fn set_config_field(
    kind: String,
    text: String,
    key: String,
    value: String,
) -> Result<ConfigDraftView, String> {
    let kind = config_kind(&kind)?;
    let content = configs::set(kind, &text, &key, &value)?;
    Ok(ConfigDraftView {
        fields: fields_of(kind, &content),
        content,
    })
}

/// The plan that saves a draft, as `helix-toolbox config` saves one: the
/// file must be the one read (`path`) and unchanged since (`revision`).
#[tauri::command]
async fn prepare_config_save(
    app: AppHandle,
    kind: String,
    path: String,
    revision: String,
    text: String,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = (|| -> Result<Plan, String> {
            let kind = config_kind(&kind)?;
            let located = configs::here(kind)?;
            if located.path != Path::new(&path) {
                return Err(
                    "Which file is edited changed since it was read; read it again.".into(),
                );
            }
            let folder = copies_folder()?;
            let checked = configs::validate(kind, &text, &configs::ghostty_here, Some(&folder));
            configs::plan_save(&located, &revision, &text, &checked, &folder)
        })();
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// A backup of a configuration put back, the file there now kept too:
/// only one of the backups kept beside it.
#[tauri::command]
async fn prepare_config_restore(
    app: AppHandle,
    kind: String,
    backup: String,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = (|| -> Result<Plan, String> {
            let located = configs::here(config_kind(&kind)?)?;
            let found = configs::history(&located)
                .into_iter()
                .find(|kept| kept.backup == Path::new(&backup))
                .ok_or("That backup is not kept beside the file.")?;
            Ok(backups::plan_restore(&found))
        })();
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Repositories

/// The repositories found, as `helix-toolbox repos --json` lists them.
#[derive(Serialize)]
struct ReposView {
    roots: Vec<PathBuf>,
    depth: usize,
    origin: String,
    repositories: Vec<RepoView>,
}

#[derive(Serialize)]
struct RepoView {
    #[serde(flatten)]
    summary: repositories::Summary,
    favorite: bool,
    group: String,
}

/// A changed file, with its state in words.
#[derive(Serialize)]
struct FileView {
    path: String,
    original: Option<String>,
    state: &'static str,
    staged: bool,
    unstaged: bool,
    conflict: bool,
}

/// One repository, as `helix-toolbox repos show --json`, with its tasks.
#[derive(Serialize)]
struct RepositoryView {
    #[serde(flatten)]
    details: repositories::Details,
    changes: Vec<FileView>,
    favorite: bool,
    tasks: Vec<workspaces::Task>,
}

/// The places repositories are looked for, and those found there.
fn repositories_here(gui: &Gui) -> (repositories::Places, Vec<PathBuf>) {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let places = repositories::places(gui.settings.as_deref().unwrap_or(Path::new("")), &home);
    let found = repositories::discover(&places);
    (places, found)
}

/// A repository the window was given, only when it is among those found.
fn repository(gui: &Gui, path: &str) -> Result<PathBuf, String> {
    let (_, found) = repositories_here(gui);
    let wanted = std::fs::canonicalize(path).map_err(|_| format!("no folder {path}"))?;
    found
        .into_iter()
        .find(|repo| *repo == wanted)
        .ok_or_else(|| format!("{path} is not among the repositories found"))
}

fn workspaces_file(gui: &Gui) -> Result<PathBuf, String> {
    gui.settings
        .as_ref()
        .map(|settings| settings.with_file_name("workspaces.toml"))
        .ok_or_else(|| "there is no configuration folder".to_string())
}

fn kept_workspaces(gui: &Gui) -> workspaces::Workspaces {
    workspaces_file(gui)
        .ok()
        .and_then(|file| workspaces::load(&file).ok())
        .unwrap_or_default()
}

#[tauri::command]
async fn read_repositories(app: AppHandle) -> Result<ReposView, String> {
    blocking(app, |gui| {
        let (places, found) = repositories_here(gui);
        let kept = kept_workspaces(gui);
        let mut summaries = repositories::summaries(&repositories::Here, &found);
        summaries.sort_by_key(|summary| !kept.favorite(&summary.path));
        ReposView {
            roots: places.roots,
            depth: places.depth,
            origin: places.origin,
            repositories: summaries
                .into_iter()
                .map(|summary| {
                    let project = kept.project(&summary.path);
                    RepoView {
                        favorite: project.is_some_and(|project| project.favorite),
                        group: project
                            .map(|project| project.group.clone())
                            .unwrap_or_default(),
                        summary,
                    }
                })
                .collect(),
        }
    })
    .await
}

#[tauri::command]
async fn read_repository(app: AppHandle, path: String) -> Result<RepositoryView, String> {
    blocking(app, move |gui| {
        let repo = repository(gui, &path)?;
        let details = repositories::details(&repositories::Here, &repo)?;
        let kept = kept_workspaces(gui);
        Ok(RepositoryView {
            changes: details
                .files
                .iter()
                .map(|file| FileView {
                    path: file.path.clone(),
                    original: file.original.clone(),
                    state: file.state(),
                    staged: file.is_staged(),
                    unstaged: file.has_unstaged(),
                    conflict: file.conflict,
                })
                .collect(),
            favorite: kept.favorite(&repo),
            tasks: workspaces::tasks(&repo, &kept),
            details,
        })
    })
    .await?
}

/// The diff of the repository or of one file, staged or not.
#[tauri::command]
async fn repository_diff(
    app: AppHandle,
    path: String,
    file: Option<String>,
    staged: bool,
) -> Result<repositories::Diff, String> {
    blocking(app, move |gui| {
        let repo = repository(gui, &path)?;
        let details = repositories::details(&repositories::Here, &repo)?;
        repositories::diff(
            &repositories::Here,
            &repo,
            &details,
            file.as_deref(),
            staged,
        )
    })
    .await?
}

/// A change the window asks for, as `helix-toolbox repos` takes it.
#[derive(serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ChangeRequest {
    Stage { files: Vec<String> },
    StageAll,
    Unstage { files: Vec<String> },
    Commit { message: String },
    CreateBranch { name: String },
    SwitchBranch { name: String },
    Stash { message: String, untracked: bool },
    ApplyStash { id: String },
    Fetch,
    Pull,
    Push,
    Publish { remote: String },
}

impl From<ChangeRequest> for repositories::Change {
    fn from(request: ChangeRequest) -> Self {
        use repositories::Change;
        match request {
            ChangeRequest::Stage { files } => Change::Stage(files),
            ChangeRequest::StageAll => Change::StageAll,
            ChangeRequest::Unstage { files } => Change::Unstage(files),
            ChangeRequest::Commit { message } => Change::Commit(message),
            ChangeRequest::CreateBranch { name } => Change::CreateBranch(name),
            ChangeRequest::SwitchBranch { name } => Change::SwitchBranch(name),
            ChangeRequest::Stash { message, untracked } => Change::Stash { message, untracked },
            ChangeRequest::ApplyStash { id } => Change::ApplyStash(id),
            ChangeRequest::Fetch => Change::Fetch,
            ChangeRequest::Pull => Change::Pull,
            ChangeRequest::Push => Change::Push,
            ChangeRequest::Publish { remote } => Change::Publish(remote),
        }
    }
}

/// A change to a repository, planned from how it is now, to confirm.
#[tauri::command]
async fn prepare_repository_change(
    app: AppHandle,
    path: String,
    change: ChangeRequest,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = repository(gui, &path).and_then(|repo| {
            let details = repositories::details(&repositories::Here, &repo)?;
            repositories::plan(&repositories::Here, &details, &change.into())
        });
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// One of a repository's tasks, to confirm before it runs in its folder.
#[tauri::command]
async fn prepare_task(app: AppHandle, path: String, id: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = repository(gui, &path).and_then(|repo| {
            let tasks = workspaces::tasks(&repo, &kept_workspaces(gui));
            let task = tasks
                .iter()
                .find(|task| task.id == id)
                .ok_or_else(|| format!("it has no task {id}"))?;
            workspaces::plan_task(&repo, task)
        });
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// Keep a task of the user's, remove one, or mark a favorite, in
/// `workspaces.toml`.
#[tauri::command]
async fn save_workspace(
    app: AppHandle,
    path: String,
    favorite: Option<bool>,
    task: Option<workspaces::Task>,
    remove: Option<String>,
) -> Result<(), String> {
    blocking(app, move |gui| {
        let repo = repository(gui, &path)?;
        let file = workspaces_file(gui)?;
        let mut kept = workspaces::load(&file)?;
        if let Some(favorite) = favorite {
            kept.set_favorite(&repo, favorite);
        }
        if let Some(task) = task {
            kept.add_task(&repo, task)?;
        }
        if let Some(id) = remove {
            kept.remove_task(&repo, &id)?;
        }
        workspaces::save(&file, &kept)
    })
    .await?
}

// Home Manager

/// Home Manager, as `helix-toolbox home-manager --json` shows it.
#[derive(Serialize)]
struct HomeManagerView {
    #[serde(flatten)]
    status: home_manager::Status,
    /// The `--flake` reference built, for a flake.
    reference: Option<String>,
    /// Where the configuration was named.
    origin: Option<&'static str>,
}

/// Home Manager: its configuration, generations, repository and links.
#[tauri::command]
async fn read_home_manager(app: AppHandle) -> Result<HomeManagerView, String> {
    blocking(app, |_| {
        let status = home_manager::status(&doctor::Here, &repositories::Here);
        HomeManagerView {
            reference: status
                .config
                .as_ref()
                .and_then(home_manager::Config::flake_reference),
            origin: status.config.as_ref().map(|config| config.origin.label()),
            status,
        }
    })
    .await
}

/// A Home Manager plan to review: `build`, `switch`, `activate` (an
/// earlier generation, by its number) or `pull` (its repository).
#[tauri::command]
async fn prepare_home_manager(
    app: AppHandle,
    action: String,
    generation: Option<u32>,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let planned = (|| -> Result<Plan, String> {
            let status = home_manager::status(&doctor::Here, &repositories::Here);
            if !status.installed {
                return Err("home-manager is not installed here.".into());
            }
            let config = match (&status.config, &status.problem) {
                (Some(config), _) => config,
                (None, Some(problem)) => return Err(problem.clone()),
                (None, None) => {
                    return Err("No Home Manager configuration was found: name its folder as flake under [home_manager] in settings.toml.".into());
                }
            };
            match action.as_str() {
                "build" => Ok(home_manager::plan_build(config)),
                "switch" => Ok(home_manager::plan_switch(config)),
                "activate" => {
                    let wanted = generation.ok_or("no generation was chosen")?;
                    let found = home_manager::find_generation(
                        &status.generations,
                        &wanted.to_string(),
                    )?;
                    home_manager::plan_activate(&doctor::Here, &found)
                }
                "pull" => {
                    let repository = status
                        .repository
                        .as_ref()
                        .ok_or("The configuration is not in a Git repository.")?;
                    let details = repositories::details(&repositories::Here, &repository.path)?;
                    repositories::plan(
                        &repositories::Here,
                        &details,
                        &repositories::Change::Pull,
                    )
                }
                other => Err(format!("{} is not a Home Manager plan", printable(other))),
            }
        })();
        let catalog = gui.catalog.read().unwrap();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Hardware

/// Firmware, disks, sensors, batteries and devices, read from this machine.
#[tauri::command]
async fn read_hardware(app: AppHandle) -> Result<hardware::Hardware, String> {
    blocking(app, |gui| {
        let found = hardware::gather(&doctor::Here);
        *gui.hardware.lock().unwrap() = Some(found.clone());
        found
    })
    .await
}

/// A firmware update, by its place in the list, ready to confirm.
#[tauri::command]
async fn prepare_firmware(app: AppHandle, index: usize) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let update = gui
            .hardware
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|found| found.firmware.updates.get(index).cloned());
        let Some(update) = update else {
            return cannot(
                gui,
                "That update is no longer listed; read the hardware again.".into(),
            );
        };
        let Some(planned) = hardware::plan_firmware(&update) else {
            return cannot(gui, "This update cannot be planned here.".into());
        };
        let catalog = gui.catalog.read().unwrap();
        prepare_plan(gui, &catalog, planned)
    })
    .await
}

/// A disk's SMART health check, ready to confirm.
#[tauri::command]
async fn prepare_smart(app: AppHandle, disk: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let Some(planned) = hardware::plan_smart(&disk) else {
            return cannot(gui, "This disk's health cannot be read here.".into());
        };
        let catalog = gui.catalog.read().unwrap();
        prepare_plan(gui, &catalog, planned)
    })
    .await
}

// Desktop

#[derive(Serialize)]
struct DesktopView {
    desktop: desktop::Desktop,
    looks: Vec<LookView>,
    /// Entries for this desktop, from the setup wizard's choices.
    suggestions: Vec<setup::Suggestion>,
}

#[derive(Serialize)]
struct LookView {
    slug: String,
    name: String,
    /// The desktop it was saved on.
    desktop: String,
    settings: usize,
    /// Saved on another desktop than this one.
    other: bool,
}

fn desktop_here() -> desktop::Desktop {
    desktop::detect(&doctor::Here, &|name| std::env::var(name).ok())
}

// ----- The setup wizard -----

/// The wizard's first pages: the machine, and the workstations to choose from.
#[derive(Serialize)]
struct SetupView {
    machine: Vec<(&'static str, String)>,
    workstations: Vec<WorkstationView>,
}

#[derive(Serialize)]
struct WorkstationView {
    id: String,
    name: String,
    description: String,
}

/// One entry the wizard suggests, ticked where it can run and is not
/// installed.
#[derive(Serialize)]
struct PickView {
    id: String,
    name: String,
    available: bool,
    note: Option<String>,
    ticked: bool,
}

/// What the wizard learns about this machine, as the command line does.
fn survey_here(gui: &Gui) -> setup::Survey {
    setup::survey(
        &gui.platform,
        &desktop_here(),
        helix_toolbox_engine::preflight::gpus(),
        setup::laptop_here(),
        &helix_toolbox_platform::binary_exists,
    )
}

/// Open the wizard: the machine and the workstations (`catalog/setup.toml`).
#[tauri::command]
async fn setup_start(app: AppHandle) -> Result<SetupView, String> {
    blocking(app, |gui| {
        let catalog = gui.catalog.read().unwrap();
        let found = setup::load(catalog.root())?;
        let survey = survey_here(gui);
        Ok(SetupView {
            machine: setup::lines(&survey),
            workstations: found
                .workstations
                .iter()
                .map(|known| WorkstationView {
                    id: known.id.clone(),
                    name: known.name.clone(),
                    description: known.description.clone(),
                })
                .collect(),
        })
    })
    .await?
}

/// The chosen workstation's entries for this machine; a workstation with no
/// entries of its own (Custom) offers every workstation's, unticked.
#[tauri::command]
async fn setup_choose(app: AppHandle, id: String) -> Result<Vec<PickView>, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let found = setup::load(catalog.root())?;
        let survey = survey_here(gui);
        let chosen = found
            .workstations
            .iter()
            .find(|known| known.id == id)
            .ok_or_else(|| format!("there is no workstation {id}"))?;
        let custom = chosen.entries.is_empty() && chosen.when.is_empty();
        let ids = if custom {
            found.everything(&survey)
        } else {
            chosen.entries_for(&survey)
        };
        let inventory = gui.inventory.lock().unwrap();
        let picks = setup::suggestions(&ids, &catalog, &gui.platform)
            .into_iter()
            .map(|suggestion| {
                let status = inventory.as_ref().and_then(|inventory| {
                    catalog
                        .find(&suggestion.id)
                        .and_then(|entry| inventory.status(entry, &gui.platform, &which))
                        .map(|status| views::status(&status))
                });
                let installed = status
                    .as_ref()
                    .is_some_and(|status| status.kind != "absent");
                PickView {
                    ticked: suggestion.available && !installed && !custom,
                    note: match status {
                        Some(status) if installed => Some(format!("installed ({})", status.detail)),
                        _ => suggestion.note,
                    },
                    id: suggestion.id,
                    name: suggestion.name,
                    available: suggestion.available,
                }
            })
            .collect();
        Ok(picks)
    })
    .await?
}

/// Remember that the wizard was offered, so it no longer opens by itself.
#[tauri::command]
fn setup_done(gui: State<Gui>) -> Result<(), String> {
    match &gui.settings {
        Some(path) => settings::mark_set_up(path)
            .map_err(|error| format!("the settings could not be saved: {error}")),
        None => Ok(()),
    }
}

/// The running desktop, its appearance, the saved looks and the entries
/// for it.
#[tauri::command]
async fn read_desktop(app: AppHandle) -> Result<DesktopView, String> {
    blocking(app, |gui| {
        let catalog = gui.catalog.read().unwrap();
        let found = desktop_here();
        let looks = desktop::dir()
            .map(|directory| desktop::load_all(&directory).0)
            .unwrap_or_default()
            .into_iter()
            .map(|(slug, look)| LookView {
                slug,
                name: helix_toolbox_core::printable(&look.name),
                desktop: helix_toolbox_core::printable(&look.desktop),
                settings: look.settings.len(),
                other: found.kind.map(|kind| kind.id()) != Some(look.desktop.as_str()),
            })
            .collect();
        let offered = setup::load(catalog.root())
            .map(|setup| setup.for_desktop(found.kind))
            .unwrap_or_default();
        let suggestions = setup::suggestions(&offered, &catalog, &gui.platform)
            .into_iter()
            .filter(|suggestion| suggestion.available)
            .collect();
        DesktopView {
            desktop: found,
            looks,
            suggestions,
        }
    })
    .await
}

/// Save the desktop's appearance as a look; what to tell the user.
#[tauri::command]
fn save_look(name: String) -> Result<String, String> {
    let found = desktop_here();
    let look = desktop::capture(&name, &found)
        .ok_or("no desktop whose appearance Helix Toolbox can read is running here")?;
    let directory = desktop::dir().ok_or("cannot find the configuration folder")?;
    let path = desktop::save(&directory, &look).map_err(|error| error.to_string())?;
    Ok(format!(
        "{} ({} settings) is saved as {}.",
        look.name,
        look.settings.len(),
        path.display()
    ))
}

/// Put a saved look back, ready to confirm.
#[tauri::command]
async fn prepare_look(app: AppHandle, slug: String) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let Some(directory) = desktop::dir() else {
            return cannot(gui, "cannot find the configuration folder".into());
        };
        let Some((_, look)) = desktop::load_all(&directory)
            .0
            .into_iter()
            .find(|(known, _)| *known == slug)
        else {
            return cannot(gui, "That look is no longer saved.".into());
        };
        match desktop::plan_apply(&look, &desktop_here()) {
            Ok(planned) => {
                let catalog = gui.catalog.read().unwrap();
                prepare_plan(gui, &catalog, planned)
            }
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Offline bundles

/// A folder as the user typed it: `~` is the home folder; anything else
/// must be a full path.
fn folder(text: &str) -> Result<PathBuf, String> {
    let text = text.trim();
    let home = || {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| "there is no home folder".to_string())
    };
    let path = match text.strip_prefix("~/") {
        Some(rest) => home()?.join(rest),
        None if text == "~" => home()?,
        None => PathBuf::from(text),
    };
    if text.is_empty() || !path.is_absolute() {
        return Err("Give the folder's full path, such as ~/helix-toolbox-bundle.".to_string());
    }
    Ok(path)
}

/// What a bundle of these entries would hold; it downloads nothing.
#[tauri::command]
async fn bundle_contents(
    app: AppHandle,
    ids: Vec<String>,
    complete: bool,
) -> Result<BundleContentsView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let scratch = bundle::scratch()
            .map_err(|error| format!("cannot create a scratch folder: {error}"))?;
        let found = bundle::contents(
            &ids,
            &catalog,
            &gui.platform,
            complete,
            &doctor::Here,
            &scratch,
        );
        let _ = std::fs::remove_dir_all(&scratch);
        found
            .map(|found| views::bundle_contents(&found))
            .map_err(|error| error.to_string())
    })
    .await?
}

/// Plan making a bundle of these entries in a new or empty folder, which
/// the command line does in the run's terminal.
#[tauri::command]
async fn prepare_bundle_create(
    app: AppHandle,
    directory: String,
    ids: Vec<String>,
    complete: bool,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let planned = (|| {
            if ids.is_empty() {
                return Err("Queue the entries to bundle in the Install Plan first.".to_string());
            }
            if let Some(id) = ids.iter().find(|id| catalog.find(id).is_none()) {
                return Err(format!("There is no entry {id} in the catalog."));
            }
            let directory = folder(&directory)?;
            if std::fs::read_dir(&directory).is_ok_and(|mut listing| listing.next().is_some()) {
                return Err(bundle::BundleError::NotEmpty(directory).to_string());
            }
            if !gui.this.is_absolute() {
                return Err(
                    "The command line, helix-toolbox, makes the bundle, and it was not found."
                        .to_string(),
                );
            }
            Ok(bundle::plan_create(&gui.this, &directory, &ids, complete))
        })();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

/// Check a bundle: made for this machine, every file matching its checksum.
#[tauri::command]
async fn bundle_check(app: AppHandle, directory: String) -> Result<BundleView, String> {
    blocking(app, move |gui| {
        let directory = folder(&directory)?;
        bundle::verify(&directory, &gui.platform)
            .map(|manifest| views::bundle(&manifest))
            .map_err(|error| error.to_string())
    })
    .await?
}

/// Check a bundle, then plan installing its packages from its files.
#[tauri::command]
async fn prepare_bundle_install(
    app: AppHandle,
    directory: String,
    trust: bool,
) -> Result<PreparedView, String> {
    blocking(app, move |gui| {
        let catalog = gui.catalog.read().unwrap();
        let planned = (|| {
            let directory = folder(&directory)?;
            let manifest =
                bundle::verify(&directory, &gui.platform).map_err(|error| error.to_string())?;
            bundle::plan_install(
                &directory,
                &manifest,
                &[],
                &gui.platform,
                &doctor::Here,
                trust,
            )
            .map_err(|error| error.to_string())
        })();
        match planned {
            Ok(planned) => prepare_plan(gui, &catalog, planned),
            Err(why) => cannot(gui, why),
        }
    })
    .await
}

// Settings

#[derive(Serialize)]
struct SettingsView {
    keep_going: bool,
    snapshots: bool,
    /// The snapshot tool set up here, if any.
    snapshot_tool: Option<&'static str>,
    /// Where the settings are kept.
    path: Option<String>,
    theme: WindowTheme,
    /// Whether update notifications are on; None where there is no home
    /// to keep their timer in.
    notifications: Option<bool>,
    /// Whether new health problems are told of; None as above.
    health: Option<bool>,
    /// The baseline Drift is watched against, by its file name.
    drift: Option<String>,
    /// The saved baselines to choose from: file name and name.
    baselines: Vec<(String, String)>,
    /// How often the timed restic backup runs (`day`, `week`), or none.
    restic_timer: Option<&'static str>,
    /// Whether a restic repository is set up, so the timer can be.
    restic_ready: bool,
}

#[tauri::command]
fn read_settings(gui: State<Gui>) -> SettingsView {
    let choices = gui
        .settings
        .as_deref()
        .map(settings::run_choices)
        .unwrap_or_default();
    let watched =
        notifications::Places::here().map(|places| watch::read(&places, gui.settings.as_deref()));
    SettingsView {
        keep_going: choices.keep_going,
        snapshots: choices.snapshots,
        snapshot_tool: backups::Snapshots::here().map(backups::Snapshots::label),
        path: gui.settings.as_ref().map(|path| path.display().to_string()),
        theme: theme(&gui),
        notifications: watched.as_ref().map(|watch| watch.updates),
        health: watched.as_ref().map(|watch| watch.health),
        drift: watched.and_then(|watch| watch.drift),
        baselines: drift::dir()
            .map(|directory| {
                drift::load_all(&directory)
                    .0
                    .into_iter()
                    .map(|(slug, baseline)| (slug, printable(&baseline.name)))
                    .collect()
            })
            .unwrap_or_default(),
        restic_timer: restic::units_here()
            .and_then(|units| restic::timer(&units))
            .map(|every| match every {
                restic::Every::Day => "day",
                restic::Every::Week => "week",
            }),
        restic_ready: restic_ready().is_ok(),
    }
}

/// Turn the update notifications on or off, as `helix-toolbox
/// notifications` does; the timer runs the command line, and a click on a
/// notification opens this window. What happened, to tell.
#[tauri::command]
async fn save_watch(
    app: AppHandle,
    updates: bool,
    health: bool,
    drift: Option<String>,
) -> Result<String, String> {
    blocking(app, move |gui| {
        let places =
            notifications::Places::here().ok_or("there is no home folder to keep the timer in")?;
        let path = gui
            .settings
            .clone()
            .ok_or("there is no home folder to keep the choices in")?;
        if let Some(slug) = &drift {
            let directory = drift::dir().ok_or("cannot find the configuration folder")?;
            drift::find(&directory, slug)
                .ok_or_else(|| format!("There is no baseline {}.", printable(slug)))?;
        }
        let chosen = watch::Watch {
            updates,
            health,
            drift,
        };
        // An AppImage runs from a mount that goes away; its own file stays.
        let window = std::env::var_os("APPIMAGE")
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok());
        let started = watch::apply(
            &places,
            &path,
            &chosen,
            &gui.this,
            window.as_deref(),
            &notifications::systemctl,
        )?;
        let what: Vec<String> = [
            chosen.updates.then(|| "updates are waiting".to_string()),
            chosen
                .health
                .then(|| "Helix Doctor finds a new problem".to_string()),
            chosen
                .drift
                .as_ref()
                .map(|slug| format!("the machine changes since the baseline {slug}")),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok(match started {
            _ if what.is_empty() => "Notifications are off.".to_string(),
            Some(notifications::Started::AtLogin) => "The checks are set up, but systemd's user manager did not answer, so they start at your next login.".to_string(),
            _ => format!(
                "Helix Toolbox checks {}, and soon after you log in, and shows a notification when {}; a click on it opens the screen here. It never updates, repairs or puts back anything itself.",
                notifications::EVERY,
                what.join(", or when ")
            ),
        })
    })
    .await?
}

/// Save the run choices, leaving the terminal interface's look as it is.
#[tauri::command]
fn save_settings(
    gui: State<Gui>,
    keep_going: bool,
    snapshots: bool,
) -> Result<SettingsView, String> {
    let path = gui
        .settings
        .as_deref()
        .ok_or("cannot find the configuration folder")?;
    settings::save_run_choices(
        path,
        RunChoices {
            keep_going,
            snapshots,
        },
    )
    .map_err(|error| format!("the settings could not be saved: {error}"))?;
    Ok(read_settings(gui))
}

/// Save the window's theme, leaving every other setting as it is.
#[tauri::command]
fn save_theme(gui: State<Gui>, theme: WindowTheme) -> Result<SettingsView, String> {
    let path = gui
        .settings
        .as_deref()
        .ok_or("cannot find the configuration folder")?;
    settings::save_window_theme(path, theme)
        .map_err(|error| format!("the theme could not be saved: {error}"))?;
    Ok(read_settings(gui))
}

/// `--drive FILE`, in debug builds: a JavaScript file run in the window once
/// it has loaded, so a test can work the interface where no pointer can.
#[cfg(debug_assertions)]
fn drive_script(args: &[String]) -> Option<String> {
    let path = args
        .iter()
        .position(|arg| arg == "--drive")
        .and_then(|index| args.get(index + 1))?;
    match std::fs::read_to_string(path) {
        Ok(script) => Some(script),
        Err(error) => {
            eprintln!("Helix Toolbox: cannot read {path}: {error}");
            std::process::exit(1);
        }
    }
}

/// What a drive script reports, on standard error.
#[cfg(debug_assertions)]
#[tauri::command]
fn trace(message: String) {
    eprintln!("drive: {message}");
}

/// A drive script is done: the program ends.
#[cfg(debug_assertions)]
#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

fn main() {
    // Before anything reads them: the folders named for HelixForge.
    for problem in helix_toolbox_engine::legacy::move_old_folders() {
        eprintln!("Helix Toolbox: {problem}");
    }
    let args: Vec<String> = std::env::args().collect();
    let platform = match LinuxInfo::detect() {
        Ok(platform) => platform,
        Err(error) => {
            eprintln!("Helix Toolbox needs Linux with /etc/os-release: {error}");
            std::process::exit(1);
        }
    };
    let catalog = match load_catalog(catalog_dir(&args).as_deref()) {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("Helix Toolbox: {error}");
            std::process::exit(1);
        }
    };
    let settings = settings::path();
    let pinned = settings
        .as_deref()
        .map(|path| settings::load_favorites(&settings::favorites_path(path)))
        .unwrap_or_default();
    let this = command_line(std::env::current_exe().ok().as_deref());
    let gui = Gui {
        catalog: RwLock::new(catalog),
        platform,
        this,
        settings,
        favorites: Mutex::new(pinned),
        pending: Mutex::new(None),
        running: Mutex::new(None),
        inventory: Mutex::new(None),
        diagnoses: Mutex::new(Vec::new()),
        channels: Mutex::new(Vec::new()),
        backups: Mutex::new(Vec::new()),
        hardware: Mutex::new(None),
        opens: ["updates", "doctor", "drift"]
            .into_iter()
            .find(|screen| args.iter().any(|arg| *arg == format!("--{screen}"))),
    };
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(gui);
    macro_rules! commands {
        ($($extra:ident),*) => {
            tauri::generate_handler![
                about, areas, statuses, favorites, pin, prepare, changes, download_sources, cancel,
                run, write, resize,
                stop, close_run, runs, save_profile, import_profile, export_profile,
                delete_profile, bug_report, copy_text, hardware_report, open_issue, doctor,
                prepare_repair, updates,
                prepare_channels, prepare_update, prepare_remove, list_backups, prepare_restore, drift_targets, check_entry, read_services, service_details,
                prepare_service, prepare_autostart, read_storage, prepare_clean,
                read_performance, compare_boots, read_recovery, prepare_snapshot_check,
                read_migration_export, export_migration, open_migration, prepare_migration,
                read_restic, restic_files, restic_history, prepare_restic, record_baseline,
                set_restic_timer,
                read_repositories, read_repository, repository_diff, prepare_repository_change,
                read_home_manager, prepare_home_manager,
                prepare_task, save_workspace,
                read_configs, validate_config, set_config_field, prepare_config_save,
                prepare_config_restore,
                drift_check, save_baseline, reliability, trust, read_hardware, prepare_firmware,
                prepare_smart, read_desktop, save_look, prepare_look, read_settings,
                save_settings, save_watch, save_theme, bundle_contents, prepare_bundle_create,
                bundle_check, prepare_bundle_install, setup_start, setup_choose, setup_done $(, $extra)*
            ]
        };
    }
    #[cfg(debug_assertions)]
    let builder = {
        let script = drive_script(&args);
        builder
            .invoke_handler(commands![trace, quit])
            .on_page_load(move |webview, payload| {
                if payload.event() == tauri::webview::PageLoadEvent::Finished
                    && let Some(script) = &script
                    && let Err(error) = webview.eval(script)
                {
                    eprintln!("drive: the script could not start: {error}");
                }
            })
    };
    #[cfg(not(debug_assertions))]
    let builder = builder.invoke_handler(commands![]);
    builder
        .run(tauri::generate_context!())
        .expect("the window could not be opened");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalog_directory_comes_from_its_option() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(
            catalog_dir(&args(&["gui", "--catalog-dir", "/tmp/c"])),
            Some(PathBuf::from("/tmp/c"))
        );
        assert_eq!(catalog_dir(&args(&["gui"])), None);
        assert_eq!(catalog_dir(&args(&["gui", "--catalog-dir"])), None);
    }

    #[test]
    fn a_profile_is_found_by_its_exact_id_inside_the_profiles_folder() {
        let catalog = Catalog::from_directory(
            &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures"),
            true,
        )
        .unwrap();
        for id in [
            "developer-basics",
            "profile-../../etc/passwd",
            "profile-Work",
            "profile-",
        ] {
            let refused = own_profile(&catalog, id).unwrap_err();
            assert!(
                refused.contains("is not one of your profiles"),
                "{id}: {refused}"
            );
        }
    }

    #[test]
    fn the_report_says_how_the_window_was_installed() {
        if std::env::var_os("APPIMAGE").is_some() {
            return;
        }
        assert_eq!(
            window_installed_from(Path::new("/usr/bin/helix-toolbox-gui")),
            "package"
        );
        assert_eq!(
            window_installed_from(Path::new(
                "/src/apps/helix-toolbox-gui/src-tauri/target/debug/helix-toolbox-gui"
            )),
            "source build"
        );
        assert_eq!(
            window_installed_from(Path::new("/opt/helix/helix-toolbox-gui")),
            "other"
        );
    }

    #[test]
    fn programs_of_the_desktop_get_the_environment_without_the_appimage() {
        let appdir = "/tmp/.mount_helixAbc";
        assert_eq!(outside("/usr/share", appdir), Outside::Keep);
        assert_eq!(
            outside(
                "/tmp/.mount_helixAbc/usr/share:/usr/share:/usr/local/share",
                appdir
            ),
            Outside::Set("/usr/share:/usr/local/share".to_string())
        );
        assert_eq!(
            outside("/tmp/.mount_helixAbc//usr/lib/gtk-3.0", appdir),
            Outside::Remove
        );
        assert_eq!(
            outside("/tmp/.mount_helixAbc/usr/share:", appdir),
            Outside::Remove
        );
    }

    #[test]
    fn the_report_shows_the_home_folder_as_a_tilde() {
        let Some(home) = std::env::var("HOME").ok().filter(|home| home.len() > 1) else {
            return;
        };
        let said = without_home(&format!(
            "- Settings: {home}/.config/helix-toolbox/settings.toml"
        ));
        assert_eq!(said, "- Settings: ~/.config/helix-toolbox/settings.toml");
    }

    #[test]
    fn a_folder_is_a_full_path_or_under_the_home_folder() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());
        assert_eq!(folder("~/bundle").unwrap(), home.join("bundle"));
        assert_eq!(
            folder(" /media/usb/b ").unwrap(),
            PathBuf::from("/media/usb/b")
        );
        assert!(folder("bundle").is_err());
        assert!(folder("-rf").is_err());
        assert!(folder("").is_err());
    }

    #[test]
    fn the_command_line_beside_this_program_comes_first() {
        let directory = tempfile::tempdir().unwrap();
        let beside = directory.path().join("helix-toolbox");
        std::fs::write(&beside, "").unwrap();
        let this = directory.path().join("helix-toolbox-gui");
        assert_eq!(command_line(Some(&this)), beside);
    }
}
