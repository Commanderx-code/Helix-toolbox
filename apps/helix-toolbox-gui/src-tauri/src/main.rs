//! Helix Toolbox in a window. The frontend asks these commands for the
//! catalog and how entries stand, has the engine plan and check what the
//! user picked, and runs it with the engine's execution events, drawn in a
//! terminal panel. Every change to the machine is the engine's, as in the
//! terminal interface.

mod views;

use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry};
use helix_toolbox_engine::{
    Plan, backups,
    confirmation::{self, Prepared},
    desktop, doctor, drift,
    execution::{Execution, Options},
    hardware, history,
    inventory::{Inventory, which},
    overview::Overview,
    plan, preflight, profiles,
    reliability::{self, Evidence},
    settings::{self, RunChoices},
    setup, trust, updates,
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
use views::{
    AreaView, BackupView, ChannelView, DiagnosisView, PreparedView, RecordView, StatusView,
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
}

#[tauri::command]
fn about(gui: State<Gui>) -> About {
    About {
        version: VERSION,
        overview: Overview::gather(&gui.platform),
        command_line: gui.this.is_absolute(),
    }
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

// Settings

#[derive(Serialize)]
struct SettingsView {
    keep_going: bool,
    snapshots: bool,
    /// The snapshot tool set up here, if any.
    snapshot_tool: Option<&'static str>,
    /// Where the settings are kept.
    path: Option<String>,
}

#[tauri::command]
fn read_settings(gui: State<Gui>) -> SettingsView {
    let choices = gui
        .settings
        .as_deref()
        .map(settings::run_choices)
        .unwrap_or_default();
    SettingsView {
        keep_going: choices.keep_going,
        snapshots: choices.snapshots,
        snapshot_tool: backups::Snapshots::here().map(backups::Snapshots::label),
        path: gui.settings.as_ref().map(|path| path.display().to_string()),
    }
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
    };
    let builder = tauri::Builder::default().manage(gui);
    macro_rules! commands {
        ($($extra:ident),*) => {
            tauri::generate_handler![
                about, areas, statuses, favorites, pin, prepare, cancel, run, write, resize,
                stop, close_run, runs, save_profile, doctor, prepare_repair, updates,
                prepare_channels, prepare_update, list_backups, prepare_restore, drift_targets,
                drift_check, save_baseline, reliability, trust, read_hardware, prepare_firmware,
                prepare_smart, read_desktop, save_look, prepare_look, read_settings,
                save_settings $(, $extra)*
            ]
        };
    }
    #[cfg(debug_assertions)]
    let builder = {
        let script = drive_script(&args);
        builder
            .invoke_handler(commands![trace])
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
    fn the_command_line_beside_this_program_comes_first() {
        let directory = tempfile::tempdir().unwrap();
        let beside = directory.path().join("helix-toolbox");
        std::fs::write(&beside, "").unwrap();
        let this = directory.path().join("helix-toolbox-gui");
        assert_eq!(command_line(Some(&this)), beside);
    }
}
