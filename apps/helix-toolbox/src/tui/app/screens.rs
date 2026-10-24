//! The tool screens, one module each: a screen owns its state, what it
//! reads in the background, its rows, its keys, its drawing and what it
//! reads again after a run, behind [`ToolScreen`]. [`App`](super::App)
//! holds them in [`Screens`] and hands the current one its keys, its frame
//! and the runs that end.
//!
//! A screen sees the rest of the interface through a [`View`] while it
//! draws or names its keys, and a [`Cx`] while it opens, answers a key or
//! takes in a read; what only the interface does, such as confirming a
//! plan or opening another screen, it [asks](Ask) for.
//!
//! Settings, the profiles, the reports and the setup wizard are the
//! interface's own and add their methods to [`App`](super::App) from a file
//! here.

use super::*;
use std::cell::OnceCell;

mod backups;
mod bundles;
mod configs;
mod desktop;
mod doctor;
mod drift;
mod hardware;
mod history;
mod home;
mod migration;
mod performance;
mod profiles;
mod recovery;
mod reliability;
mod reports;
mod repos;
mod restic;
mod services;
mod settings;
mod storage;
mod updates;
mod wizard;

#[cfg(test)]
pub(in crate::tui::app) use self::{
    bundles::{BUNDLE_ROWS, BundleRow},
    configs::ConfigRow,
    desktop::DesktopRow,
    drift::DriftTarget,
    hardware::{HardwareAct, HardwareRow},
    home::HomeRow,
    migration::MigrationRow,
    performance::PerformanceRow,
    reliability::Coverage,
    repos::{RepoRow, task_shell},
    restic::{ResticReading, ResticRow},
    services::ServiceRow,
    storage::StorageRow,
};
pub use configs::EditRequest;

/// The tool screens, as the interface holds them.
#[derive(Default)]
pub(in crate::tui::app) struct Screens {
    pub(in crate::tui::app) history: history::History,
    pub(in crate::tui::app) backups: backups::Backups,
    pub(in crate::tui::app) doctor: doctor::Doctor,
    pub(in crate::tui::app) updates: updates::Updates,
    pub(in crate::tui::app) drift: drift::Drift,
    pub(in crate::tui::app) reliability: reliability::Reliability,
    pub(in crate::tui::app) hardware: hardware::Hardware,
    pub(in crate::tui::app) desktop: desktop::Desktop,
    pub(in crate::tui::app) bundles: bundles::Bundles,
    pub(in crate::tui::app) services: services::Services,
    pub(in crate::tui::app) storage: storage::Storage,
    pub(in crate::tui::app) performance: performance::Performance,
    pub(in crate::tui::app) recovery: recovery::Recovery,
    pub(in crate::tui::app) migration: migration::Migration,
    pub(in crate::tui::app) restic: restic::Restic,
    pub(in crate::tui::app) repos: repos::Repos,
    pub(in crate::tui::app) configs: configs::Configs,
    pub(in crate::tui::app) home: home::Home,
}

impl Screens {
    /// The tool screen `screen` is, if it is one.
    pub(in crate::tui::app) fn tool(&self, screen: Screen) -> Option<&dyn ToolScreen> {
        Some(match screen {
            Screen::History => &self.history,
            Screen::Backups => &self.backups,
            Screen::Doctor => &self.doctor,
            Screen::Updates => &self.updates,
            Screen::Drift => &self.drift,
            Screen::Reliability => &self.reliability,
            Screen::Hardware => &self.hardware,
            Screen::Desktop => &self.desktop,
            Screen::Bundles => &self.bundles,
            Screen::Services => &self.services,
            Screen::Storage => &self.storage,
            Screen::Performance => &self.performance,
            Screen::Recovery => &self.recovery,
            Screen::Migration => &self.migration,
            Screen::Restic => &self.restic,
            Screen::Repositories => &self.repos,
            Screen::Configs => &self.configs,
            Screen::HomeManager => &self.home,
            Screen::Dashboard
            | Screen::Area(_)
            | Screen::Plan
            | Screen::Favorites
            | Screen::Settings => return None,
        })
    }

    pub(in crate::tui::app) fn tool_mut(&mut self, screen: Screen) -> Option<&mut dyn ToolScreen> {
        Some(match screen {
            Screen::History => &mut self.history,
            Screen::Backups => &mut self.backups,
            Screen::Doctor => &mut self.doctor,
            Screen::Updates => &mut self.updates,
            Screen::Drift => &mut self.drift,
            Screen::Reliability => &mut self.reliability,
            Screen::Hardware => &mut self.hardware,
            Screen::Desktop => &mut self.desktop,
            Screen::Bundles => &mut self.bundles,
            Screen::Services => &mut self.services,
            Screen::Storage => &mut self.storage,
            Screen::Performance => &mut self.performance,
            Screen::Recovery => &mut self.recovery,
            Screen::Migration => &mut self.migration,
            Screen::Restic => &mut self.restic,
            Screen::Repositories => &mut self.repos,
            Screen::Configs => &mut self.configs,
            Screen::HomeManager => &mut self.home,
            Screen::Dashboard
            | Screen::Area(_)
            | Screen::Plan
            | Screen::Favorites
            | Screen::Settings => return None,
        })
    }
}

impl App {
    /// Do something with the tool screens and what they may use of the
    /// rest of the interface; then what they asked for.
    pub(in crate::tui::app) fn with_screens<R>(
        &mut self,
        act: impl FnOnce(&mut Screens, &mut Cx) -> R,
    ) -> R {
        let mut asks = Vec::new();
        let result = {
            let App {
                screens,
                host,
                platform,
                catalog,
                statuses,
                inventory,
                queue,
                screen,
                main_state,
                popup,
                settings_path,
                evidence,
                ..
            } = self;
            let mut cx = Cx {
                host,
                platform,
                catalog,
                statuses,
                inventory: inventory.as_ref(),
                queue,
                screen: *screen,
                selected: main_state,
                popup,
                settings_path: settings_path.as_deref(),
                evidence,
                asks: &mut asks,
            };
            act(screens, &mut cx)
        };
        for ask in asks {
            self.ask(ask);
        }
        result
    }

    /// Do something with the current screen, when it is a tool screen.
    pub(in crate::tui::app) fn with_tool<R>(
        &mut self,
        act: impl FnOnce(&mut dyn ToolScreen, &mut Cx) -> R,
    ) -> Option<R> {
        let screen = self.screen;
        self.with_screens(|screens, cx| screens.tool_mut(screen).map(|tool| act(tool, cx)))
    }

    /// The configuration copy to open in the user's editor, once.
    pub fn take_edit(&mut self) -> Option<EditRequest> {
        self.screens.configs.take_edit()
    }

    /// The editor closed: check the copy, and review saving it.
    pub fn edited(&mut self, request: EditRequest, closed: Result<(), String>) {
        self.with_screens(|screens, cx| screens.configs.edited(cx, request, closed));
    }

    /// What the screens see of the interface while they draw.
    pub(in crate::tui::app) fn view(&self) -> View<'_> {
        View {
            host: &self.host,
            platform: &self.platform,
            catalog: &self.catalog,
            statuses: &self.statuses,
            queue: &self.queue,
            favorites: &self.favorites,
            focused: self.focus == Focus::Main,
            theme: self.theme,
            icons: self.icons,
            hits: &self.hits,
            settings_path: self.settings_path.as_deref(),
            snapshot_tool: self.snapshot_tool,
            snapshots: self.snapshots,
            evidence: &self.evidence,
        }
    }

    /// Do what a screen asked for.
    fn ask(&mut self, ask: Ask) {
        match ask {
            Ask::Plan(planned) => self.confirm_plan(planned),
            Ask::Run(entries) => self.confirm(entries),
            Ask::Update(entries) => self.update(entries),
            Ask::Effects(planned) => self.show_effects(&planned, false),
            Ask::Open(screen) => {
                self.open_screen(screen);
                self.focus = Focus::Main;
            }
            Ask::ShowUnit(name) => {
                self.screens.services.show(&name);
                self.open_screen(Screen::Services);
            }
            Ask::Inspect(id) => self.inspect(&id),
            Ask::ReportWorked(record, only) => self.report_worked(&record, only.as_deref()),
            Ask::ShowCheck(wanted) => {
                self.with_screens(|screens, cx| screens.doctor.show_check(cx, wanted))
            }
            Ask::Describe => self.describe(),
            Ask::Preview => self.preview(),
            Ask::Pin => self.toggle_favorite(),
            Ask::CheckStatuses => self.refresh_statuses(),
            Ask::Sidebar => self.focus = Focus::Sidebar,
            Ask::Clamp => self.clamp_main(),
        }
    }
}

/// What the interface asks of a tool screen.
pub(in crate::tui::app) trait ToolScreen {
    /// The screen opens: read what it shows, or lay out what was read.
    fn open(&mut self, cx: &mut Cx);

    /// How many rows its list has.
    fn len(&self, view: &View) -> usize;

    /// A key for the main list; `j` and `k` have moved the selection.
    fn key(&mut self, cx: &mut Cx, code: KeyCode);

    /// What the keys do with row `at` selected, for the footer.
    fn hints(&self, view: &View, at: Option<usize>, list: &mut Vec<Hint>);

    /// Draw it beside the sidebar; `state` is the main list's selection.
    fn draw(&self, view: &View, frame: &mut Frame, area: Rect, state: &mut ListState);

    /// Take in what its reads in the background found; true when one ended.
    fn poll(&mut self, _cx: &mut Cx) -> bool {
        false
    }

    /// Read again what a run left out of date, while it is shown.
    fn after_run(&mut self, _cx: &mut Cx, _reading: Reading) {}

    /// The catalog entry at row `at`, for the keys entries have everywhere.
    fn entry<'a>(&self, _view: &View<'a>, _at: Option<usize>) -> Option<&'a Entry> {
        None
    }
}

/// What a screen asks the interface to do once its key or read is handled.
pub(in crate::tui::app) enum Ask {
    /// Show a plan's confirmation, or why there is nothing to run.
    Plan(Result<Plan, EngineError>),
    /// Plan these entries and ask before running them.
    Run(Vec<Entry>),
    /// Plan updating these entries, from the last status check.
    Update(Vec<Entry>),
    /// What this plan is expected to change.
    Effects(Plan),
    /// Open this screen, with the focus on its list.
    Open(Screen),
    /// Show this unit in the Services screen, filtered to it.
    ShowUnit(String),
    /// The trust inspector for the entry with this id.
    Inspect(String),
    /// A report that a run worked here: of every entry, or only this one.
    ReportWorked(helix_toolbox_engine::history::Record, Option<String>),
    /// Select the health check with this id and show it.
    ShowCheck(&'static str),
    /// The selected entry's details, as the areas show them.
    Describe,
    /// The selected entry's script or commands.
    Preview,
    /// Pin the selected entry, or unpin it.
    Pin,
    /// Check how package entries stand, now.
    CheckStatuses,
    /// Give the sidebar the focus.
    Sidebar,
    /// The rows changed: keep the selection on one of the current screen's.
    Clamp,
}

/// What a tool screen may use of the rest of the interface while it opens,
/// answers a key or takes in a read.
pub(in crate::tui::app) struct Cx<'a> {
    /// The machine the screens read.
    pub(in crate::tui::app) host: &'a Host,
    pub(in crate::tui::app) platform: &'a LinuxInfo,
    pub(in crate::tui::app) catalog: &'a Catalog,
    /// How each package entry stands, and what that check read, once it has.
    pub(in crate::tui::app) statuses: &'a HashMap<String, Status>,
    pub(in crate::tui::app) inventory: Option<&'a Inventory>,
    /// The Install Plan's entries.
    pub(in crate::tui::app) queue: &'a mut Vec<Entry>,
    /// The screen shown.
    pub(in crate::tui::app) screen: Screen,
    /// The main list's selection.
    pub(in crate::tui::app) selected: &'a mut ListState,
    pub(in crate::tui::app) popup: &'a mut Option<Popup>,
    /// The settings file, when there is one.
    pub(in crate::tui::app) settings_path: Option<&'a Path>,
    evidence: &'a OnceCell<helix_toolbox_engine::reliability::Evidence>,
    asks: &'a mut Vec<Ask>,
}

impl Cx<'_> {
    pub(in crate::tui::app) fn ask(&mut self, ask: Ask) {
        self.asks.push(ask);
    }

    /// Show a plan's confirmation.
    pub(in crate::tui::app) fn confirm(&mut self, planned: Plan) {
        self.ask(Ask::Plan(Ok(planned)));
    }

    /// Show a plan's confirmation, or why it could not be made.
    pub(in crate::tui::app) fn confirm_or_say(&mut self, planned: Result<Plan, String>) {
        match planned {
            Ok(planned) => self.confirm(planned),
            Err(why) => self.say("Cannot plan", why),
        }
    }

    /// Show some text in a popup.
    pub(in crate::tui::app) fn say(&mut self, title: impl Into<String>, text: impl Into<String>) {
        *self.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    /// The selected row.
    pub(in crate::tui::app) fn at(&self) -> Option<usize> {
        self.selected.selected()
    }

    /// The rows changed: keep the selection on one, once this is handled.
    pub(in crate::tui::app) fn clamp(&mut self) {
        self.ask(Ask::Clamp);
    }

    /// Add an entry to the Install Plan, or take it out.
    pub(in crate::tui::app) fn toggle_queued(&mut self, entry: Entry) {
        toggle_queued(self.queue, self.popup, entry);
    }

    /// This build's testing record, read on first use.
    pub(in crate::tui::app) fn evidence(&self) -> helix_toolbox_engine::reliability::Evidence {
        evidence(self.evidence, self.catalog)
    }
}

/// What a tool screen may see of the rest of the interface while it draws
/// or names its keys.
pub(in crate::tui::app) struct View<'a> {
    /// The machine the screens read.
    pub(in crate::tui::app) host: &'a Host,
    pub(in crate::tui::app) platform: &'a LinuxInfo,
    pub(in crate::tui::app) catalog: &'a Catalog,
    /// How each package entry stands, from the last check.
    pub(in crate::tui::app) statuses: &'a HashMap<String, Status>,
    /// The Install Plan's entries.
    pub(in crate::tui::app) queue: &'a [Entry],
    /// Pinned entry ids.
    pub(in crate::tui::app) favorites: &'a [String],
    /// Whether the main list has the focus.
    pub(in crate::tui::app) focused: bool,
    pub(in crate::tui::app) theme: Theme,
    /// Nerd Font icons, or plain text.
    pub(in crate::tui::app) icons: bool,
    /// What the frame drew where, for the mouse.
    pub(in crate::tui::app) hits: &'a RefCell<Hits>,
    /// The settings file, when there is one.
    pub(in crate::tui::app) settings_path: Option<&'a Path>,
    /// The snapshot tool set up on the host, and whether Settings takes
    /// snapshots with it.
    pub(in crate::tui::app) snapshot_tool: Option<helix_toolbox_engine::backups::Snapshots>,
    pub(in crate::tui::app) snapshots: bool,
    pub(in crate::tui::app) evidence: &'a OnceCell<helix_toolbox_engine::reliability::Evidence>,
}

impl View<'_> {
    /// This build's testing record, read on first use.
    pub(in crate::tui::app) fn evidence(&self) -> helix_toolbox_engine::reliability::Evidence {
        evidence(self.evidence, self.catalog)
    }
}

/// This build's testing record, read into `cell` on first use.
pub(in crate::tui::app) fn evidence(
    cell: &OnceCell<helix_toolbox_engine::reliability::Evidence>,
    catalog: &Catalog,
) -> helix_toolbox_engine::reliability::Evidence {
    cell.get_or_init(|| helix_toolbox_engine::reliability::Evidence::load(catalog.root()))
        .clone()
}

/// Add an entry to the Install Plan, or take it out; one that runs on its
/// own says so instead.
pub(in crate::tui::app) fn toggle_queued(
    queue: &mut Vec<Entry>,
    popup: &mut Option<Popup>,
    entry: Entry,
) {
    if !entry.multi_select {
        *popup = Some(Popup::Text(TextPopup::new(
            entry.name.clone(),
            "This entry runs on its own and cannot be queued with others. Press Enter to run it.",
            true,
        )));
        return;
    }
    if let Some(position) = queue.iter().position(|other| other.id == entry.id) {
        queue.remove(position);
    } else {
        queue.push(entry);
    }
}

/// A read of the host in the background, until its answer is taken.
pub(in crate::tui::app) struct Read<T>(Option<mpsc::Receiver<T>>);

impl<T> Default for Read<T> {
    fn default() -> Self {
        Self(None)
    }
}

impl<T: Send + 'static> Read<T> {
    /// Start reading, in place of any read under way: on a thread of its
    /// own on this machine, where it can take a while; at once on a made-up
    /// one, whose answers are already there, so the next pass of the event
    /// loop shows it.
    pub(in crate::tui::app) fn start(
        &mut self,
        host: &Host,
        work: impl FnOnce(Host) -> T + Send + 'static,
    ) {
        let (sender, receiver) = mpsc::channel();
        let host = host.clone();
        if host.is_here() {
            std::thread::spawn(move || {
                let _ = sender.send(work(host));
            });
        } else {
            let _ = sender.send(work(host));
        }
        self.0 = Some(receiver);
    }
}

impl<T> Read<T> {
    /// Whether a read is under way.
    pub(in crate::tui::app) fn busy(&self) -> bool {
        self.0.is_some()
    }

    /// What the read found, once it has; a read that ended without an
    /// answer is forgotten.
    pub(in crate::tui::app) fn take(&mut self) -> Option<T> {
        match self.0.as_ref()?.try_recv() {
            Ok(found) => {
                self.0 = None;
                Some(found)
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                self.0 = None;
                None
            }
            Err(mpsc::TryRecvError::Empty) => None,
        }
    }
}

#[cfg(test)]
impl<T> From<mpsc::Receiver<T>> for Read<T> {
    fn from(receiver: mpsc::Receiver<T>) -> Self {
        Self(Some(receiver))
    }
}
