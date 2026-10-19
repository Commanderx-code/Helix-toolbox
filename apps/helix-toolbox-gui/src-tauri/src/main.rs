//! Helix Toolbox in a window. The frontend asks these commands for the
//! catalog and how entries stand, has the engine plan and check what the
//! user picked, and runs it with the engine's execution events, drawn in a
//! terminal panel. Every change to the machine is the engine's, as in the
//! terminal interface.

mod views;

use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::Entry;
use helix_toolbox_engine::{
    Plan, backups,
    confirmation::{self, Prepared},
    execution::{Execution, Options},
    history,
    inventory::{Inventory, which},
    overview::Overview,
    plan, preflight, profiles,
    reliability::Evidence,
    settings,
};
use helix_toolbox_platform::LinuxInfo;
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tauri::{AppHandle, Manager, State, ipc::Channel};
use views::{AreaView, PreparedView, RecordView, StatusView};

/// What the window works with.
struct Gui {
    catalog: Catalog,
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

/// Facts about this program and the machine, for the header and dashboard.
#[derive(Serialize)]
struct About {
    version: &'static str,
    overview: Overview,
    /// Whether the command line was found, which native steps need.
    command_line: bool,
}

#[tauri::command]
fn about(gui: State<Gui>) -> About {
    About {
        version: env!("CARGO_PKG_VERSION"),
        overview: Overview::gather(&gui.platform),
        command_line: gui.this.is_absolute(),
    }
}

#[tauri::command]
fn areas(gui: State<Gui>) -> Vec<AreaView> {
    views::areas(&gui.catalog)
}

/// How each package entry stands, by id. It asks the package managers, so
/// it runs off the window's thread.
#[tauri::command]
async fn statuses(gui: State<'_, Gui>) -> Result<HashMap<String, StatusView>, String> {
    let entries: Vec<Entry> = gui
        .catalog
        .entries()
        .into_iter()
        .filter(|entry| entry.identity().is_some())
        .cloned()
        .collect();
    let platform = gui.platform.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let inventory = Inventory::gather(&platform);
        entries
            .iter()
            .filter_map(|entry| {
                inventory
                    .status(entry, &platform, &which)
                    .map(|status| (entry.id.clone(), views::status(&status)))
            })
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
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

/// Plan these entries and check the plan, as the terminal interface does
/// before it asks. A plan ready to run waits for `run`.
#[tauri::command]
async fn prepare(app: AppHandle, ids: Vec<String>) -> Result<PreparedView, String> {
    // Planning asks the package manager about each package, so it runs off
    // the window's thread.
    tauri::async_runtime::spawn_blocking(move || {
        let gui = app.state::<Gui>();
        let (view, ready) = prepare_here(&gui, &ids);
        *gui.pending.lock().unwrap() = ready;
        view
    })
    .await
    .map_err(|error| error.to_string())
}

fn prepare_here(gui: &Gui, ids: &[String]) -> (PreparedView, Option<Plan>) {
    let lookup = |id: &str| gui.catalog.find(id);
    let mut entries = Vec::new();
    for id in ids {
        match lookup(id) {
            Some(entry) => entries.push(entry),
            None => {
                let lines = vec![format!("There is no entry {id} in the catalog.")];
                return (PreparedView::Error { lines }, None);
            }
        }
    }
    let planned = match plan(entries, &gui.platform, &lookup) {
        Ok(planned) => planned,
        Err(error) => {
            let lines = vec![error.to_string()];
            return (PreparedView::Error { lines }, None);
        }
    };
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
        &lookup,
        choices,
        &preflight::Facts::real(),
        &Evidence::load(gui.catalog.root()),
    );
    let view = views::prepared(&prepared);
    let ready = match prepared {
        Prepared::Ready(ready) => Some(ready.plan),
        _ => None,
    };
    (view, ready)
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
    let (execution, received) =
        Execution::start(&planned, &gui.catalog.lib_dir(), &gui.this, options)
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
        catalog,
        platform,
        this,
        settings,
        favorites: Mutex::new(pinned),
        pending: Mutex::new(None),
        running: Mutex::new(None),
    };
    let builder = tauri::Builder::default().manage(gui);
    #[cfg(debug_assertions)]
    let builder = {
        let script = drive_script(&args);
        builder
            .invoke_handler(tauri::generate_handler![
                about, areas, statuses, favorites, pin, prepare, cancel, run, write, resize, stop,
                close_run, runs, trace
            ])
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
    let builder = builder.invoke_handler(tauri::generate_handler![
        about, areas, statuses, favorites, pin, prepare, cancel, run, write, resize, stop,
        close_run, runs
    ]);
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
    fn the_command_line_beside_this_program_comes_first() {
        let directory = tempfile::tempdir().unwrap();
        let beside = directory.path().join("helix-toolbox");
        std::fs::write(&beside, "").unwrap();
        let this = directory.path().join("helix-toolbox-gui");
        assert_eq!(command_line(Some(&this)), beside);
    }
}
