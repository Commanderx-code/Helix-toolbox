//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the HelixForge dashboard design.

use super::{
    banner,
    effects::{EffectsPopup, Outcome as EffectsOutcome},
    hints::Hint,
    icons::Icon,
    palette::{Choice, Outcome, Palette},
    popup::{Answer, ConfirmPopup, Decision, InputPopup, TextPopup, centered},
    running::RunningView,
    settings::{self, Look},
    sysinfo::Overview,
    theme::{self, Theme},
    wizard::{Outcome as WizardOutcome, Pick, Wizard},
};
use crate::TuiArgs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use helixforge_catalog::Catalog;
use helixforge_core::{Action, Entry, Item};
use helixforge_engine::{
    EngineError, Plan, backups, doctor, drift, history,
    inventory::{Inventory, Status, which},
    plan, plan_removal, preflight, profiles, reliability, updates,
};
use helixforge_platform::LinuxInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, List, ListState, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image, protocol::Protocol};
use std::{collections::HashMap, sync::mpsc};

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 25;
const SIDEBAR_WIDTH: u16 = 30;
const HEADER_HEIGHT: u16 = 4;
/// Room kept beside the banner for the status on the right.
const STATUS_WIDTH: u16 = 18;
const HIGHLIGHT: &str = "> ";
const ROOT_WARNING: &str = "You are running HelixForge as root.\n\nEvery entry then has full access to the system. The entries expect to run as your normal user and ask for your password when they need it.";
const FLAGS_GUIDE: &str = "The flags column names what an entry does:

D  - disk changes, such as partitioning (privileged)
FI - Flatpak installation
FM - file changes
I  - installation (privileged)
K  - kernel changes (privileged)
MP - package manager actions
SI - full system installation
SS - service actions (privileged)
RP - package removal

P* - the same action, privileged

Risk in a plan: D, K and SI are high; installs, services and anything privileged are medium; the rest is low.
";
const HELP: &str = "Everywhere
  Tab / Shift-Tab   next / previous screen       t   next theme (remembered)
  /                 search every area            ?   this help
  Ctrl-K            commands, by name            q / Ctrl-C  quit

Sidebar and Dashboard
  j / k             move                         Enter / l   open the screen or quick action

Areas (Applications and the other catalog screens)
  j / k             move                         Enter / l   open a group or run an entry
  h                 parent group, or the sidebar Space       add to or remove from the install plan
  d                 description                  p           preview the script or command
  r                 remove an installed package  v           where it comes from (trust)
  u                 update the entry             U           update every entry with an update
  g                 flags guide     *   pin a favorite     i   show all, installed or updates

Install Plan
  Enter             run the plan                 x / Delete  remove the selected entry
  c                 clear the plan               s           save it as a profile;  e  what it changes

History, Backups, Doctor, Hardware, Desktop, Updates, Drift, Reliability, Settings
  Enter             log / restore / details / change   r   reload or check again; f repair; u / U update
  Hardware: f       update firmware (type FIRMWARE)    s   SMART check;  Backups: e  side by side
  Desktop: s        save the look; Enter puts a saved one back      Settings: the setup wizard
  Drift: b          compare with another         s   save this machine; Space / a plan what is missing
  Reliability: i    critical or untested only    v   where the entry comes from

Running window
  Ctrl-C            stop; again, kill what is left  Enter / q   close when finished
  l                 save the log                 Page up / down   scroll
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    Dashboard,
    Area(usize),
    Plan,
    /// Pinned entries, then the ones run recently.
    Favorites,
    History,
    Backups,
    /// The health checks, with the Helix Doctor area's reports below them.
    Doctor,
    /// Pending updates of each tool that keeps the machine.
    Updates,
    /// How the machine differs from a baseline or a profile.
    Drift,
    /// How far each entry has been tested.
    Reliability,
    /// Firmware, disks, temperatures, batteries and devices.
    Hardware,
    /// The desktop environment, its look and the entries for it.
    Desktop,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Sidebar,
    Main,
    Search,
}

enum Popup {
    Text(TextPopup),
    Confirm(ConfirmPopup),
    Running(Box<RunningView>),
    /// The name to save the Install Plan's queue under, as a profile.
    ProfileName(InputPopup),
    /// The name to save this machine under, as a baseline.
    BaselineName(InputPopup),
    /// Every screen, action and entry, searchable (Ctrl-K).
    Palette(Palette),
    /// What a plan is expected to change.
    Effects(EffectsPopup),
    /// The word that confirms a firmware update, before its plan is shown.
    FirmwareWord(InputPopup),
    /// The setup wizard.
    Wizard(Box<Wizard>),
    /// The name to save the desktop's look under.
    LookName(InputPopup),
}

/// A line of the Desktop screen.
#[derive(Clone, Debug)]
enum DesktopRow {
    Heading(String),
    Info { text: String, details: String },
    Entry { id: String, text: String },
    Look { slug: String, text: String },
}

/// A confirmation's header and its steps with their commands.
type Confirming = (Vec<String>, Vec<(String, String)>);

/// A line of the Hardware screen.
#[derive(Clone, Debug)]
enum HardwareRow {
    Heading(String),
    Item {
        text: String,
        /// Shown in a warning color.
        attention: bool,
        details: String,
        act: Option<HardwareAct>,
    },
}

#[derive(Clone, Debug)]
enum HardwareAct {
    /// Update this firmware, by its index in the updates.
    Firmware(usize),
    /// Check this disk's SMART health.
    Smart(String),
}

/// What a command palette choice does.
#[derive(Clone, Debug)]
enum Act {
    Open(Screen),
    /// Plan the entry and ask, as Enter on it does.
    Entry(String),
    /// Show the trust inspector for the entry.
    Inspect(String),
    UpdatesOnline,
    /// Open Helix Doctor, and the details of the check with this id.
    Doctor(Option<&'static str>),
    SaveBaseline,
    Wizard,
    RunPlan,
    ClearPlan,
    SaveProfile,
    UpdateEntries,
    Theme,
    Icons,
    Help,
    Quit,
}

/// Which entries the Reliability screen lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Coverage {
    All,
    /// Boot, login-screen, driver and whole-system entries.
    Critical,
    /// Not yet validated, or tested with stand-ins only.
    Untested,
}

impl Coverage {
    fn next(self) -> Self {
        match self {
            Coverage::All => Coverage::Critical,
            Coverage::Critical => Coverage::Untested,
            Coverage::Untested => Coverage::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Coverage::All => "all",
            Coverage::Critical => "critical",
            Coverage::Untested => "untested",
        }
    }

    fn allows(self, assessment: &reliability::Assessment) -> bool {
        match self {
            Coverage::All => true,
            Coverage::Critical => assessment.critical,
            Coverage::Untested => assessment.level >= reliability::Level::StandInTested,
        }
    }
}

/// What Drift compares the machine with.
#[derive(Clone, Debug)]
enum DriftTarget {
    Baseline {
        slug: String,
        baseline: drift::Baseline,
    },
    Profile {
        id: String,
        name: String,
        entries: Vec<String>,
    },
}

impl DriftTarget {
    fn key(&self) -> String {
        match self {
            DriftTarget::Baseline { slug, .. } => format!("baseline:{slug}"),
            DriftTarget::Profile { id, .. } => format!("profile:{id}"),
        }
    }
}

#[derive(Clone)]
enum Row {
    Up,
    Group { index: usize, name: String },
    Entry(Box<Entry>),
}

struct MenuItem {
    label: String,
    screen: Screen,
    section: &'static str,
}

/// Which entries a list shows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Filter {
    All,
    /// Installed, needing an update, or managed by Nix.
    Installed,
    Updates,
}

impl Filter {
    fn next(self) -> Self {
        match self {
            Filter::All => Filter::Installed,
            Filter::Installed => Filter::Updates,
            Filter::Updates => Filter::All,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Filter::All => "all",
            Filter::Installed => "installed",
            Filter::Updates => "updates",
        }
    }

    fn allows(self, status: Option<&Status>) -> bool {
        match self {
            Filter::All => true,
            Filter::Installed => matches!(
                status,
                Some(
                    Status::Installed { .. }
                        | Status::Update { .. }
                        | Status::Managed { .. }
                        | Status::Elsewhere { .. }
                )
            ),
            Filter::Updates => matches!(status, Some(Status::Update { .. })),
        }
    }
}

/// Where the user was in one catalog area.
#[derive(Clone, Default)]
struct AreaState {
    path: Vec<(usize, usize)>,
    selected: usize,
}

const QUICK_ICONS: [Icon; 5] = [
    Icon::Applications,
    Icon::Plan,
    Icon::Doctor,
    Icon::History,
    Icon::Favorites,
];

const QUICK_ACTIONS: [(&str, &str); 5] = [
    ("Browse Applications", "Search and install tools"),
    ("Review the Install Plan", "See what is queued and run it"),
    ("Run System Doctor", "Health checks and reports"),
    ("View History", "Past runs and their logs"),
    ("Favorites", "Pinned and recently run entries"),
];

pub struct App {
    catalog: Catalog,
    platform: LinuxInfo,
    theme: Theme,
    /// The theme to remember: the one chosen, not one NO_COLOR imposed.
    saved_theme: Theme,
    focus: Focus,
    screen: Screen,
    menu: Vec<MenuItem>,
    menu_state: ListState,
    main_state: ListState,
    areas: Vec<AreaState>,
    rows: Vec<Row>,
    search: Vec<char>,
    cursor: usize,
    completion: Option<String>,
    queue: Vec<Entry>,
    pending: Option<Plan>,
    popup: Option<Popup>,
    records: Vec<history::Record>,
    overview: Overview,
    size_bypass: bool,
    skip_confirmation: bool,
    show_hidden: bool,
    /// Nerd Font icons, or plain text.
    icons: bool,
    /// After a failed step, continue with the steps that do not need it.
    keep_going: bool,
    /// Take a snapshot before system changes, where the machine can.
    snapshots: bool,
    /// The snapshot tool set up here, if any; none in tests.
    snapshot_tool: Option<backups::Snapshots>,
    /// The backups the Backups screen lists, newest first.
    backups: Vec<backups::Backup>,
    /// What the last health check found.
    diagnoses: Vec<doctor::Diagnosis>,
    /// Where profiles are saved; the user's configuration unless a test says.
    profiles_dir: Option<std::path::PathBuf>,
    /// Pinned entry ids, in the order pinned.
    favorites: Vec<String>,
    /// Where they are saved; none in tests.
    favorites_path: Option<std::path::PathBuf>,
    /// Entry ids run recently, newest first.
    recent: Vec<String>,
    /// Which entries the lists show.
    filter: Filter,
    /// The row selected before a search started, to return to.
    before_search: Option<usize>,
    /// A health check running in the background, if any.
    examining: Option<mpsc::Receiver<Vec<doctor::Diagnosis>>>,
    /// The update channels, from the last look.
    channels: Vec<updates::Channel>,
    /// Whether that look asked the network.
    channels_online: bool,
    /// A look at the update channels running in the background, if any.
    gathering: Option<mpsc::Receiver<(Vec<updates::Channel>, bool)>>,
    /// Whether this run of the interface checks the machine; off in tests.
    examine_here: bool,
    /// Whether the run in the window started from the Install Plan, whose
    /// unfinished entries then stay queued.
    ran_from_plan: bool,
    /// Where theme and icon choices are saved; none in tests.
    settings_path: Option<std::path::PathBuf>,
    /// The banner picture, when the terminal can show one.
    banner: Option<Protocol>,
    /// The Install Plan screen's plan, kept briefly: planning asks the
    /// package manager about every queued package.
    planned: Option<PlanCache>,
    /// How each package entry stands, by entry id, from the last check.
    statuses: HashMap<String, Status>,
    /// The snapshot that check read, for planning updates.
    inventory: Option<Inventory>,
    /// The check running in the background, if any; off in tests.
    checking: Option<mpsc::Receiver<(Inventory, HashMap<String, Status>)>>,
    check_statuses: bool,
    /// What Drift can compare with: saved baselines, then profiles.
    drift_targets: Vec<DriftTarget>,
    /// The one chosen.
    drift_target: usize,
    /// The last comparison, with the target's key.
    drift_report: Option<(String, drift::Report)>,
    /// Where baselines are saved; the user's configuration unless a test says.
    baselines_dir: Option<std::path::PathBuf>,
    /// What the palette's choices do, by index.
    palette_acts: Vec<Act>,
    /// A health check to show once the checks finish, from the palette.
    doctor_wanted: Option<&'static str>,
    /// What this build knows about how its entries were tested; read once.
    evidence: Option<reliability::Evidence>,
    /// The confirmation's lines, to show it again after the change preview.
    confirming: Option<Confirming>,
    /// The Reliability screen's rows: entry ids, names and standing.
    reliability_rows: Vec<(String, String, reliability::Assessment)>,
    coverage: Coverage,
    /// What the Hardware screen last read, and its rows.
    hardware: Option<helixforge_engine::hardware::Hardware>,
    hardware_rows: Vec<HardwareRow>,
    /// A look at the hardware running in the background, if any.
    reading_hardware: Option<mpsc::Receiver<helixforge_engine::hardware::Hardware>>,
    /// The firmware update waiting for its confirming word.
    firmware_pending: Option<Plan>,
    /// The desktop as last read, and the Desktop screen's rows.
    desktop: Option<helixforge_engine::desktop::Desktop>,
    desktop_rows: Vec<DesktopRow>,
    /// Where looks are saved; the user's configuration unless a test says.
    looks_dir: Option<std::path::PathBuf>,
    /// Whether the wizard has been offered, remembered in the settings.
    set_up: bool,
    /// The wizard's workstations, by index, while it is open.
    wizard_ids: Vec<String>,
}

/// A plan of the queue as it was, from when it was made, with what the
/// checks before a run found.
struct PlanCache {
    ids: Vec<String>,
    made: std::time::Instant,
    result: Result<Plan, String>,
    findings: Vec<preflight::Finding>,
}

/// How long a cached plan is trusted, for changes made outside HelixForge.
const PLAN_KEPT: std::time::Duration = std::time::Duration::from_secs(3);

impl App {
    pub fn new(catalog: Catalog, platform: LinuxInfo, args: &TuiArgs) -> Self {
        let menu = build_menu(&catalog);
        let areas = vec![AreaState::default(); catalog.tabs.len()];
        let overview = Overview::gather(&platform);
        let mut app = Self {
            catalog,
            platform,
            theme: args.theme.unwrap_or_default(),
            saved_theme: Theme::default(),
            focus: Focus::Sidebar,
            screen: Screen::Dashboard,
            menu,
            menu_state: ListState::default().with_selected(Some(0)),
            main_state: ListState::default().with_selected(Some(0)),
            areas,
            rows: Vec::new(),
            search: Vec::new(),
            cursor: 0,
            completion: None,
            queue: Vec::new(),
            pending: None,
            popup: None,
            records: Vec::new(),
            overview,
            size_bypass: args.size_bypass,
            skip_confirmation: args.skip_confirmation,
            show_hidden: args.override_validation,
            icons: !args.plain,
            keep_going: false,
            snapshots: true,
            snapshot_tool: None,
            backups: Vec::new(),
            diagnoses: Vec::new(),
            profiles_dir: None,
            favorites: Vec::new(),
            favorites_path: None,
            recent: Vec::new(),
            filter: Filter::All,
            before_search: None,
            examining: None,
            examine_here: false,
            channels: Vec::new(),
            channels_online: false,
            gathering: None,
            ran_from_plan: false,
            settings_path: None,
            banner: None,
            planned: None,
            statuses: HashMap::new(),
            inventory: None,
            checking: None,
            check_statuses: false,
            drift_targets: Vec::new(),
            drift_target: 0,
            drift_report: None,
            baselines_dir: None,
            palette_acts: Vec::new(),
            doctor_wanted: None,
            evidence: None,
            confirming: None,
            reliability_rows: Vec::new(),
            coverage: Coverage::All,
            hardware: None,
            hardware_rows: Vec::new(),
            reading_hardware: None,
            firmware_pending: None,
            desktop: None,
            desktop_rows: Vec::new(),
            looks_dir: None,
            set_up: true,
            wizard_ids: Vec::new(),
        };
        app.update_rows();
        if !args.bypass_root && nix::unistd::Uid::effective().is_root() {
            app.popup = Some(Popup::Text(TextPopup::new(
                "Root user warning",
                ROOT_WARNING,
                true,
            )));
        }
        app
    }

    /// Use and keep the saved look; `--theme` and `--plain` given on the
    /// command line win for this run.
    pub fn remembering(
        mut self,
        look: Look,
        path: Option<std::path::PathBuf>,
        args: &TuiArgs,
    ) -> Self {
        // NO_COLOR applies to this run only; the remembered theme stays.
        self.saved_theme = look.theme;
        self.theme = args.theme.unwrap_or(if theme::no_color() {
            Theme::Monochrome
        } else {
            look.theme
        });
        self.icons = look.icons && !args.plain;
        self.keep_going = look.keep_going;
        if let Some(path) = &path {
            let favorites = settings::favorites_path(path);
            self.favorites = settings::load_favorites(&favorites);
            self.favorites_path = Some(favorites);
        }
        self.snapshots = look.snapshots;
        self.set_up = look.set_up || path.is_none();
        self.snapshot_tool = backups::Snapshots::here();
        self.settings_path = path;
        self
    }

    /// Check in the background how package entries stand, now and after
    /// every run.
    pub fn with_status_checks(mut self) -> Self {
        self.check_statuses = true;
        self.examine_here = true;
        self.refresh_statuses();
        self
    }

    /// Start a fresh status check; the old answers stay shown until it ends.
    fn refresh_statuses(&mut self) {
        if !self.check_statuses {
            return;
        }
        let entries: Vec<Entry> = self
            .catalog
            .entries()
            .into_iter()
            .filter(|entry| entry.identity().is_some())
            .cloned()
            .collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let inventory = Inventory::gather(&platform);
            let statuses = entries
                .iter()
                .filter_map(|entry| {
                    inventory
                        .status(entry, &platform, &which)
                        .map(|status| (entry.id.clone(), status))
                })
                .collect();
            let _ = sender.send((inventory, statuses));
        });
        self.checking = Some(receiver);
    }

    /// Draw this picture as the header where it fits.
    pub fn with_banner(mut self, banner: Option<Protocol>) -> Self {
        self.banner = banner;
        self
    }

    /// The banner, unless icons are off, a theme with few or no colors is in use,
    /// or the terminal is too narrow for it and the status beside it.
    fn banner(&self, width: u16) -> Option<&Protocol> {
        self.banner.as_ref().filter(|banner| {
            self.icons && self.theme.colorful() && banner.size().width + 2 + STATUS_WIDTH <= width
        })
    }

    /// Save the theme and icon choice, when there is somewhere to save it.
    fn save_look(&mut self) {
        let Some(path) = &self.settings_path else {
            return;
        };
        let look = Look {
            theme: self.saved_theme,
            icons: self.icons,
            keep_going: self.keep_going,
            snapshots: self.snapshots,
            set_up: self.set_up,
        };
        if let Err(error) = settings::save(path, &look) {
            tracing::warn!(%error, "the settings were not saved");
        }
    }

    // ----- state helpers -----

    fn current_area(&self) -> Option<usize> {
        match self.screen {
            Screen::Area(index) => Some(index),
            _ => None,
        }
    }

    fn area_mut(&mut self) -> Option<&mut AreaState> {
        self.current_area().map(|index| &mut self.areas[index])
    }

    #[cfg(test)]
    fn at_root(&self) -> bool {
        self.current_area()
            .is_none_or(|index| self.areas[index].path.is_empty())
    }

    fn searching(&self) -> bool {
        !self.search.is_empty()
    }

    fn is_queued(&self, entry: &Entry) -> bool {
        self.queue.iter().any(|other| other.id == entry.id)
    }

    /// The visible rows of an area screen: the current group's items, or
    /// every entry matching the search across all areas.
    fn update_rows(&mut self) {
        let query = self.search.iter().collect::<String>().to_lowercase();
        let filter = self.filter;
        let wanted = |entry: &Entry| filter.allows(self.statuses.get(&entry.id));
        self.rows = if !query.is_empty() {
            // Ranked: the best match first, then by name.
            let mut found: Vec<(u32, Entry)> = self
                .catalog
                .entries()
                .into_iter()
                .filter(|entry| wanted(entry))
                .filter_map(|entry| {
                    let score = helixforge_core::search::score(
                        &query,
                        &entry.name,
                        &entry.id,
                        &entry.description,
                    )?;
                    Some((score, entry.clone()))
                })
                .collect();
            found.sort_by(|a, b| {
                b.0.cmp(&a.0)
                    .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
            });
            found
                .into_iter()
                .map(|(_, entry)| Row::Entry(Box::new(entry)))
                .collect()
        } else if self.screen == Screen::Favorites {
            let mut ids: Vec<&String> = self.favorites.iter().collect();
            ids.extend(self.recent.iter().filter(|id| !self.favorites.contains(id)));
            ids.into_iter()
                .filter_map(|id| self.catalog.find(id))
                .filter(|entry| wanted(entry))
                .map(|entry| Row::Entry(Box::new(entry.clone())))
                .collect()
        } else if let Some(index) = self.current_area() {
            if filter != Filter::All {
                // Filtered: every matching entry of the area, groups flattened.
                let mut found: Vec<Entry> = self.catalog.tabs[index]
                    .entries()
                    .into_iter()
                    .filter(|entry| wanted(entry))
                    .cloned()
                    .collect();
                found.sort_by_key(|entry| entry.name.to_lowercase());
                found
                    .into_iter()
                    .map(|entry| Row::Entry(Box::new(entry)))
                    .collect()
            } else {
                let area = &self.areas[index];
                let path: Vec<usize> = area.path.iter().map(|(group, _)| *group).collect();
                let mut rows = Vec::new();
                if !area.path.is_empty() {
                    rows.push(Row::Up);
                }
                let items = self.catalog.tabs[index].items_at(&path).unwrap_or(&[]);
                rows.extend(items.iter().enumerate().map(|(index, item)| match item {
                    Item::Group(group) => Row::Group {
                        index,
                        name: group.name.clone(),
                    },
                    Item::Entry(entry) => Row::Entry(Box::new(entry.clone())),
                }));
                rows
            }
        } else {
            Vec::new()
        };
        self.completion = if query.is_empty() {
            None
        } else {
            self.rows.iter().find_map(|row| match row {
                Row::Entry(entry) => entry
                    .name
                    .to_lowercase()
                    .strip_prefix(query.as_str())
                    .map(str::to_string),
                _ => None,
            })
        };
        self.clamp_main();
    }

    fn main_len(&self) -> usize {
        match self.screen {
            Screen::Dashboard => QUICK_ACTIONS.len(),
            Screen::Area(_) | Screen::Favorites => self.rows.len(),
            Screen::Plan => self.queue.len(),
            Screen::History => self.records.len(),
            Screen::Backups => self.backups.len(),
            Screen::Updates => self.channels.len(),
            Screen::Drift => self
                .drift_report
                .as_ref()
                .map_or(0, |(_, report)| report.differences.len()),
            Screen::Reliability => self.reliability_rows.len(),
            Screen::Hardware => self.hardware_rows.len(),
            Screen::Desktop => self.desktop_rows.len(),
            Screen::Doctor => self.diagnoses.len() + self.doctor_entries().len(),
            Screen::Settings => self.settings().len(),
        }
    }

    fn clamp_main(&mut self) {
        let count = self.main_len();
        let selected = self.main_state.selected().unwrap_or(0);
        self.main_state.select(Some(if count == 0 {
            0
        } else {
            selected.min(count - 1)
        }));
    }

    fn selected_row(&self) -> Option<&Row> {
        self.rows.get(self.main_state.selected()?)
    }

    fn selected_entry(&self) -> Option<&Entry> {
        match self.selected_row() {
            Some(Row::Entry(entry)) => Some(entry.as_ref()),
            _ => None,
        }
    }

    fn open_screen(&mut self, screen: Screen) {
        // Remember the row in the area being left.
        if let Some(index) = self.current_area() {
            self.areas[index].selected = self.main_state.selected().unwrap_or(0);
        }
        self.screen = screen;
        self.search.clear();
        self.cursor = 0;
        if let Some(position) = self.menu.iter().position(|item| item.screen == screen) {
            self.menu_state.select(Some(position));
        }
        let selected = match screen {
            Screen::Area(index) => self.areas[index].selected,
            _ => 0,
        };
        self.main_state.select(Some(selected));
        if screen == Screen::History {
            self.reload_history();
        }
        if screen == Screen::Backups {
            self.reload_backups();
        }
        if screen == Screen::Favorites {
            self.reload_recent();
        }
        if screen == Screen::Doctor && self.diagnoses.is_empty() {
            self.examine();
        }
        if screen == Screen::Updates && self.channels.is_empty() {
            self.gather_updates(false);
        }
        if screen == Screen::Drift {
            self.reload_drift();
        }
        if screen == Screen::Reliability {
            self.reload_reliability();
        }
        if screen == Screen::Hardware && self.hardware.is_none() {
            self.read_hardware();
        }
        if screen == Screen::Desktop {
            self.reload_desktop();
        }
        self.update_rows();
    }

    fn reload_history(&mut self) {
        self.records = history::read(200).unwrap_or_default();
        self.clamp_main();
    }

    /// Whether something changed without a key press: a finished command.
    pub fn needs_redraw(&mut self) -> bool {
        if let Some(receiver) = &self.examining {
            match receiver.try_recv() {
                Ok(diagnoses) => {
                    self.diagnoses = diagnoses;
                    self.examining = None;
                    self.clamp_main();
                    if let Some(wanted) = self.doctor_wanted.take() {
                        self.show_check(wanted);
                    }
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.examining = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_hardware {
            match receiver.try_recv() {
                Ok(found) => {
                    self.hardware_rows = hardware_rows(&found);
                    self.hardware = Some(found);
                    self.reading_hardware = None;
                    if self.screen == Screen::Hardware {
                        self.clamp_main();
                    }
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_hardware = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.gathering {
            match receiver.try_recv() {
                Ok((channels, online)) => {
                    self.channels = channels;
                    self.channels_online = online;
                    self.gathering = None;
                    if self.screen == Screen::Updates {
                        self.clamp_main();
                    }
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.gathering = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.checking {
            match receiver.try_recv() {
                Ok((inventory, statuses)) => {
                    self.inventory = Some(inventory);
                    self.statuses = statuses;
                    self.checking = None;
                    if self.screen == Screen::Drift {
                        self.check_drift();
                    }
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.checking = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        match &mut self.popup {
            Some(Popup::Running(running)) => running.needs_redraw(),
            _ => false,
        }
    }

    // ----- drawing -----

    fn glyph(&self, icon: Icon) -> &'static str {
        icon.glyph(self.icons)
    }

    /// An icon and the space after it, or nothing when icons are off.
    fn icon_gap(&self, icon: Icon) -> String {
        match self.glyph(icon) {
            "" => String::new(),
            glyph => format!("{glyph}  "),
        }
    }

    fn menu_icon(&self, screen: Screen) -> Icon {
        match screen {
            Screen::Dashboard => Icon::Dashboard,
            Screen::Plan => Icon::Plan,
            Screen::History => Icon::History,
            Screen::Backups => Icon::Backups,
            Screen::Updates => Icon::Updates,
            Screen::Drift => Icon::Drift,
            Screen::Reliability => Icon::Reliability,
            Screen::Hardware => Icon::Hardware,
            Screen::Desktop => Icon::Desktop,
            Screen::Favorites => Icon::Favorites,
            Screen::Doctor => Icon::Doctor,
            Screen::Settings => Icon::Settings,
            Screen::Area(index) => Icon::for_area(&self.catalog.tabs[index].id),
        }
    }

    /// The style of the selected row of a list that has the focus.
    fn highlight(&self, focused: bool) -> Style {
        if focused {
            self.theme.selection()
        } else {
            Style::default()
        }
    }

    /// What marks the selected row of a list.
    fn marker(&self, focused: bool) -> &'static str {
        match (focused, self.icons) {
            (false, _) => "  ",
            (true, true) => "▌ ",
            (true, false) => HIGHLIGHT,
        }
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        frame.render_widget(Block::default().style(self.theme.base()), area);
        let too_small = (area.width < MIN_WIDTH || area.height < MIN_HEIGHT)
            && !self.size_bypass
            && self.popup.is_none();
        if too_small {
            let warning = Paragraph::new(format!(
                "The terminal is {} by {}; HelixForge needs {} by {}.\nEnlarge the window, or start with --size-bypass.",
                area.width, area.height, MIN_WIDTH, MIN_HEIGHT
            ))
            .alignment(Alignment::Center)
            .style(Style::default().fg(self.theme.failure()).bold())
            .wrap(Wrap { trim: true });
            let rows = Layout::vertical([
                Constraint::Fill(1),
                Constraint::Length(4),
                Constraint::Fill(1),
            ])
            .split(area);
            frame.render_widget(warning, rows[1]);
            return;
        }
        let header = if self.banner(area.width).is_some() {
            banner::ROWS + 1
        } else {
            HEADER_HEIGHT
        };
        let rows = Layout::vertical([
            Constraint::Length(header),
            Constraint::Min(1),
            Constraint::Length(2),
        ])
        .split(area);
        self.draw_header(frame, rows[0]);
        let columns = Layout::horizontal([Constraint::Length(SIDEBAR_WIDTH), Constraint::Fill(1)])
            .split(rows[1]);
        self.draw_sidebar(frame, columns[0]);
        let main = columns[1];
        match self.screen {
            Screen::Dashboard => self.draw_dashboard(frame, main),
            Screen::Area(_) => self.draw_area(frame, main),
            Screen::Favorites => self.draw_favorites(frame, main),
            Screen::Plan => self.draw_plan(frame, main),
            Screen::History => self.draw_history(frame, main),
            Screen::Backups => self.draw_backups(frame, main),
            Screen::Updates => self.draw_updates(frame, main),
            Screen::Drift => self.draw_drift(frame, main),
            Screen::Reliability => self.draw_reliability(frame, main),
            Screen::Hardware => self.draw_hardware(frame, main),
            Screen::Desktop => self.draw_desktop(frame, main),
            Screen::Doctor => self.draw_doctor(frame, main),
            Screen::Settings => self.draw_settings(frame, main),
        }
        match &mut self.popup {
            Some(Popup::Text(popup)) => popup.draw(frame, centered(main, 95, 95), self.theme),
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup),
            ) => {
                let area = popup.fitted(centered(main, 80, 70));
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Wizard(popup)) => popup.draw(frame, centered(main, 92, 90), self.theme),
            Some(Popup::Palette(popup)) => {
                let area = popup.fitted(main);
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Effects(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            Some(Popup::Confirm(popup)) => {
                let area = popup.fitted(centered(main, 80, 70));
                popup.draw(frame, area, self.theme)
            }
            Some(Popup::Running(popup)) => popup.draw(frame, centered(main, 96, 96), self.theme),
            None => {}
        }
        self.draw_footer(frame, rows[2]);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let (status, color) = match &self.popup {
            Some(Popup::Running(running)) if !running.is_finished() => {
                ("● RUNNING", self.theme.title())
            }
            _ => ("● SYSTEM READY", self.theme.success()),
        };
        if let Some(banner) = self.banner(area.width) {
            let size = banner.size();
            frame.render_widget(
                Image::new(banner),
                Rect::new(area.x + 2, area.y, size.width, size.height).intersection(area),
            );
            let middle = Rect::new(
                area.x,
                area.y + size.height.saturating_sub(1) / 2,
                area.width.saturating_sub(2),
                1,
            )
            .intersection(area);
            frame.render_widget(
                Paragraph::new(Line::styled(status, Style::default().fg(color).bold()))
                    .alignment(Alignment::Right),
                middle,
            );
            self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
            return;
        }
        // The logo tile, then the wordmark and the tagline beside it.
        let tile = Rect::new(area.x + 2, area.y, 5, 3).intersection(area);
        let inside = self.framed(
            frame,
            tile,
            self.theme.selected_line(),
            self.theme.selected_fill(),
        );
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Logo),
                Style::default().fg(self.theme.focused()).bold(),
            ))
            .alignment(Alignment::Center),
            inside,
        );
        let text = Rect::new(
            area.x + 9,
            area.y,
            area.width.saturating_sub(11),
            area.height.min(3),
        )
        .intersection(area);
        let brand = Text::from(vec![
            Line::from(vec![
                Span::styled("HELIX", Style::default().fg(self.theme.focused()).bold()),
                Span::styled("FORGE", Style::default().fg(self.theme.title()).bold()),
            ]),
            Line::styled(
                "BUILD • CONFIGURE • OPTIMIZE",
                Style::default().fg(self.theme.faint()),
            ),
        ]);
        frame.render_widget(Paragraph::new(brand), text);
        frame.render_widget(
            Paragraph::new(Line::styled(status, Style::default().fg(color).bold()))
                .alignment(Alignment::Right),
            text,
        );
        self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
    }

    /// A rounded card whose fill stays inside its border, as in the design;
    /// returns the space inside.
    fn framed(&self, frame: &mut Frame, area: Rect, border: Color, fill: Style) -> Rect {
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(border));
        let inside = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Block::default().style(fill), inside);
        inside
    }

    /// A horizontal divider.
    fn rule(&self, frame: &mut Frame, x: u16, y: u16, width: u16) {
        frame.render_widget(
            Paragraph::new("─".repeat(width as usize))
                .style(Style::default().fg(self.theme.line())),
            Rect::new(x, y, width, 1).intersection(frame.area()),
        );
    }

    fn draw_sidebar(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Sidebar;
        let edge = Block::default()
            .borders(ratatui::widgets::Borders::RIGHT)
            .border_set(border::PLAIN)
            .border_style(Style::default().fg(self.theme.line()));
        let inner = edge.inner(area);
        frame.render_widget(edge, area);
        let inner = Rect::new(
            inner.x + 1,
            inner.y + 1,
            inner.width.saturating_sub(2),
            inner.height.saturating_sub(1),
        );
        // Menu items by section, in order.
        let mut sections: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (index, item) in self.menu.iter().enumerate() {
            match sections.last_mut() {
                Some((name, items)) if *name == item.section => items.push(index),
                _ => sections.push((item.section, vec![index])),
            }
        }
        // The first section's items are cards when there is room for them.
        // The main menu's items are cards when they fit with a few lines of
        // the other sections below them, which scroll when they must.
        let first = sections[0].1.len();
        let cards = inner.height as usize >= 1 + 3 * first + 1 + 2 + 1 + 6;
        if !cards {
            self.draw_sidebar_rows(frame, inner, &sections, focused);
            return;
        }
        let (name, items) = &sections[0];
        frame.render_widget(
            Paragraph::new(Line::styled(
                *name,
                Style::default().fg(self.theme.faint()).bold(),
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        let mut y = inner.y + 1;
        for &index in items {
            let item = &self.menu[index];
            let current = item.screen == self.screen;
            let label = self.menu_label(item, current);
            let border = match (current, focused) {
                (true, true) => self.theme.focused(),
                (true, false) => self.theme.selected_line(),
                _ => self.theme.line(),
            };
            let fill = if current {
                self.theme.selected_fill()
            } else {
                self.theme.card()
            };
            let inside = self.framed(frame, Rect::new(inner.x, y, inner.width, 3), border, fill);
            frame.render_widget(
                Paragraph::new(label).block(Block::default().padding(Padding::horizontal(1))),
                inside,
            );
            y += 3;
        }
        if sections.len() > 1 {
            y += 1;
            self.rule(frame, inner.x, y, inner.width);
            y += 2;
            let rest = Rect::new(inner.x, y, inner.width, inner.bottom().saturating_sub(y));
            self.draw_sidebar_rows(frame, rest, &sections[1..], focused);
        }
    }

    /// A menu item's icon and label.
    fn menu_label(&self, item: &MenuItem, current: bool) -> Line<'static> {
        let icon = self.icon_gap(self.menu_icon(item.screen));
        let text_style = if current {
            Style::default().fg(self.theme.on_selected()).bold()
        } else {
            Style::default().fg(self.theme.entry())
        };
        Line::from(vec![
            Span::styled(
                icon,
                Style::default().fg(if current {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ),
            Span::styled(item.label.clone(), text_style),
        ])
    }

    /// One menu item as a line, marked when it is the current screen.
    fn menu_row(
        &self,
        frame: &mut Frame,
        row: Rect,
        label: Line<'static>,
        current: bool,
        focused: bool,
    ) {
        let edge = if current && focused {
            Span::styled("▌", Style::default().fg(self.theme.focused()))
        } else {
            Span::raw(" ")
        };
        let mut spans = vec![edge, Span::raw(" ")];
        spans.extend(label.spans);
        let style = if current {
            self.theme.selected_fill()
        } else {
            Style::default()
        };
        frame.render_widget(Paragraph::new(Line::from(spans)).style(style), row);
    }

    /// The menu as one line per item: with a rule and space between the
    /// sections when they fit, without them when not, and scrolled to keep
    /// the current screen in view when even that is too tall.
    fn draw_sidebar_rows(
        &self,
        frame: &mut Frame,
        inner: Rect,
        sections: &[(&'static str, Vec<usize>)],
        focused: bool,
    ) {
        enum Row {
            Rule,
            Blank,
            Heading(&'static str),
            Item(usize),
        }
        let build = |spaced: bool| {
            let mut rows = Vec::new();
            for (number, (name, items)) in sections.iter().enumerate() {
                if number > 0 && spaced {
                    rows.push(Row::Rule);
                    rows.push(Row::Blank);
                }
                rows.push(Row::Heading(name));
                rows.extend(items.iter().map(|&index| Row::Item(index)));
                if spaced {
                    rows.push(Row::Blank);
                }
            }
            rows
        };
        let height = inner.height as usize;
        let mut rows = build(true);
        // The blank after the last section is not needed to fit.
        if rows.len().saturating_sub(1) > height {
            rows = build(false);
        }
        let current = rows
            .iter()
            .position(
                |row| matches!(row, Row::Item(index) if self.menu[*index].screen == self.screen),
            )
            .unwrap_or(0);
        let offset = (current + 1)
            .saturating_sub(height)
            .min(rows.len().saturating_sub(height));
        for (line, row) in rows.iter().skip(offset).take(height).enumerate() {
            let area = Rect::new(inner.x, inner.y + line as u16, inner.width, 1);
            match row {
                Row::Rule => self.rule(frame, area.x, area.y, area.width),
                Row::Blank => {}
                Row::Heading(name) => frame.render_widget(
                    Paragraph::new(Line::styled(
                        *name,
                        Style::default().fg(self.theme.faint()).bold(),
                    )),
                    area,
                ),
                Row::Item(index) => {
                    let item = &self.menu[*index];
                    let current = item.screen == self.screen;
                    self.menu_row(
                        frame,
                        area,
                        self.menu_label(item, current),
                        current,
                        focused,
                    );
                }
            }
        }
    }

    /// A screen's title and subtitle; returns the space below them.
    fn page(&self, frame: &mut Frame, area: Rect, title: &str, subtitle: &str) -> Rect {
        let inner = Rect::new(
            area.x + 2,
            area.y + 1,
            area.width.saturating_sub(4),
            area.height.saturating_sub(1),
        );
        frame.render_widget(
            Paragraph::new(Text::from(vec![
                Line::styled(
                    title.to_string(),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Line::styled(
                    subtitle.to_string(),
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
            inner,
        );
        Rect::new(
            inner.x,
            inner.y + 3,
            inner.width,
            inner.height.saturating_sub(3),
        )
    }

    fn card(&self, title: &str) -> Block<'static> {
        Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(self.theme.line()))
            .title(format!(" {title} "))
            .title_style(Style::default().fg(self.theme.entry()).bold())
            .padding(Padding::horizontal(1))
    }

    /// Fill a card's inside, so the fill never reaches past its rounded
    /// border; draw the card's widget over it afterwards.
    fn fill(&self, frame: &mut Frame, area: Rect) {
        let inside = Block::bordered().inner(area);
        frame.render_widget(Block::default().style(self.theme.card()), inside);
    }

    fn draw_dashboard(&mut self, frame: &mut Frame, area: Rect) {
        let content = self.page(
            frame,
            area,
            "Dashboard",
            "Welcome to your Linux command center.",
        );
        let rows = self.overview.rows.len() as u16;
        let actions = QUICK_ACTIONS.len() as u16;
        let overview_height = |spaced: bool| if spaced { rows * 2 + 1 } else { rows + 2 };
        // Roomy rows and two-line cards when they fit, tighter otherwise.
        let (spaced, card) = [(true, 4), (false, 4), (false, 3), (false, 1)]
            .into_iter()
            .find(|&(spaced, card)| overview_height(spaced) + 3 + actions * card <= content.height)
            .unwrap_or((false, 1));
        let width = content.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line> = Vec::new();
        for (index, (label, value)) in self.overview.rows.iter().enumerate() {
            if spaced && index > 0 {
                lines.push(Line::from(""));
            }
            let gap = width.saturating_sub(label.chars().count() + value.chars().count());
            lines.push(Line::from(vec![
                Span::styled(*label, Style::default().fg(self.theme.muted())),
                Span::raw(" ".repeat(gap)),
                Span::styled(value.clone(), Style::default().fg(self.theme.entry())),
            ]));
        }
        let overview = Rect::new(
            content.x,
            content.y,
            content.width,
            overview_height(spaced).min(content.height),
        );
        let mut block = self.card("System Overview");
        let corner = self.glyph(Icon::Overview);
        if !corner.is_empty() {
            block = block.title(
                Line::styled(
                    format!(" {corner} "),
                    Style::default().fg(self.theme.focused()),
                )
                .right_aligned(),
            );
        }
        self.fill(frame, overview);
        frame.render_widget(Paragraph::new(Text::from(lines)).block(block), overview);
        // A blank line before the heading, unless every action would not fit.
        let gap = u16::from(overview_height(spaced) + 3 + actions * card <= content.height);
        let heading = overview.bottom() + gap;
        if heading >= content.bottom() {
            return;
        }
        frame.render_widget(
            Paragraph::new(Line::styled(
                "Quick Actions",
                Style::default().fg(self.theme.entry()).bold(),
            )),
            Rect::new(content.x, heading, content.width, 1),
        );
        let list = Rect::new(
            content.x,
            heading + 1,
            content.width,
            content.bottom().saturating_sub(heading + 1),
        );
        let focused = self.focus == Focus::Main;
        if card == 1 {
            let items: Vec<Line> = QUICK_ACTIONS
                .iter()
                .map(|(title, detail)| {
                    Line::from(vec![
                        Span::styled(*title, Style::default().fg(self.theme.entry()).bold()),
                        Span::styled(
                            format!("   {detail}"),
                            Style::default().fg(self.theme.muted()),
                        ),
                    ])
                })
                .collect();
            let widget = List::new(items)
                .highlight_style(self.highlight(focused))
                .highlight_symbol(self.marker(focused));
            frame.render_stateful_widget(widget, list, &mut self.main_state);
            return;
        }
        let selected = self.main_state.selected().unwrap_or(0);
        for (index, (title, detail)) in QUICK_ACTIONS.iter().enumerate() {
            let rect = Rect::new(list.x, list.y + index as u16 * card, list.width, card);
            if rect.bottom() > list.bottom() {
                break;
            }
            self.action_card(
                frame,
                rect,
                QUICK_ICONS[index],
                title,
                detail,
                index == selected,
                focused,
            );
        }
    }

    /// A quick action as a card: icon, title, detail and a chevron.
    #[allow(clippy::too_many_arguments)]
    fn action_card(
        &self,
        frame: &mut Frame,
        area: Rect,
        icon: Icon,
        title: &str,
        detail: &str,
        selected: bool,
        focused: bool,
    ) {
        let border = match (selected, focused) {
            (true, true) => self.theme.focused(),
            (true, false) => self.theme.selected_line(),
            _ => self.theme.line(),
        };
        let fill = if selected {
            self.theme.selected_fill()
        } else {
            self.theme.card()
        };
        let inside = self.framed(frame, area, border, fill);
        let inner = Rect::new(
            inside.x + 1,
            inside.y,
            inside.width.saturating_sub(2),
            inside.height,
        );
        let icon = self.icon_gap(icon);
        let indent = " ".repeat(icon.chars().count());
        let title_style = Style::default()
            .fg(if selected {
                self.theme.on_selected()
            } else {
                self.theme.entry()
            })
            .bold();
        let icon = Span::styled(icon, Style::default().fg(self.theme.focused()));
        let lines = if inner.height >= 2 {
            vec![
                Line::from(vec![icon, Span::styled(title.to_string(), title_style)]),
                Line::from(vec![
                    Span::raw(indent),
                    Span::styled(detail.to_string(), Style::default().fg(self.theme.muted())),
                ]),
            ]
        } else {
            vec![Line::from(vec![
                icon,
                Span::styled(title.to_string(), title_style),
                Span::styled(
                    format!("   {detail}"),
                    Style::default().fg(self.theme.muted()),
                ),
            ])]
        };
        let text = Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        frame.render_widget(Paragraph::new(Text::from(lines)), text);
        frame.render_widget(
            Paragraph::new(Line::styled(
                self.glyph(Icon::Chevron),
                Style::default().fg(if selected {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ))
            .alignment(Alignment::Right),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
    }

    fn draw_area(&mut self, frame: &mut Frame, area: Rect) {
        let Some(index) = self.current_area() else {
            return;
        };
        let name = self.catalog.tabs[index].name.clone();
        let trail: Vec<String> = {
            let tab = &self.catalog.tabs[index];
            let mut names = Vec::new();
            let mut items = tab.items.as_slice();
            for (group, _) in &self.areas[index].path {
                if let Some(Item::Group(group)) = items.get(*group) {
                    names.push(group.name.clone());
                    items = &group.items;
                }
            }
            names
        };
        let subtitle = if trail.is_empty() {
            format!("{} entries", self.catalog.tabs[index].entries().len())
        } else {
            trail.join(" › ")
        };
        let content = self.page(frame, area, &name, &subtitle);
        let parts = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(content);
        self.draw_search(frame, parts[0]);
        self.draw_list(frame, parts[1]);
    }

    fn draw_search(&self, frame: &mut Frame, area: Rect) {
        let searching = self.focus == Focus::Search;
        let prefix = self.icon_gap(Icon::Search);
        let mut spans = vec![Span::styled(
            prefix.clone(),
            Style::default().fg(if searching {
                self.theme.focused()
            } else {
                self.theme.muted()
            }),
        )];
        if !searching && self.search.is_empty() {
            spans.push(Span::styled(
                "Type to search every area (/)",
                Style::default().fg(self.theme.muted()),
            ));
        } else {
            spans.push(Span::styled(
                self.search.iter().collect::<String>(),
                Style::default().fg(self.theme.focused()).bold(),
            ));
            if let Some(completion) = &self.completion {
                spans.push(Span::styled(
                    completion.clone(),
                    Style::default().fg(self.theme.muted()),
                ));
            }
        }
        let color = if searching {
            self.theme.focused()
        } else {
            self.theme.line()
        };
        let block = Block::bordered()
            .border_set(border::ROUNDED)
            .border_style(Style::default().fg(color))
            .padding(Padding::horizontal(1));
        self.fill(frame, area);
        if searching {
            let inner = block.inner(area);
            let x =
                inner.x + prefix.chars().count() as u16 + self.search[..self.cursor].len() as u16;
            frame.set_cursor_position((x.min(inner.right().saturating_sub(1)), inner.y));
        }
        frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
    }

    fn draw_list(&mut self, frame: &mut Frame, area: Rect) {
        let focused = self.focus == Focus::Main;
        let title = match (self.searching(), self.screen) {
            (true, _) => "Results",
            (false, Screen::Favorites) => "Favorites",
            (false, _) => "Entries",
        };
        let title = match self.filter {
            Filter::All => title.to_string(),
            filter => format!("{title} · {}", filter.label()),
        };
        let block = self.card(&title).title(
            Line::styled(" STATUS · FLAGS ", Style::default().fg(self.theme.muted()))
                .right_aligned(),
        );
        let marker = self.marker(focused);
        let content_width =
            (block.inner(area).width as usize).saturating_sub(marker.chars().count());
        let group = self.glyph(Icon::Group);
        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{group}  .."))
                    .style(Style::default().fg(self.theme.group())),
                Row::Group { name, .. } => Line::from(format!("{group}  {name}"))
                    .style(Style::default().fg(self.theme.group())),
                Row::Entry(entry) => {
                    let queued = self.is_queued(entry);
                    let (style, icon_color) = if queued {
                        (
                            Style::default().fg(self.theme.focused()).bold(),
                            self.theme.success(),
                        )
                    } else {
                        (Style::default().fg(self.theme.entry()), self.theme.muted())
                    };
                    let icon = if queued {
                        self.glyph(Icon::Queued)
                    } else {
                        self.glyph(Icon::Entry)
                    };
                    // Without icons, a queued entry is marked after its name; a pinned one
                    // always is.
                    let pinned = if self.favorites.contains(&entry.id) {
                        " ★"
                    } else {
                        ""
                    };
                    let suffix =
                        format!("{}{pinned}", if queued && !self.icons { " *" } else { "" });
                    let left = format!("{icon}  {}{suffix}", entry.name);
                    let flags = format!("  {} ", entry.flags);
                    // The status word, then where from, dropped first when short of room.
                    let (word, detail, color) = match self.statuses.get(&entry.id) {
                        Some(status) => (
                            status.label().to_string(),
                            status.detail(),
                            self.status_color(status),
                        ),
                        None => (String::new(), String::new(), self.theme.muted()),
                    };
                    let room = content_width.saturating_sub(left.chars().count() + 2);
                    let mut detail = if detail.is_empty() {
                        detail
                    } else {
                        format!("  {detail}")
                    };
                    let width = |detail: &str| {
                        word.chars().count() + detail.chars().count() + flags.chars().count()
                    };
                    if width(&detail) > room {
                        detail.clear();
                    }
                    let flags = if width(&detail) > room {
                        " ".to_string()
                    } else {
                        flags
                    };
                    let right =
                        word.chars().count() + detail.chars().count() + flags.chars().count();
                    let gap =
                        " ".repeat(content_width.saturating_sub(left.chars().count() + right));
                    Line::from(vec![
                        Span::styled(icon.to_string(), Style::default().fg(icon_color)),
                        Span::styled(format!("  {}{suffix}", entry.name), style),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(color)),
                        Span::styled(detail, Style::default().fg(self.theme.faint())),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ])
                }
            })
            .collect();
        self.fill(frame, area);
        let list = List::new(lines)
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(marker)
            .scroll_padding(1);
        frame.render_stateful_widget(list, area, &mut self.main_state);
    }

    fn draw_plan(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Install Plan",
            "Review planned changes before they run.",
        );
        if self.queue.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "Nothing is queued. In an area, press Space on an entry to add it here.",
                )
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                inner,
            );
            return;
        }
        let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
        let fresh = self
            .planned
            .as_ref()
            .is_some_and(|cache| cache.ids == ids && cache.made.elapsed() < PLAN_KEPT);
        if !fresh {
            let result = plan(self.queue.iter(), &self.platform, &|id| {
                self.catalog.find(id)
            })
            .map_err(|error| error.to_string());
            let findings = match &result {
                Ok(planned) => self.findings(planned),
                Err(_) => Vec::new(),
            };
            self.planned = Some(PlanCache {
                ids,
                made: std::time::Instant::now(),
                result,
                findings,
            });
        }
        let planned = self
            .planned
            .as_ref()
            .map(|cache| cache.result.clone())
            .unwrap_or_else(|| Err("not planned".to_string()));
        let summary = match &planned {
            Ok(planned) => {
                let mut lines = vec![format!(
                    "{} step{}, risk {}{}",
                    planned.steps.len(),
                    if planned.steps.len() == 1 { "" } else { "s" },
                    planned.risk.label(),
                    if planned.privileged {
                        ", asks for your password"
                    } else {
                        ""
                    }
                )];
                if !planned.requirements.is_empty() {
                    lines.push(format!("Needs: {}", planned.requirements.join(", ")));
                }
                lines.extend(
                    self.planned
                        .iter()
                        .flat_map(|cache| cache.findings.iter().map(preflight::Finding::line)),
                );
                lines.extend(planned.notes.iter().cloned());
                lines.extend(
                    planned
                        .blocked
                        .iter()
                        .map(|blocked| format!("Not run: {blocked}")),
                );
                lines
            }
            Err(error) => vec![format!("Cannot plan: {error}")],
        };
        let summary_height = summary.len() as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Length(summary_height),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(
                summary.into_iter().map(Line::from).collect::<Vec<_>>(),
            ))
            .block(self.card("Summary"))
            .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = self.focus == Focus::Main;
        let steps = planned.map(|planned| planned.steps).unwrap_or_default();
        let items: Vec<Line> = self
            .queue
            .iter()
            .map(|entry| {
                let command = steps
                    .iter()
                    .find(|step| step.id() == entry.id)
                    .map(|step| step.display())
                    .unwrap_or_else(|| "nothing to do".to_string());
                Line::from(vec![
                    Span::styled(
                        self.icon_gap(Icon::Queued),
                        Style::default().fg(self.theme.success()),
                    ),
                    Span::styled(
                        entry.name.clone(),
                        Style::default().fg(self.theme.entry()).bold(),
                    ),
                    Span::styled(
                        format!("   {command}"),
                        Style::default().fg(self.theme.muted()),
                    ),
                ])
            })
            .collect();
        self.fill(frame, parts[2]);
        let list = List::new(items)
            .block(self.card("Queued entries"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, parts[2], &mut self.main_state);
    }

    fn draw_backups(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Backups",
            "Configuration backups HelixForge's entries left, newest first.",
        );
        let snapshots = match (self.snapshot_tool, self.snapshots) {
            (Some(tool), true) => vec![
                format!(
                    "{} is set up: a snapshot is taken before runs that change system files, the kernel or disks.",
                    tool.label()
                ),
                format!("Roll back with {}.", tool.how_to_restore()),
            ],
            (Some(tool), false) => vec![
                format!(
                    "{} is set up, but Settings has snapshots before system changes off.",
                    tool.label()
                ),
                format!("Roll back with {}.", tool.how_to_restore()),
            ],
            (None, _) => vec![
                "Neither Snapper (with a root configuration) nor Timeshift is set up, so no snapshots are taken.".to_string(),
            ],
        };
        let closing = "HelixForge restores configuration backups; it does not roll packages back.";
        // The card is as tall as its text once wrapped to the card's width.
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let rows: usize = snapshots
            .iter()
            .map(String::as_str)
            .chain([closing])
            .map(|text| text.chars().count().div_ceil(width).max(1))
            .sum();
        let mut lines: Vec<Line> = snapshots.into_iter().map(Line::from).collect();
        lines.push(Line::styled(
            closing,
            Style::default().fg(self.theme.muted()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Snapshots"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        self.fill(frame, parts[2]);
        if self.backups.is_empty() {
            frame.render_widget(
                Paragraph::new(
                    "No backups yet. Entries that change your configuration or system files keep the old copy here.",
                )
                .block(self.card("Backups"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                parts[2],
            );
            return;
        }
        let home = std::env::var("HOME").unwrap_or_default();
        let shown = |path: &std::path::Path| {
            let text = helixforge_core::printable(&path.display().to_string());
            match text.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => text,
            }
        };
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .backups
            .iter()
            .map(|backup| {
                let (scope, color) = if backup.system {
                    ("system", self.theme.warning())
                } else {
                    ("user  ", self.theme.success())
                };
                Line::from(vec![
                    Span::styled(
                        format!("{:<21}", backup.taken.as_deref().unwrap_or("unknown time")),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(format!("{scope}  "), Style::default().fg(color)),
                    Span::styled(
                        shown(&backup.original),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        if backup.directory { "/" } else { "" },
                        Style::default().fg(self.theme.entry()),
                    ),
                ])
            })
            .collect();
        let list = List::new(items)
            .block(self.card("Backups"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, parts[2], &mut self.main_state);
    }

    fn draw_history(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(frame, area, "History", "Past runs, newest first.");
        if self.records.is_empty() {
            frame.render_widget(
                Paragraph::new("No runs recorded yet.")
                    .style(Style::default().fg(self.theme.muted())),
                inner,
            );
            return;
        }
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .records
            .iter()
            .map(|record| {
                let (status, color) = if record.success {
                    ("ok", self.theme.success())
                } else {
                    ("failed", self.theme.failure())
                };
                Line::from(vec![
                    Span::styled("● ", Style::default().fg(color)),
                    Span::styled(format!("{status:<8}"), Style::default().fg(color).bold()),
                    Span::styled(
                        format!("{:<22}", timestamp(record.unix_timestamp)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        record.names.join(", "),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        format!("   {}", record.result),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        if record.log.is_some() { "   log" } else { "" },
                        Style::default().fg(self.theme.focused()),
                    ),
                ])
            })
            .collect();
        self.fill(frame, inner);
        let list = List::new(items)
            .block(self.card("Runs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn settings(&self) -> Vec<(String, String)> {
        vec![
            ("Theme".to_string(), self.theme.label().to_string()),
            (
                "Icons".to_string(),
                if self.icons {
                    "Nerd Font".to_string()
                } else {
                    "plain".to_string()
                },
            ),
            (
                "Ask before running".to_string(),
                if self.skip_confirmation {
                    "no".to_string()
                } else {
                    "yes".to_string()
                },
            ),
            (
                "Show entries hidden on this machine".to_string(),
                if self.show_hidden {
                    "yes".to_string()
                } else {
                    "no".to_string()
                },
            ),
            (
                "After a failed step".to_string(),
                if self.keep_going {
                    "continue".to_string()
                } else {
                    "stop".to_string()
                },
            ),
            (
                "Snapshot before system changes".to_string(),
                match (self.snapshot_tool, self.snapshots) {
                    (None, _) => "not set up here".to_string(),
                    (Some(tool), true) => format!("yes ({})", tool.label()),
                    (Some(_), false) => "no".to_string(),
                },
            ),
            ("Setup wizard".to_string(), "Enter opens it".to_string()),
            (
                "Log level (HELIXFORGE_LOG)".to_string(),
                std::env::var("HELIXFORGE_LOG").unwrap_or_else(|_| "info".to_string()),
            ),
        ]
    }

    fn draw_settings(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Settings",
            "Enter changes the selected setting; theme and icons are remembered.",
        );
        let focused = self.focus == Focus::Main;
        let width = inner.width.saturating_sub(6) as usize;
        let items: Vec<Line> = self
            .settings()
            .into_iter()
            .map(|(label, value)| {
                let gap = width.saturating_sub(label.len() + value.len());
                Line::from(vec![
                    Span::styled(label, Style::default().fg(self.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(self.theme.focused()).bold()),
                ])
            })
            .collect();
        self.fill(frame, inner);
        let list = List::new(items)
            .block(self.card("Settings"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        self.rule(frame, area.x, area.y, area.width);
        let (_, hints) = self.hints();
        // Help and quit sit on the right, as in the design.
        let (right, mut left): (Vec<&Hint>, Vec<&Hint>) = hints
            .iter()
            .partition(|hint| matches!(hint.keys(), ["?"] | ["q"]));
        // Hints that do not fit beside help and quit are left out, last first.
        let width = |hints: &[&Hint]| -> usize {
            hints
                .iter()
                .map(|hint| hint.keys().join("/").chars().count() + 1 + hint.text().chars().count())
                .sum::<usize>()
                + hints.len().saturating_sub(1) * 3
        };
        let room = area.width.saturating_sub(2) as usize;
        while !left.is_empty() && width(&left) + width(&right) + 2 > room {
            left.pop();
        }
        let spans = |hints: &[&Hint]| {
            let mut spans: Vec<Span> = Vec::new();
            for (index, hint) in hints.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(self.theme.faint())));
                }
                spans.push(Span::styled(
                    hint.keys().join("/"),
                    Style::default().fg(self.theme.entry()).bold(),
                ));
                spans.push(Span::styled(
                    format!(" {}", hint.text()),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            spans
        };
        let row =
            Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), 1).intersection(area);
        frame.render_widget(Paragraph::new(Line::from(spans(&left))), row);
        frame.render_widget(
            Paragraph::new(Line::from(spans(&right))).alignment(Alignment::Right),
            row,
        );
    }

    fn hints(&self) -> (&'static str, Vec<Hint>) {
        match &self.popup {
            Some(Popup::Text(popup)) => return popup.hints(),
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup),
            ) => return popup.hints(),
            Some(Popup::Wizard(popup)) => return popup.hints(),
            Some(Popup::Palette(popup)) => return popup.hints(),
            Some(Popup::Effects(popup)) => return popup.hints(),
            Some(Popup::Confirm(popup)) => return popup.hints(),
            Some(Popup::Running(popup)) => return popup.hints(),
            None => {}
        }
        let mut list = Vec::new();
        match self.focus {
            Focus::Search => {
                list.push(Hint::new(&["Enter"], "Keep results"));
                list.push(Hint::new(&["Esc"], "Clear"));
                return ("Search", list);
            }
            Focus::Sidebar => {
                list.push(Hint::new(&["↑↓"], "Navigate"));
                list.push(Hint::new(&["Enter"], "Open"));
            }
            Focus::Main => match self.screen {
                Screen::Dashboard => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    list.push(Hint::new(&["Enter"], "Select"));
                }
                Screen::Area(_) | Screen::Favorites => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_row() {
                        Some(Row::Up) => list.push(Hint::new(&["Enter", "h"], "Parent")),
                        Some(Row::Group { .. }) => list.push(Hint::new(&["Enter"], "Open")),
                        Some(Row::Entry(entry)) => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            if removable(entry) {
                                list.push(Hint::new(&["r"], "Remove"));
                            }
                            if let Some(Status::Update { .. }) = self.statuses.get(&entry.id) {
                                list.push(Hint::new(&["u"], "Update"));
                            }
                            list.push(Hint::new(&["Space"], "Plan"));
                            list.push(Hint::new(&["d"], "Describe"));
                            list.push(Hint::new(&["p"], "Preview"));
                            list.push(Hint::new(&["v"], "Trust"));
                            list.push(Hint::new(
                                &["*"],
                                if self.favorites.contains(&entry.id) {
                                    "Unpin"
                                } else {
                                    "Pin"
                                },
                            ));
                        }
                        None => {}
                    }
                    list.push(Hint::new(&["i"], self.filter_hint()));
                }
                Screen::Plan => {
                    list.push(Hint::new(&["Enter"], "Run plan"));
                    if !self.queue.is_empty() {
                        list.push(Hint::new(&["e"], "Changes"));
                        list.push(Hint::new(&["s"], "Save as profile"));
                    }
                    list.push(Hint::new(&["x"], "Remove"));
                    list.push(Hint::new(&["c"], "Clear"));
                }
                Screen::History => {
                    list.push(Hint::new(&["Enter"], "Show log"));
                    list.push(Hint::new(&["r"], "Reload"));
                }
                Screen::Doctor => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_diagnosis() {
                        Some(diagnosis) => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            if diagnosis.repair.is_some() {
                                list.push(Hint::new(&["f"], "Repair"));
                            } else if diagnosis.entry.is_some() {
                                list.push(Hint::new(&["f"], "Run the entry"));
                            }
                        }
                        None => list.push(Hint::new(&["Enter"], "Run")),
                    }
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Updates => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if !self.channels.is_empty() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        list.push(Hint::new(&["u"], "Update"));
                        list.push(Hint::new(&["U"], "Update all"));
                    }
                    list.push(Hint::new(&["r"], "Check online"));
                }
                Screen::Drift => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(difference) = self.selected_difference() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        if difference.entry.is_some() && restorable(difference.kind) {
                            list.push(Hint::new(&["Space"], "Plan"));
                        }
                    }
                    if self
                        .drift_report
                        .as_ref()
                        .is_some_and(|(_, report)| !report.restore().is_empty())
                    {
                        list.push(Hint::new(&["a"], "Plan all missing"));
                    }
                    if self.drift_targets.len() > 1 {
                        list.push(Hint::new(&["b"], "Compare with"));
                    }
                    list.push(Hint::new(&["s"], "Save baseline"));
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Desktop => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_desktop_row() {
                        Some(DesktopRow::Entry { .. }) => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            list.push(Hint::new(&["Space"], "Plan"));
                        }
                        Some(DesktopRow::Look { .. }) => list.push(Hint::new(&["Enter"], "Apply")),
                        Some(DesktopRow::Info { .. }) => {
                            list.push(Hint::new(&["Enter"], "Details"))
                        }
                        _ => {}
                    }
                    list.push(Hint::new(&["s"], "Save this look"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Hardware => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(HardwareRow::Item { act, .. }) = self.selected_hardware() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        match act {
                            Some(HardwareAct::Firmware(_)) => {
                                list.push(Hint::new(&["f"], "Update firmware"))
                            }
                            Some(HardwareAct::Smart(_)) => {
                                list.push(Hint::new(&["s"], "SMART check"))
                            }
                            None => {}
                        }
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Reliability => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if !self.reliability_rows.is_empty() {
                        list.push(Hint::new(&["Enter"], "Details"));
                        list.push(Hint::new(&["v"], "Trust"));
                    }
                    list.push(Hint::new(
                        &["i"],
                        match self.coverage {
                            Coverage::All => "Show: all",
                            Coverage::Critical => "Show: critical",
                            Coverage::Untested => "Show: untested",
                        },
                    ));
                }
                Screen::Backups => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if !self.backups.is_empty() {
                        list.push(Hint::new(&["Enter"], "Restore"));
                        list.push(Hint::new(&["e"], "Side by side"));
                        list.push(Hint::new(&["d"], "Details"));
                    }
                    list.push(Hint::new(&["r"], "Reload"));
                }
                Screen::Settings => list.push(Hint::new(&["Enter"], "Change")),
            },
        }
        list.push(Hint::new(&["/"], "Search"));
        list.push(Hint::new(&["^K"], "Commands"));
        list.push(Hint::new(&["?"], "Help"));
        list.push(Hint::new(&["q"], "Quit"));
        ("HelixForge", list)
    }

    // ----- keys -----

    /// Returns false when HelixForge should exit.
    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let control_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        match &mut self.popup {
            Some(Popup::Text(popup)) => {
                if popup.handle_key(key) {
                    self.popup = None;
                }
                return true;
            }
            Some(Popup::ProfileName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => self.save_profile(&name),
                }
                return true;
            }
            Some(Popup::Wizard(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    WizardOutcome::Open => {}
                    WizardOutcome::Closed => self.close_wizard(),
                    WizardOutcome::Chosen(index) => {
                        let picks = self.wizard_picks(index);
                        if let Some(Popup::Wizard(popup)) = &mut self.popup {
                            popup.pick(picks);
                        }
                    }
                    WizardOutcome::Queue(ids) => {
                        self.close_wizard();
                        self.queue_from_wizard(&ids);
                    }
                }
                return true;
            }
            Some(Popup::LookName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => self.save_look_as(&name),
                }
                return true;
            }
            Some(Popup::FirmwareWord(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => {
                        self.popup = None;
                        self.firmware_pending = None;
                    }
                    Answer::Given(word) => {
                        self.popup = None;
                        match self.firmware_pending.take() {
                            Some(planned) if word.trim() == "FIRMWARE" => {
                                self.confirm_plan(Ok(planned))
                            }
                            _ => {
                                self.popup = Some(Popup::Text(TextPopup::new(
                                    "Not confirmed",
                                    "The firmware update needs the word FIRMWARE, in capitals. Nothing was changed.",
                                    true,
                                )))
                            }
                        }
                    }
                }
                return true;
            }
            Some(Popup::BaselineName(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(name) => self.save_baseline(&name),
                }
                return true;
            }
            Some(Popup::Effects(popup)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    EffectsOutcome::Open => {}
                    EffectsOutcome::Closed => {
                        let back = popup.from_confirmation;
                        self.popup = None;
                        if back && let Some((header, lines)) = &self.confirming {
                            self.popup = Some(Popup::Confirm(ConfirmPopup::new(header, lines)));
                        }
                    }
                    EffectsOutcome::Download => {
                        let (planned, back) = (popup.plan.clone(), popup.from_confirmation);
                        self.download_sources(&planned, back);
                    }
                }
                return true;
            }
            Some(Popup::Palette(popup)) => {
                match popup.handle_key(key) {
                    Outcome::Open => {}
                    Outcome::Closed => self.popup = None,
                    Outcome::Chosen(index) => {
                        self.popup = None;
                        if let Some(act) = self.palette_acts.get(index).cloned() {
                            return self.act(act);
                        }
                    }
                }
                return true;
            }
            Some(Popup::Confirm(popup)) => {
                if control_c {
                    return false;
                }
                popup.handle_key(key);
                match popup.decision {
                    Decision::Run => {
                        self.popup = None;
                        self.run_pending();
                    }
                    Decision::Cancel => {
                        self.popup = None;
                        self.pending = None;
                        self.confirming = None;
                    }
                    Decision::Effects => {
                        if let Some(planned) = self.pending.clone() {
                            self.show_effects(&planned, true);
                        }
                    }
                    Decision::Pending => {}
                }
                return true;
            }
            Some(Popup::Running(popup)) => {
                if popup.handle_key(key) {
                    // What is installed may have changed.
                    self.planned = None;
                    let unfinished = popup.unfinished();
                    let summary = popup.summary();
                    self.popup = popup
                        .take_problem()
                        .map(|problem| Popup::Text(TextPopup::new("Not recorded", problem, true)));
                    if std::mem::take(&mut self.ran_from_plan) {
                        let mut kept: Vec<Entry> = Vec::new();
                        for id in &unfinished {
                            if let Some(entry) = self.catalog.find(id)
                                && !kept.iter().any(|kept| kept.id == entry.id)
                            {
                                kept.push(entry.clone());
                            }
                        }
                        self.queue = kept;
                        self.clamp_main();
                        if !self.queue.is_empty() && self.popup.is_none() {
                            self.popup = Some(Popup::Text(TextPopup::new(
                                "Not everything finished",
                                format!(
                                    "{summary}\n\nWhat did not finish stays in the Install Plan; Enter there runs it again."
                                ),
                                true,
                            )));
                        }
                    }
                    self.refresh_statuses();
                    if self.screen == Screen::Updates {
                        self.gather_updates(false);
                    }
                    if self.screen == Screen::History {
                        self.reload_history();
                    }
                }
                return true;
            }
            None => {}
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('k') {
            self.open_palette();
            return true;
        }
        if self.focus == Focus::Search {
            self.handle_search_key(key);
            return true;
        }
        if control_c || key.code == KeyCode::Char('q') {
            return false;
        }
        match key.code {
            KeyCode::Tab => self.next_menu(1),
            KeyCode::BackTab => self.next_menu(-1),
            KeyCode::Char('/') => {
                if self.current_area().is_none() {
                    self.open_screen(Screen::Area(self.first_area()));
                }
                // Escape comes back to this row.
                if self.search.is_empty() {
                    self.before_search = self.main_state.selected();
                }
                self.focus = Focus::Search;
                self.cursor = self.search.len();
            }
            KeyCode::Char('?') => {
                self.popup = Some(Popup::Text(TextPopup::new("Keys", HELP, false)));
            }
            KeyCode::Char('t') | KeyCode::Char('T') => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            KeyCode::Char('g') | KeyCode::Char('G') if matches!(self.screen, Screen::Area(_)) => {
                self.popup = Some(Popup::Text(TextPopup::new("Flags", FLAGS_GUIDE, true)));
            }
            _ => match self.focus {
                Focus::Sidebar => self.handle_sidebar_key(key),
                Focus::Main => self.handle_main_key(key),
                Focus::Search => {}
            },
        }
        true
    }

    fn first_area(&self) -> usize {
        self.menu
            .iter()
            .find_map(|item| match item.screen {
                Screen::Area(index) => Some(index),
                _ => None,
            })
            .unwrap_or(0)
    }

    fn handle_sidebar_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.next_menu(1),
            KeyCode::Char('k') | KeyCode::Up => self.next_menu(-1),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.focus = Focus::Main,
            _ => {}
        }
    }

    fn next_menu(&mut self, step: isize) {
        let count = self.menu.len() as isize;
        let current = self.menu_state.selected().unwrap_or(0) as isize;
        let next = ((current + step).rem_euclid(count)) as usize;
        let screen = self.menu[next].screen;
        self.open_screen(screen);
    }

    fn handle_main_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            _ => {}
        }
        match self.screen {
            Screen::Dashboard => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    let screen = match self.main_state.selected().unwrap_or(0) {
                        0 => Screen::Area(self.first_area()),
                        1 => Screen::Plan,
                        2 => Screen::Doctor,
                        4 => Screen::Favorites,
                        _ => Screen::History,
                    };
                    self.open_screen(screen);
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Area(_) | Screen::Favorites => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.enter(),
                KeyCode::Char('*') => self.toggle_favorite(),
                KeyCode::Char('i') => {
                    self.filter = self.filter.next();
                    self.update_rows();
                    self.main_state.select(Some(0));
                }
                KeyCode::Char('h') | KeyCode::Left => self.back(),
                KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                KeyCode::Char('d') | KeyCode::Char('D') => self.describe(),
                KeyCode::Char('v') => {
                    if let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) {
                        self.inspect(&id);
                    }
                }
                KeyCode::Char(' ') => self.toggle_queue(),
                KeyCode::Char('r') | KeyCode::Char('R') => {
                    if let Some(entry) = self.selected_entry().cloned()
                        && removable(&entry)
                    {
                        let planned = plan_removal([&entry], &self.platform);
                        self.confirm_plan(planned);
                    }
                }
                KeyCode::Char('u') => {
                    if let Some(entry) = self.selected_entry().cloned() {
                        self.update(vec![entry]);
                    }
                }
                KeyCode::Char('U') => {
                    let waiting: Vec<Entry> = self
                        .catalog
                        .entries()
                        .into_iter()
                        .filter(|entry| {
                            matches!(self.statuses.get(&entry.id), Some(Status::Update { .. }))
                        })
                        .cloned()
                        .collect();
                    if waiting.is_empty() {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Nothing to update",
                            "No package entry has an update in the repository metadata on this machine.",
                            true,
                        )));
                    } else {
                        self.update(waiting);
                    }
                }
                _ => {}
            },
            Screen::Plan => match key.code {
                KeyCode::Enter => self.confirm(self.queue.clone()),
                KeyCode::Char('x') | KeyCode::Delete => {
                    if let Some(selected) = self.main_state.selected()
                        && selected < self.queue.len()
                    {
                        self.queue.remove(selected);
                        self.clamp_main();
                    }
                }
                KeyCode::Char('c') => {
                    self.queue.clear();
                    self.clamp_main();
                }
                KeyCode::Char('e') if !self.queue.is_empty() => {
                    match plan(self.queue.iter(), &self.platform, &|id| {
                        self.catalog.find(id)
                    }) {
                        Ok(planned) => self.show_effects(&planned, false),
                        Err(error) => {
                            self.popup = Some(Popup::Text(TextPopup::new(
                                "Cannot plan",
                                error.to_string(),
                                true,
                            )))
                        }
                    }
                }
                KeyCode::Char('s') if !self.queue.is_empty() => {
                    self.popup = Some(Popup::ProfileName(InputPopup::new(
                        "Save as a profile",
                        "A name for this profile (it is saved in ~/.config/helixforge/profiles):",
                    )));
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::History => match key.code {
                KeyCode::Enter => self.show_log(),
                KeyCode::Char('r') => self.reload_history(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Doctor => match key.code {
                KeyCode::Enter => match self.selected_diagnosis() {
                    Some(_) => self.describe_diagnosis(),
                    None => {
                        let index = self
                            .main_state
                            .selected()
                            .unwrap_or(0)
                            .saturating_sub(self.diagnoses.len());
                        if let Some(entry) = self.doctor_entries().get(index).cloned() {
                            self.confirm(vec![entry]);
                        }
                    }
                },
                KeyCode::Char('f') => self.repair_selected(),
                KeyCode::Char('r') => self.examine(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Updates => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_channel(),
                KeyCode::Char('u') => match self.selected_channel().cloned() {
                    Some(channel) if channel.commands.is_empty() => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Nothing to run",
                            format!("{}: {}", channel.name, channel.note),
                            true,
                        )));
                    }
                    Some(channel) => self.confirm_plan(Ok(updates::plan(&[&channel]))),
                    None => {}
                },
                KeyCode::Char('U') => self.update_every_channel(),
                KeyCode::Char('r') => self.gather_updates(true),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Desktop => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => match self.selected_desktop_row().cloned() {
                    Some(DesktopRow::Info { text, details }) => {
                        self.popup = Some(Popup::Text(TextPopup::new(text, details, true)));
                    }
                    Some(DesktopRow::Entry { id, .. }) => {
                        if let Some(entry) = self.catalog.find(&id).cloned() {
                            self.confirm(vec![entry]);
                        }
                    }
                    Some(DesktopRow::Look { slug, .. }) => self.apply_look(&slug),
                    _ => {}
                },
                KeyCode::Char(' ') => {
                    if let Some(DesktopRow::Entry { id, .. }) = self.selected_desktop_row().cloned()
                        && let Some(entry) = self.catalog.find(&id).cloned()
                    {
                        self.toggle_queued(entry);
                        self.reload_desktop();
                    }
                }
                KeyCode::Char('s') => {
                    self.popup = Some(Popup::LookName(InputPopup::new(
                        "Save this look",
                        "A name for the desktop's look (it is saved in ~/.config/helixforge/looks):",
                    )));
                }
                KeyCode::Char('r') => self.reload_desktop(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Hardware => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => {
                    if let Some(HardwareRow::Item { text, details, .. }) = self.selected_hardware()
                    {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            text.clone(),
                            details.clone(),
                            true,
                        )));
                    }
                }
                KeyCode::Char('f') => {
                    if let Some(HardwareRow::Item {
                        act: Some(HardwareAct::Firmware(index)),
                        ..
                    }) = self.selected_hardware()
                        && let Some(update) = self
                            .hardware
                            .as_ref()
                            .and_then(|found| found.firmware.updates.get(*index))
                        && let Some(planned) = helixforge_engine::hardware::plan_firmware(update)
                    {
                        let prompt = format!(
                            "Updating firmware can leave {} unusable if it is interrupted. Keep the machine on mains power. Type FIRMWARE to see the plan:",
                            update.device
                        );
                        self.firmware_pending = Some(planned);
                        self.popup = Some(Popup::FirmwareWord(InputPopup::new(
                            "Firmware update",
                            prompt,
                        )));
                    }
                }
                KeyCode::Char('s') => {
                    if let Some(HardwareRow::Item {
                        act: Some(HardwareAct::Smart(disk)),
                        ..
                    }) = self.selected_hardware()
                        && let Some(planned) = helixforge_engine::hardware::plan_smart(disk)
                    {
                        self.confirm_plan(Ok(planned));
                    }
                }
                KeyCode::Char('r') => self.read_hardware(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Reliability => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_reliability(),
                KeyCode::Char('v') => {
                    if let Some((id, _, _)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                    {
                        let id = id.clone();
                        self.inspect(&id);
                    }
                }
                KeyCode::Char('i') => {
                    self.coverage = self.coverage.next();
                    self.main_state.select(Some(0));
                    self.reload_reliability();
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Drift => match key.code {
                KeyCode::Enter | KeyCode::Char('d') => self.describe_difference(),
                KeyCode::Char(' ') => {
                    if let Some(difference) = self.selected_difference()
                        && restorable(difference.kind)
                        && let Some(entry) = difference
                            .entry
                            .as_deref()
                            .and_then(|id| self.catalog.find(id))
                            .cloned()
                    {
                        self.toggle_queued(entry);
                    }
                }
                KeyCode::Char('a') => self.queue_missing(),
                KeyCode::Char('b') => self.next_drift_target(1),
                KeyCode::Char('B') => self.next_drift_target(-1),
                KeyCode::Char('s') => {
                    self.popup = Some(Popup::BaselineName(InputPopup::new(
                        "Save this machine as a baseline",
                        "A name for the baseline (it is saved in ~/.config/helixforge/baselines):",
                    )));
                }
                KeyCode::Char('r') => {
                    self.reload_drift();
                    self.refresh_statuses();
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Backups => match key.code {
                KeyCode::Enter => {
                    if let Some(backup) = self.selected_backup().cloned() {
                        self.confirm_plan(Ok(backups::plan_restore(&backup)));
                    }
                }
                KeyCode::Char('d') => self.describe_backup(),
                KeyCode::Char('e') => {
                    if let Some(backup) = self.selected_backup().cloned() {
                        self.show_effects(&backups::plan_restore(&backup), false);
                    }
                }
                KeyCode::Char('r') => self.reload_backups(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Settings => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.change_setting()
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
        }
    }

    fn handle_search_key(&mut self, key: &KeyEvent) {
        match key.code {
            KeyCode::Enter => {
                self.focus = Focus::Main;
                self.main_state.select(Some(0));
            }
            KeyCode::Esc => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
                self.main_state
                    .select(Some(self.before_search.take().unwrap_or(0)));
                self.clamp_main();
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.clear();
                self.cursor = 0;
                self.focus = Focus::Main;
                self.update_rows();
            }
            KeyCode::Tab => {
                if let Some(completion) = self.completion.take() {
                    self.search.extend(completion.chars());
                    self.cursor = self.search.len();
                    self.update_rows();
                }
            }
            KeyCode::Backspace => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Delete => {
                if self.cursor < self.search.len() {
                    self.search.remove(self.cursor);
                    self.update_rows();
                }
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.search.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.search.len(),
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.search.insert(self.cursor, character);
                self.cursor += 1;
                self.update_rows();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, step: isize) {
        let count = self.main_len();
        if count == 0 {
            return;
        }
        let current = self.main_state.selected().unwrap_or(0) as isize;
        self.main_state
            .select(Some((current + step).clamp(0, count as isize - 1) as usize));
    }

    fn enter(&mut self) {
        match self.selected_row().cloned() {
            Some(Row::Up) => self.back(),
            Some(Row::Group { index, .. }) => {
                let row = self.main_state.selected().unwrap_or(0);
                if let Some(area) = self.area_mut() {
                    area.path.push((index, row));
                }
                self.main_state.select(Some(0));
                self.update_rows();
            }
            Some(Row::Entry(entry)) => self.confirm(vec![*entry]),
            None => {}
        }
    }

    fn back(&mut self) {
        if self.searching() {
            self.search.clear();
            self.cursor = 0;
            self.update_rows();
            self.main_state
                .select(Some(self.before_search.take().unwrap_or(0)));
            self.clamp_main();
            return;
        }
        // A filtered list is flat, so there is no group to leave.
        let popped = if self.filter == Filter::All {
            self.area_mut().and_then(|area| area.path.pop())
        } else {
            None
        };
        match popped {
            Some((_, row)) => {
                self.update_rows();
                self.main_state.select(Some(row));
            }
            None => self.focus = Focus::Sidebar,
        }
    }

    /// Pin the selected entry, or unpin it, and remember it.
    fn toggle_favorite(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        match self.favorites.iter().position(|id| *id == entry.id) {
            Some(position) => {
                self.favorites.remove(position);
            }
            None => self.favorites.push(entry.id.clone()),
        }
        if let Some(path) = &self.favorites_path
            && let Err(error) = settings::save_favorites(path, &self.favorites)
        {
            tracing::warn!(%error, "the favorites were not saved");
        }
        if self.screen == Screen::Favorites {
            self.update_rows();
        }
    }

    /// The entries run recently, newest first, from the history.
    fn reload_recent(&mut self) {
        self.take_recent(history::read(50).unwrap_or_default());
    }

    fn take_recent(&mut self, records: Vec<history::Record>) {
        let mut recent: Vec<String> = Vec::new();
        for record in records {
            for id in record.entries {
                if !recent.contains(&id) && self.catalog.find(&id).is_some() {
                    recent.push(id);
                }
            }
            if recent.len() >= 10 {
                break;
            }
        }
        recent.truncate(10);
        self.recent = recent;
    }

    fn filter_hint(&self) -> &'static str {
        match self.filter {
            Filter::All => "Show: all",
            Filter::Installed => "Show: installed",
            Filter::Updates => "Show: updates",
        }
    }

    fn draw_favorites(&mut self, frame: &mut Frame, area: Rect) {
        let content = self.page(
            frame,
            area,
            "Favorites",
            "Entries pinned with *, then the ones run recently.",
        );
        if self.rows.is_empty() {
            self.fill(frame, content);
            frame.render_widget(
                Paragraph::new(if self.filter == Filter::All {
                    "Nothing here yet. Press * on an entry anywhere to pin it here; entries you run appear here too."
                } else {
                    "Nothing here matches the filter; i changes it."
                })
                .block(self.card("Favorites"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
                content,
            );
            return;
        }
        self.draw_list(frame, content);
    }

    fn toggle_queue(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        self.toggle_queued(entry);
    }

    /// Add an entry to the Install Plan, or take it out.
    fn toggle_queued(&mut self, entry: Entry) {
        if !entry.multi_select {
            self.popup = Some(Popup::Text(TextPopup::new(
                entry.name.clone(),
                "This entry runs on its own and cannot be queued with others. Press Enter to run it.",
                true,
            )));
            return;
        }
        if let Some(position) = self.queue.iter().position(|other| other.id == entry.id) {
            self.queue.remove(position);
        } else {
            self.queue.push(entry);
        }
    }

    fn preview(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let text = match &entry.action {
            Action::Script { file, .. } => std::fs::read_to_string(file)
                .unwrap_or_else(|error| format!("The script cannot be read: {error}")),
            Action::Command { line } => line.clone(),
            _ => match plan([entry], &self.platform, &|id| self.catalog.find(id)) {
                Ok(planned) => {
                    let mut lines = planned.notes.clone();
                    for step in &planned.steps {
                        lines.push(step.display());
                        lines.extend(step.fallbacks());
                    }
                    lines.join("\n")
                }
                Err(error) => error.to_string(),
            },
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Preview: {}", entry.name),
            text,
            false,
        )));
    }

    /// The color of a status word.
    fn status_color(&self, status: &Status) -> Color {
        match status {
            Status::Installed { .. } | Status::Elsewhere { .. } => self.theme.success(),
            Status::Update { .. } => self.theme.warning(),
            Status::Managed { .. } => self.theme.title(),
            Status::Absent { .. } => self.theme.muted(),
        }
    }

    fn describe(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        let mut text = if entry.description.is_empty() {
            "This entry has no description.".to_string()
        } else {
            entry.description.clone()
        };
        if let Some(status) = self.statuses.get(&entry.id) {
            let detail = status.detail();
            text.push_str(&format!("\n\nStatus: {}", status.label()));
            if !detail.is_empty() {
                text.push_str(&format!(" ({detail})"));
            }
            if let Status::Update { .. } = status {
                text.push_str(
                    "\nThe repository metadata on this machine has a newer version: u updates it (on Arch, with the whole system).",
                );
            }
        }
        self.popup = Some(Popup::Text(TextPopup::new(entry.name.clone(), text, true)));
    }

    /// Save the queue as a profile and show it in the Profiles area.
    fn save_profile(&mut self, name: &str) {
        let profile = profiles::from_queue(name, &self.queue);
        let directory = self.profiles_dir.clone().or_else(profiles::dir);
        let saved = directory
            .as_deref()
            .ok_or_else(|| "cannot find the configuration folder".to_string())
            .and_then(|directory| {
                profiles::save(directory, &profile, &self.catalog)
                    .map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok(path) => {
                if let Some(directory) = &directory {
                    profiles::add_to(&mut self.catalog, directory);
                    self.menu = build_menu(&self.catalog);
                    self.update_rows();
                }
                TextPopup::new(
                    "Profile saved",
                    format!(
                        "{} ({} entries) is saved as {}.\n\nIt is under Profiles, Your profiles: Enter runs it, Space queues it. To share it: helixforge profile export \"{}\".",
                        profile.name,
                        profile.entries.len(),
                        path.display(),
                        profile.name
                    ),
                    true,
                )
            }
            Err(error) => TextPopup::new("Profile not saved", error, true),
        }));
    }

    /// Run the health checks in the background.
    fn examine(&mut self) {
        if !self.examine_here {
            return;
        }
        let manager = self.platform.manager;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(doctor::examine(&doctor::Here, manager));
        });
        self.examining = Some(receiver);
    }

    /// The Helix Doctor area's entries, shown below the checks.
    fn doctor_entries(&self) -> Vec<Entry> {
        self.catalog
            .tabs
            .iter()
            .find(|tab| tab.id == "doctor")
            .map(|tab| tab.entries().into_iter().cloned().collect())
            .unwrap_or_default()
    }

    fn selected_diagnosis(&self) -> Option<&doctor::Diagnosis> {
        self.main_state
            .selected()
            .and_then(|index| self.diagnoses.get(index))
    }

    fn describe_diagnosis(&mut self) {
        let Some(diagnosis) = self.selected_diagnosis() else {
            return;
        };
        let mut text = format!("{}: {}\n", diagnosis.health.label(), diagnosis.summary);
        if !diagnosis.evidence.is_empty() {
            text.push_str("\nWhat was found:\n");
            for line in &diagnosis.evidence {
                text.push_str(&format!("  {line}\n"));
            }
        }
        if let Some(advice) = &diagnosis.advice {
            text.push_str(&format!("\nWhat to do: {advice}\n"));
        }
        if let Some(repair) = &diagnosis.repair {
            text.push_str(&format!(
                "\nRepair: {}. f runs it after its own confirmation.\n",
                repair.label
            ));
        } else if let Some(entry) = diagnosis
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
        {
            text.push_str(&format!(
                "\nThe {} entry addresses it; f runs it.\n",
                entry.name
            ));
        }
        self.popup = Some(Popup::Text(TextPopup::new(
            diagnosis.title.clone(),
            text,
            true,
        )));
    }

    /// Run the selected check's repair, or the entry that addresses it.
    fn repair_selected(&mut self) {
        let Some(diagnosis) = self.selected_diagnosis().cloned() else {
            return;
        };
        if let Some(repair) = &diagnosis.repair {
            self.confirm_plan(Ok(repair.plan(&diagnosis.id)));
        } else if let Some(entry) = diagnosis
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
            .cloned()
        {
            self.confirm(vec![entry]);
        }
    }

    fn draw_doctor(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Helix Doctor",
            "The checks only read the system; a repair runs after its own confirmation.",
        );
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.diagnoses.is_empty() {
            items.push(Line::styled(
                if self.examining.is_some() {
                    "Checking…"
                } else {
                    "Press r to check this machine."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        // The title column fits the longest title.
        let title_width = self
            .diagnoses
            .iter()
            .map(|diagnosis| diagnosis.title.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        for diagnosis in &self.diagnoses {
            let color = match diagnosis.health {
                doctor::Health::Good => self.theme.success(),
                doctor::Health::Notice => self.theme.warning(),
                doctor::Health::Problem => self.theme.failure(),
            };
            let offer = if diagnosis.repair.is_some() {
                "  · f repairs"
            } else if diagnosis.entry.is_some() {
                "  · f runs the entry"
            } else {
                ""
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", diagnosis.health.label()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    format!("{:<title_width$}", diagnosis.title),
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    diagnosis.summary.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(self.theme.focused())),
            ]));
        }
        for entry in self.doctor_entries() {
            items.push(Line::from(vec![
                Span::styled("Report  ", Style::default().fg(self.theme.group())),
                Span::styled(entry.name.clone(), Style::default().fg(self.theme.entry())),
            ]));
        }
        self.fill(frame, inner);
        let list = List::new(items)
            .block(
                self.card(if self.examining.is_some() && !self.diagnoses.is_empty() {
                    "Checks (checking again…)"
                } else {
                    "Checks"
                }),
            )
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    /// Look at the update channels in the background; `online` also asks the
    /// network, through read-only commands.
    fn gather_updates(&mut self, online: bool) {
        if !self.examine_here {
            return;
        }
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let channels =
                updates::overview(&doctor::Here, &platform, &crate::this_places(), online);
            let _ = sender.send((channels, online));
        });
        self.gathering = Some(receiver);
    }

    fn selected_channel(&self) -> Option<&updates::Channel> {
        self.main_state
            .selected()
            .and_then(|index| self.channels.get(index))
    }

    fn describe_channel(&mut self) {
        let Some(channel) = self.selected_channel() else {
            return;
        };
        let mut text = format!(
            "{}: {}\n{}\n",
            channel.name,
            channel.summary(),
            channel.note
        );
        if let Some(pending) = &channel.pending
            && !pending.is_empty()
        {
            text.push('\n');
            for item in pending {
                let versions = match (&item.from, &item.to) {
                    (Some(from), Some(to)) => format!("{from} → {to}"),
                    (None, Some(to)) => to.clone(),
                    _ => String::new(),
                };
                text.push_str(&format!(
                    "  {}  {}\n",
                    helixforge_core::printable(&item.name),
                    helixforge_core::printable(&versions)
                ));
            }
        }
        if channel.commands.is_empty() {
            text.push_str("\nThere is nothing to run here.\n");
        } else {
            text.push_str(&format!("\nu runs: {}\n", channel.command_line()));
        }
        if let Some(caveat) = &channel.caveat {
            text.push_str(&format!("Note: {caveat}\n"));
        }
        self.popup = Some(Popup::Text(TextPopup::new("Updates", text, true)));
    }

    /// Update every channel with updates known to wait, one after another.
    fn update_every_channel(&mut self) {
        let chosen: Vec<&updates::Channel> = self
            .channels
            .iter()
            .filter(|channel| channel.count().is_some_and(|count| count > 0))
            .filter(|channel| !channel.commands.is_empty())
            .collect();
        if chosen.is_empty() {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to update",
                "No channel is known to have updates waiting. r checks online; u updates the selected channel anyway, which is how Nix channels are updated, since they count nothing beforehand.",
                true,
            )));
            return;
        }
        let planned = updates::plan(&chosen);
        self.confirm_plan(Ok(planned));
    }

    fn draw_updates(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Updates",
            "What each tool has waiting; updating runs that tool, after confirmation.",
        );
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.channels.is_empty() {
            items.push(Line::styled(
                if self.gathering.is_some() {
                    "Looking…"
                } else {
                    "No package manager, Flatpak or Nix setup HelixForge knows was found. r looks again, online."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        let name_width = self
            .channels
            .iter()
            .map(|channel| channel.name.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        for channel in &self.channels {
            let color = match channel.count() {
                Some(0) => self.theme.success(),
                Some(_) => self.theme.warning(),
                None => self.theme.muted(),
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<name_width$}", channel.name),
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    format!("{:<14}", channel.summary()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    channel.note.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
            ]));
        }
        let title = match (self.gathering.is_some(), self.channels_online) {
            (true, _) if !self.channels.is_empty() => "Channels (looking again…)",
            (_, true) => "Channels · checked online",
            _ => "Channels",
        };
        self.fill(frame, inner);
        let list = List::new(items)
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    // ----- the command palette -----

    /// Every screen, action and entry, with what each does.
    fn palette_choices(&self) -> (Vec<Choice>, Vec<Act>) {
        let mut choices = Vec::new();
        let mut acts = Vec::new();
        let mut add = |label: String, detail: &str, keywords: &str, listed: bool, act: Act| {
            choices.push(Choice {
                label,
                detail: detail.to_string(),
                keywords: keywords.to_string(),
                listed,
            });
            acts.push(act);
        };
        for item in &self.menu {
            add(
                format!("Open {}", item.label),
                "screen",
                "go to show",
                true,
                Act::Open(item.screen),
            );
        }
        let actions: [(&str, &str, Act); 14] = [
            (
                "Check updates online",
                "updates refresh",
                Act::UpdatesOnline,
            ),
            (
                "Run the health checks",
                "doctor diagnostics",
                Act::Doctor(None),
            ),
            (
                "Run DNS diagnostics",
                "doctor dns name resolution",
                Act::Doctor(Some("dns")),
            ),
            (
                "Check the network",
                "doctor internet connection route",
                Act::Doctor(Some("network")),
            ),
            (
                "Check disk space",
                "doctor free storage",
                Act::Doctor(Some("disk-space")),
            ),
            (
                "Check failed services",
                "doctor systemd units",
                Act::Doctor(Some("services")),
            ),
            (
                "Save this machine as a baseline",
                "drift snapshot record",
                Act::SaveBaseline,
            ),
            ("Run the Install Plan", "queue", Act::RunPlan),
            ("Clear the Install Plan", "queue empty", Act::ClearPlan),
            (
                "Save the Install Plan as a profile",
                "queue",
                Act::SaveProfile,
            ),
            (
                "Update every entry with an update",
                "upgrade packages",
                Act::UpdateEntries,
            ),
            ("Change the theme", "colors colors look", Act::Theme),
            (
                "Set up this machine",
                "wizard first run workstation",
                Act::Wizard,
            ),
            (
                if self.icons {
                    "Turn icons off"
                } else {
                    "Turn icons on"
                },
                "nerd font plain",
                Act::Icons,
            ),
        ];
        for (label, keywords, act) in actions {
            add(label.to_string(), "action", keywords, true, act);
        }
        add(
            "Show the keys".to_string(),
            "action",
            "help",
            true,
            Act::Help,
        );
        add(
            "Quit HelixForge".to_string(),
            "action",
            "exit close",
            true,
            Act::Quit,
        );
        for tab in &self.catalog.tabs {
            for entry in tab.entries() {
                let verb = match (&entry.action, self.statuses.get(&entry.id)) {
                    (_, Some(Status::Absent { .. })) => "Install",
                    (Action::Package { .. }, None) => "Install",
                    (Action::Profile { .. }, _) => "Run profile",
                    _ => "Run",
                };
                add(
                    format!("{verb} {}", entry.name),
                    &tab.name,
                    &entry.id,
                    false,
                    Act::Entry(entry.id.clone()),
                );
                add(
                    format!("Inspect {}", entry.name),
                    "trust and testing",
                    &entry.id,
                    false,
                    Act::Inspect(entry.id.clone()),
                );
            }
        }
        (choices, acts)
    }

    fn open_palette(&mut self) {
        let (choices, acts) = self.palette_choices();
        self.palette_acts = acts;
        self.popup = Some(Popup::Palette(Palette::new(choices)));
    }

    /// Do what a palette choice says; false quits.
    fn act(&mut self, act: Act) -> bool {
        match act {
            Act::Open(screen) => {
                self.open_screen(screen);
                if screen != Screen::Dashboard {
                    self.focus = Focus::Main;
                }
            }
            Act::Entry(id) => {
                if let Some(entry) = self.catalog.find(&id).cloned() {
                    self.confirm(vec![entry]);
                }
            }
            Act::Inspect(id) => self.inspect(&id),
            Act::UpdatesOnline => {
                self.open_screen(Screen::Updates);
                self.focus = Focus::Main;
                self.gather_updates(true);
            }
            Act::Doctor(wanted) => {
                self.open_screen(Screen::Doctor);
                self.focus = Focus::Main;
                if let Some(wanted) = wanted {
                    if self.diagnoses.is_empty() || self.examining.is_some() {
                        self.doctor_wanted = Some(wanted);
                    } else {
                        self.show_check(wanted);
                    }
                } else if !self.diagnoses.is_empty() {
                    self.examine();
                }
            }
            Act::SaveBaseline => {
                self.open_screen(Screen::Drift);
                self.focus = Focus::Main;
                self.popup = Some(Popup::BaselineName(InputPopup::new(
                    "Save this machine as a baseline",
                    "A name for the baseline (it is saved in ~/.config/helixforge/baselines):",
                )));
            }
            Act::Wizard => self.open_wizard(false),
            Act::RunPlan => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
                self.confirm(self.queue.clone());
            }
            Act::ClearPlan => {
                self.queue.clear();
                self.clamp_main();
            }
            Act::SaveProfile => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
                if !self.queue.is_empty() {
                    self.popup = Some(Popup::ProfileName(InputPopup::new(
                        "Save as a profile",
                        "A name for this profile (it is saved in ~/.config/helixforge/profiles):",
                    )));
                }
            }
            Act::UpdateEntries => {
                let waiting: Vec<Entry> = self
                    .catalog
                    .entries()
                    .into_iter()
                    .filter(|entry| {
                        matches!(self.statuses.get(&entry.id), Some(Status::Update { .. }))
                    })
                    .cloned()
                    .collect();
                if waiting.is_empty() {
                    self.popup = Some(Popup::Text(TextPopup::new(
                        "Nothing to update",
                        "No package entry has an update in the repository metadata on this machine.",
                        true,
                    )));
                } else {
                    self.update(waiting);
                }
            }
            Act::Theme => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            Act::Icons => {
                self.icons = !self.icons;
                self.save_look();
            }
            Act::Help => {
                self.popup = Some(Popup::Text(TextPopup::new("Keys", HELP, false)));
            }
            Act::Quit => return false,
        }
        true
    }

    /// Select a health check by id (or the first whose id starts with it)
    /// and show its details.
    fn show_check(&mut self, wanted: &str) {
        let found = self
            .diagnoses
            .iter()
            .position(|diagnosis| diagnosis.id == wanted)
            .or_else(|| {
                self.diagnoses
                    .iter()
                    .position(|diagnosis| diagnosis.id.starts_with(wanted))
            });
        if let Some(index) = found {
            self.main_state.select(Some(index));
            self.describe_diagnosis();
        }
    }

    // ----- Drift -----

    fn drift_dir(&self) -> Option<std::path::PathBuf> {
        self.baselines_dir.clone().or_else(drift::dir)
    }

    /// Read the baselines and profiles again, keep the chosen one, compare.
    fn reload_drift(&mut self) {
        let chosen = self
            .drift_targets
            .get(self.drift_target)
            .map(DriftTarget::key);
        let mut targets: Vec<DriftTarget> = self
            .drift_dir()
            .map(|directory| drift::load_all(&directory).0)
            .unwrap_or_default()
            .into_iter()
            .map(|(slug, baseline)| DriftTarget::Baseline { slug, baseline })
            .collect();
        if let Some(tab) = self.catalog.tabs.iter().find(|tab| tab.id == "profiles") {
            for entry in tab.entries() {
                if let Action::Profile { entries } = &entry.action {
                    targets.push(DriftTarget::Profile {
                        id: entry.id.clone(),
                        name: entry.name.clone(),
                        entries: entries.clone(),
                    });
                }
            }
        }
        self.drift_target = chosen
            .and_then(|key| targets.iter().position(|target| target.key() == key))
            .unwrap_or(0);
        self.drift_targets = targets;
        self.check_drift();
    }

    /// Compare with the chosen target, from the last look at what is
    /// installed; nothing is compared before that look ends.
    fn check_drift(&mut self) {
        let Some(target) = self.drift_targets.get(self.drift_target).cloned() else {
            self.drift_report = None;
            return;
        };
        let Some(inventory) = &self.inventory else {
            self.drift_report = None;
            return;
        };
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let (records, settings) = if self.examine_here {
            (
                history::read(1000).unwrap_or_default(),
                drift::settings_here(&doctor::Here),
            )
        } else {
            (Vec::new(), Default::default())
        };
        let settings = move || settings.clone();
        let scripts = self.catalog.root().join("scripts");
        let examine_here = self.examine_here;
        let render = move |arguments: &[String]| {
            if examine_here {
                helixforge_engine::preview::render_with(&scripts, arguments)
            } else {
                Err("not this machine".to_string())
            }
        };
        let sources = drift::Sources {
            catalog: &self.catalog,
            platform: &self.platform,
            inventory,
            on_path: if self.examine_here { &which } else { &|_| None },
            system: &doctor::Here,
            home: &home,
            history: &records,
            settings: &settings,
            render: &render,
        };
        let report = match &target {
            DriftTarget::Baseline { baseline, .. } => drift::compare(baseline, &sources),
            DriftTarget::Profile { name, entries, .. } => {
                drift::compare_profile(name, entries, &sources)
            }
        };
        self.drift_report = Some((target.key(), report));
        if self.screen == Screen::Drift {
            self.clamp_main();
        }
    }

    fn next_drift_target(&mut self, step: isize) {
        if self.drift_targets.len() < 2 {
            return;
        }
        let count = self.drift_targets.len() as isize;
        self.drift_target = (self.drift_target as isize + step).rem_euclid(count) as usize;
        self.main_state.select(Some(0));
        self.check_drift();
    }

    fn selected_difference(&self) -> Option<&drift::Difference> {
        let (_, report) = self.drift_report.as_ref()?;
        report.differences.get(self.main_state.selected()?)
    }

    fn describe_difference(&mut self) {
        let Some(difference) = self.selected_difference() else {
            return;
        };
        let mut text = format!(
            "{}: {}\n{}\n\n{}\n",
            difference.kind.label(),
            difference.subject,
            difference.note,
            difference.kind.explain()
        );
        if let Some(was) = &difference.was {
            text.push_str(&format!("\nIn the baseline: {was}"));
        }
        if let Some(now) = &difference.now {
            text.push_str(&format!("\nHere: {now}"));
        }
        if let Some(entry) = difference
            .entry
            .as_deref()
            .and_then(|id| self.catalog.find(id))
            && restorable(difference.kind)
        {
            text.push_str(&format!(
                "\n\nThe {} entry puts it back: Space adds it to the Install Plan, where it is planned and confirmed before anything runs.",
                entry.name
            ));
            if matches!(
                difference.kind,
                drift::Kind::FileChanged | drift::Kind::FileReplaced
            ) {
                text.push_str(" Running it replaces what is there now, keeping a backup.");
            }
        }
        self.popup = Some(Popup::Text(TextPopup::new("Drift", text, true)));
    }

    /// Queue every entry that puts back something missing, and show the
    /// Install Plan to review them.
    fn queue_missing(&mut self) {
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        let (restore, unavailable) =
            crate::drift::available(&report.restore(), &self.catalog, &self.platform);
        let mut added = Vec::new();
        for id in &restore {
            if let Some(entry) = self.catalog.find(id).cloned()
                && entry.multi_select
                && !self.is_queued(&entry)
            {
                added.push(entry.name.clone());
                self.queue.push(entry);
            }
        }
        let alone: Vec<String> = restore
            .iter()
            .filter_map(|id| self.catalog.find(id))
            .filter(|entry| !entry.multi_select)
            .map(|entry| entry.name.clone())
            .collect();
        let mut text = if added.is_empty() {
            "Nothing new was added to the Install Plan.".to_string()
        } else {
            format!(
                "Added to the Install Plan: {}.\n\nNothing has run: review the plan there and press Enter to run it.",
                added.join(", ")
            )
        };
        if !alone.is_empty() {
            text.push_str(&format!(
                "\n\nThese run on their own, so run each from its area: {}.",
                alone.join(", ")
            ));
        }
        if !unavailable.is_empty() {
            text.push_str(&format!(
                "\n\nNot available on {}: {}.",
                self.platform.pretty_name,
                unavailable.join(", ")
            ));
        }
        if !added.is_empty() {
            self.open_screen(Screen::Plan);
            self.focus = Focus::Main;
        }
        self.popup = Some(Popup::Text(TextPopup::new("Install Plan", text, true)));
    }

    /// Record this machine as a baseline and compare with it.
    fn save_baseline(&mut self, name: &str) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "HelixForge is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        let (records, settings) = if self.examine_here {
            (
                history::read(1000).unwrap_or_default(),
                drift::settings_here(&doctor::Here),
            )
        } else {
            (Vec::new(), Default::default())
        };
        let settings = move || settings.clone();
        let scripts = self.catalog.root().join("scripts");
        let examine_here = self.examine_here;
        let render = move |arguments: &[String]| {
            if examine_here {
                helixforge_engine::preview::render_with(&scripts, arguments)
            } else {
                Err("not this machine".to_string())
            }
        };
        let sources = drift::Sources {
            catalog: &self.catalog,
            platform: &self.platform,
            inventory,
            on_path: if self.examine_here { &which } else { &|_| None },
            system: &doctor::Here,
            home: &home,
            history: &records,
            settings: &settings,
            render: &render,
        };
        let baseline = drift::capture(name, history::now(), &sources, &[]);
        let saved = self
            .drift_dir()
            .ok_or_else(|| "cannot find the configuration folder".to_string())
            .and_then(|directory| {
                drift::save(&directory, &baseline).map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok(path) => {
                let slug = path
                    .file_stem()
                    .map(|stem| stem.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.reload_drift();
                if let Some(position) = self.drift_targets.iter().position(|target| {
                    matches!(target, DriftTarget::Baseline { slug: known, .. } if *known == slug)
                }) {
                    self.drift_target = position;
                    self.check_drift();
                }
                TextPopup::new(
                    "Baseline saved",
                    format!(
                        "{} is saved as {}: {} entries, {} configuration paths, {} settings and {} runs.\n\nDrift compares with it from now on; b chooses another. To compare another machine with it: helixforge drift export \"{}\", then helixforge drift check <file> there.",
                        baseline.name,
                        path.display(),
                        baseline.entries.len(),
                        baseline.files.len(),
                        baseline.settings.len(),
                        baseline.applied.len(),
                        baseline.name
                    ),
                    true,
                )
            }
            Err(error) => TextPopup::new("Baseline not saved", error, true),
        }));
    }

    fn draw_drift(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Drift",
            "How this machine differs from a saved baseline or a profile. It only reads.",
        );
        let target = self.drift_targets.get(self.drift_target);
        let mut lines: Vec<Line> = Vec::new();
        match target {
            None => lines.push(Line::from(
                "No baseline saved yet, and no profile to compare with.",
            )),
            Some(DriftTarget::Baseline { baseline, .. }) => lines.push(Line::from(vec![
                Span::styled("Baseline  ", Style::default().fg(self.theme.muted())),
                Span::styled(
                    helixforge_core::printable(&baseline.name),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Span::styled(
                    format!(
                        " · saved {} on {}",
                        timestamp(baseline.taken),
                        helixforge_core::printable(&baseline.system)
                    ),
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
            Some(DriftTarget::Profile { name, .. }) => lines.push(Line::from(vec![
                Span::styled("Profile  ", Style::default().fg(self.theme.muted())),
                Span::styled(name.clone(), Style::default().fg(self.theme.entry()).bold()),
                Span::styled(
                    " · is each of its entries here?",
                    Style::default().fg(self.theme.muted()),
                ),
            ])),
        }
        let summary = match &self.drift_report {
            _ if target.is_none() => None,
            None => Some(Line::styled(
                "Checking what is installed…",
                Style::default().fg(self.theme.muted()),
            )),
            Some((_, report)) if report.differences.is_empty() => Some(Line::styled(
                format!(
                    "No differences: {} things are as they were.",
                    report.matched
                ),
                Style::default().fg(self.theme.success()),
            )),
            Some((_, report)) => {
                let counts: Vec<String> = report
                    .counts()
                    .into_iter()
                    .map(|(label, count)| format!("{count} {}", label.to_lowercase()))
                    .collect();
                Some(Line::styled(
                    format!("{} · {} the same", counts.join(" · "), report.matched),
                    Style::default().fg(self.theme.warning()),
                ))
            }
        };
        lines.extend(summary);
        lines.push(Line::styled(
            match self.drift_targets.len() {
                0 | 1 => "s saves this machine as a baseline".to_string(),
                count => format!(
                    "b: another baseline or profile ({} of {count}) · s: save this machine",
                    self.drift_target + 1
                ),
            },
            Style::default().fg(self.theme.faint()),
        ));
        let parts = Layout::vertical([
            Constraint::Length(lines.len() as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Compared with"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        self.fill(frame, parts[2]);
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        if report.differences.is_empty() {
            let mut text = "Everything recorded is as it was.".to_string();
            if !report.unknown.is_empty() {
                text.push_str(&format!(
                    " Not in this catalog, so not compared: {}.",
                    report.unknown.join(", ")
                ));
            }
            frame.render_widget(
                Paragraph::new(text)
                    .block(self.card("Differences"))
                    .style(Style::default().fg(self.theme.muted()))
                    .wrap(Wrap { trim: true }),
                parts[2],
            );
            return;
        }
        let subject_width = report
            .differences
            .iter()
            .map(|difference| difference.subject.chars().count())
            .max()
            .unwrap_or(0)
            .min(36)
            + 2;
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = report
            .differences
            .iter()
            .map(|difference| {
                let color = match difference.kind {
                    kind if kind.missing() => self.theme.failure(),
                    drift::Kind::Added | drift::Kind::Version => self.theme.muted(),
                    _ => self.theme.warning(),
                };
                let queued = difference
                    .entry
                    .as_deref()
                    .is_some_and(|id| self.queue.iter().any(|entry| entry.id == id))
                    && restorable(difference.kind);
                let subject: String = difference.subject.chars().take(36).collect();
                Line::from(vec![
                    Span::styled(
                        format!("{:<10}", difference.kind.label()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::styled(
                        format!("{subject:<subject_width$}"),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        difference.note.clone(),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(
                        if queued { "  · planned" } else { "" },
                        Style::default().fg(self.theme.success()),
                    ),
                ])
            })
            .collect();
        let list = List::new(items)
            .block(self.card("Differences"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        frame.render_stateful_widget(list, parts[2], &mut self.main_state);
    }

    /// The change preview of a plan.
    fn show_effects(&mut self, planned: &Plan, from_confirmation: bool) {
        let scripts = self.catalog.root().join("scripts");
        let render =
            |arguments: &[String]| helixforge_engine::preview::render_with(&scripts, arguments);
        let look = helixforge_engine::preview::Look {
            system: &doctor::Here,
            render: &render,
            pinned: &|name| helixforge_engine::preview::pinned_source(&scripts, name),
        };
        let steps = helixforge_engine::preview::preview(
            planned,
            &|id| self.catalog.find(id),
            &self.platform,
            &look,
        );
        let download = helixforge_engine::preview::sources_for(planned)
            .into_iter()
            .any(|name| {
                matches!(
                    helixforge_engine::preview::pinned_source(&scripts, name),
                    helixforge_engine::preview::Pinned::Missing { .. }
                )
            });
        self.popup = Some(Popup::Effects(EffectsPopup::new(
            steps,
            from_confirmation,
            planned.clone(),
            download,
        )));
    }

    /// Download the pinned sources a plan reads, which changes nothing
    /// else, then show its changes again.
    fn download_sources(&mut self, planned: &Plan, from_confirmation: bool) {
        let scripts = self.catalog.root().join("scripts");
        for name in helixforge_engine::preview::sources_for(planned) {
            if let Err(why) = helixforge_engine::preview::fetch_source(&scripts, name) {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot download",
                    format!("{name}: {why}"),
                    true,
                )));
                return;
            }
        }
        self.show_effects(planned, from_confirmation);
    }

    // ----- The setup wizard -----

    /// Open the wizard; on the first run it says welcome.
    pub fn open_wizard(&mut self, first_run: bool) {
        let setup = match helixforge_engine::setup::load(self.catalog.root()) {
            Ok(setup) => setup,
            // A catalog without workstations (a test's) has nothing to offer at the start.
            Err(_) if first_run => return,
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new("No setup wizard", error, true)));
                return;
            }
        };
        let desktop = self.desktop_here();
        let survey = if self.examine_here {
            crate::setup::survey_here(&self.platform, &desktop)
        } else {
            helixforge_engine::setup::survey(&self.platform, &desktop, Vec::new(), false, &|_| {
                false
            })
        };
        self.wizard_ids = setup
            .workstations
            .iter()
            .map(|known| known.id.clone())
            .collect();
        let workstations = setup
            .workstations
            .iter()
            .map(|known| (known.name.clone(), known.description.clone()))
            .collect();
        self.popup = Some(Popup::Wizard(Box::new(Wizard::new(
            crate::setup::survey_lines(&survey),
            workstations,
            first_run,
        ))));
    }

    /// Whether this is the first run, when the wizard opens by itself.
    pub fn first_run(&self) -> bool {
        !self.set_up
    }

    fn close_wizard(&mut self) {
        self.popup = None;
        if !self.set_up {
            self.set_up = true;
            self.save_look();
        }
    }

    /// The chosen workstation's entries for this machine, ticked where they
    /// can run and are not installed.
    fn wizard_picks(&mut self, index: usize) -> Vec<Pick> {
        let Ok(setup) = helixforge_engine::setup::load(self.catalog.root()) else {
            return Vec::new();
        };
        let desktop = self.desktop_here();
        let survey = if self.examine_here {
            crate::setup::survey_here(&self.platform, &desktop)
        } else {
            helixforge_engine::setup::survey(&self.platform, &desktop, Vec::new(), false, &|_| {
                false
            })
        };
        let Some(chosen) = self
            .wizard_ids
            .get(index)
            .and_then(|id| setup.workstations.iter().find(|known| known.id == *id))
        else {
            return Vec::new();
        };
        let custom = chosen.entries.is_empty() && chosen.when.is_empty();
        let ids = if custom {
            setup.everything(&survey)
        } else {
            chosen.entries_for(&survey)
        };
        helixforge_engine::setup::suggestions(&ids, &self.catalog, &self.platform)
            .into_iter()
            .map(|suggestion| {
                let status = self.statuses.get(&suggestion.id);
                let installed = matches!(
                    status,
                    Some(
                        Status::Installed { .. }
                            | Status::Update { .. }
                            | Status::Managed { .. }
                            | Status::Elsewhere { .. }
                    )
                );
                let note = if installed {
                    Some(format!(
                        "installed ({})",
                        status.map(Status::detail).unwrap_or_default()
                    ))
                } else {
                    suggestion.note.clone()
                };
                Pick {
                    ticked: suggestion.available && !installed && !custom,
                    id: suggestion.id,
                    name: suggestion.name,
                    available: suggestion.available,
                    note,
                }
            })
            .collect()
    }

    /// Put the wizard's choices in the Install Plan and show it.
    fn queue_from_wizard(&mut self, ids: &[String]) {
        let mut added = Vec::new();
        let mut alone = Vec::new();
        for id in ids {
            let Some(entry) = self.catalog.find(id).cloned() else {
                continue;
            };
            if !entry.multi_select {
                alone.push(entry.name.clone());
            } else if !self.is_queued(&entry) {
                added.push(entry.name.clone());
                self.queue.push(entry);
            }
        }
        let mut text = if added.is_empty() {
            "Nothing was added to the Install Plan.".to_string()
        } else {
            format!(
                "Added to the Install Plan: {}.\n\nNothing has run. Review the plan, e shows what it would change, and Enter runs it.",
                added.join(", ")
            )
        };
        if !alone.is_empty() {
            text.push_str(&format!(
                "\n\nThese run on their own; run each from its area: {}.",
                alone.join(", ")
            ));
        }
        self.open_screen(Screen::Plan);
        self.focus = Focus::Main;
        self.popup = Some(Popup::Text(TextPopup::new("Install Plan", text, true)));
    }

    // ----- Desktop -----

    fn desktop_here(&self) -> helixforge_engine::desktop::Desktop {
        if self.examine_here {
            crate::setup::desktop_here()
        } else {
            self.desktop.clone().unwrap_or_default()
        }
    }

    fn selected_desktop_row(&self) -> Option<&DesktopRow> {
        self.desktop_rows.get(self.main_state.selected()?)
    }

    fn looks_directory(&self) -> Option<std::path::PathBuf> {
        self.looks_dir
            .clone()
            .or_else(helixforge_engine::desktop::dir)
    }

    fn reload_desktop(&mut self) {
        let found = self.desktop_here();
        let mut rows = vec![DesktopRow::Heading("This desktop".to_string())];
        let name = match (&found.name, &found.session) {
            (Some(name), Some(session)) => format!("{name}, {session} session"),
            (Some(name), None) => name.clone(),
            (None, _) => "No desktop environment found".to_string(),
        };
        rows.push(DesktopRow::Info {
            details: crate::setup::desktop_text(&found),
            text: name,
        });
        if let Some(manager) = &found.display_manager {
            rows.push(DesktopRow::Info {
                text: format!(
                    "Login screen: {}{}",
                    manager.name,
                    manager
                        .theme
                        .as_deref()
                        .map(|theme| format!(", theme {theme}"))
                        .unwrap_or_default()
                ),
                details: format!(
                    "The display manager {} runs the login screen.\n\nIts configuration:\n{}",
                    manager.name,
                    if manager.files.is_empty() {
                        "  none found".to_string()
                    } else {
                        manager
                            .files
                            .iter()
                            .map(|file| format!("  {file}"))
                            .collect::<Vec<_>>()
                            .join("\n")
                    }
                ),
            });
        }
        if !found.appearance.is_empty() {
            rows.push(DesktopRow::Heading("Appearance".to_string()));
        }
        for value in &found.appearance {
            rows.push(DesktopRow::Info {
                text: format!(
                    "{:<16}{}",
                    value.label,
                    value.value.as_deref().unwrap_or("not set")
                ),
                details: format!(
                    "{}: {}\n\ns saves the whole look, to put back later from this screen.",
                    value.label,
                    value.value.as_deref().unwrap_or("not set")
                ),
            });
        }
        for owner in &found.owners {
            rows.push(DesktopRow::Info {
                text: format!("Note: {owner}"),
                details: owner.clone(),
            });
        }
        if let Some(blocked) = &found.blocked {
            rows.push(DesktopRow::Info {
                text: format!("Not changed here: {blocked}"),
                details: blocked.clone(),
            });
        }
        let looks = self
            .looks_directory()
            .map(|directory| helixforge_engine::desktop::load_all(&directory).0)
            .unwrap_or_default();
        if !looks.is_empty() {
            rows.push(DesktopRow::Heading("Saved looks".to_string()));
        }
        for (slug, look) in looks {
            let other = found.kind.map(|kind| kind.id()) != Some(look.desktop.as_str());
            rows.push(DesktopRow::Look {
                text: format!(
                    "{}{}",
                    helixforge_core::printable(&look.name),
                    if other {
                        format!("  (saved on {})", helixforge_core::printable(&look.desktop))
                    } else {
                        format!("  ({} settings)", look.settings.len())
                    }
                ),
                slug,
            });
        }
        let offered = helixforge_engine::setup::load(self.catalog.root())
            .map(|setup| setup.for_desktop(found.kind))
            .unwrap_or_default();
        let offered: Vec<_> =
            helixforge_engine::setup::suggestions(&offered, &self.catalog, &self.platform)
                .into_iter()
                .filter(|suggestion| suggestion.available)
                .collect();
        if !offered.is_empty() {
            rows.push(DesktopRow::Heading("For this desktop".to_string()));
        }
        for suggestion in offered {
            let queued = self.queue.iter().any(|entry| entry.id == suggestion.id);
            rows.push(DesktopRow::Entry {
                text: format!(
                    "{}{}",
                    suggestion.name,
                    if queued { "  · planned" } else { "" }
                ),
                id: suggestion.id,
            });
        }
        self.desktop = Some(found);
        self.desktop_rows = rows;
        self.clamp_main();
    }

    fn save_look_as(&mut self, name: &str) {
        let found = self.desktop_here();
        let saved = helixforge_engine::desktop::capture(name, &found)
            .ok_or_else(|| {
                "no desktop whose appearance HelixForge can read is running here".to_string()
            })
            .and_then(|look| {
                let directory = self
                    .looks_directory()
                    .ok_or_else(|| "cannot find the configuration folder".to_string())?;
                helixforge_engine::desktop::save(&directory, &look)
                    .map(|path| (look, path))
                    .map_err(|error| error.to_string())
            });
        self.popup = Some(Popup::Text(match saved {
            Ok((look, path)) => TextPopup::new(
                "Look saved",
                format!(
                    "{} ({} settings) is saved as {}. Enter on it under Saved looks puts it back.",
                    look.name,
                    look.settings.len(),
                    path.display()
                ),
                true,
            ),
            Err(error) => TextPopup::new("Look not saved", error, true),
        }));
        self.reload_desktop();
    }

    fn apply_look(&mut self, slug: &str) {
        let Some(directory) = self.looks_directory() else {
            return;
        };
        let Some((_, look)) = helixforge_engine::desktop::load_all(&directory)
            .0
            .into_iter()
            .find(|(known, _)| known == slug)
        else {
            return;
        };
        let found = self.desktop_here();
        match helixforge_engine::desktop::plan_apply(&look, &found) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot apply this look",
                    why,
                    true,
                )));
            }
        }
    }

    fn draw_desktop(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Desktop",
            "The desktop, its login screen and look, and the entries for it; who else sets the look.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let items: Vec<Line> = self
            .desktop_rows
            .iter()
            .map(|row| match row {
                DesktopRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                DesktopRow::Info { text, .. } => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        text.clone(),
                        Style::default().fg(
                            if text.starts_with("Note:") || text.starts_with("Not changed") {
                                self.theme.warning()
                            } else {
                                self.theme.entry()
                            },
                        ),
                    ),
                ]),
                DesktopRow::Entry { text, .. } | DesktopRow::Look { text, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", self.glyph(Icon::Entry)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(self.theme.entry())),
                ]),
            })
            .collect();
        let list = List::new(items)
            .block(self.card("This machine"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    // ----- Hardware -----

    /// Read the hardware in the background; fwupd can take a moment.
    fn read_hardware(&mut self) {
        if !self.examine_here {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(helixforge_engine::hardware::gather(&doctor::Here));
        });
        self.reading_hardware = Some(receiver);
    }

    fn selected_hardware(&self) -> Option<&HardwareRow> {
        self.hardware_rows.get(self.main_state.selected()?)
    }

    fn draw_hardware(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Hardware",
            "Firmware, disks, sensors, batteries and devices, read-only; updates and SMART checks ask first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.hardware_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_hardware.is_some() {
                    "Reading the hardware…"
                } else {
                    "Press r to read this machine's hardware."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        for row in &self.hardware_rows {
            items.push(match row {
                HardwareRow::Heading(title) => Line::styled(
                    title.clone(),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                HardwareRow::Item {
                    text,
                    attention,
                    act,
                    ..
                } => Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        text.clone(),
                        Style::default().fg(if *attention {
                            self.theme.warning()
                        } else {
                            self.theme.entry()
                        }),
                    ),
                    Span::styled(
                        match act {
                            Some(HardwareAct::Firmware(_)) => "  · f updates it",
                            Some(HardwareAct::Smart(_)) => "  · s checks its health",
                            None => "",
                        },
                        Style::default().fg(self.theme.focused()),
                    ),
                ]),
            });
        }
        let title = if self.reading_hardware.is_some() && !self.hardware_rows.is_empty() {
            "This machine (reading again…)"
        } else {
            "This machine"
        };
        let list = List::new(items)
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        frame.render_stateful_widget(list, inner, &mut self.main_state);
    }

    // ----- Reliability and trust -----

    /// This build's testing record, read on first use.
    fn evidence(&mut self) -> reliability::Evidence {
        if self.evidence.is_none() {
            self.evidence = Some(reliability::Evidence::load(self.catalog.root()));
        }
        self.evidence.clone().unwrap_or_default()
    }

    fn run_history(&self) -> Vec<history::Record> {
        if self.examine_here {
            history::read(1000).unwrap_or_default()
        } else {
            self.records.clone()
        }
    }

    fn reload_reliability(&mut self) {
        let evidence = self.evidence();
        let records = self.run_history();
        self.reliability_rows = crate::inspect::rows(&self.catalog, &evidence, &records)
            .into_iter()
            .filter(|(_, assessment)| self.coverage.allows(assessment))
            .map(|(entry, assessment)| (entry.id.clone(), entry.name.clone(), assessment))
            .collect();
        self.clamp_main();
    }

    fn describe_reliability(&mut self) {
        let Some((id, name, assessment)) = self
            .main_state
            .selected()
            .and_then(|index| self.reliability_rows.get(index))
        else {
            return;
        };
        let mut text = format!(
            "{}{}\n\n{}\n",
            assessment.level.label(),
            assessment
                .verified
                .as_deref()
                .map(|date| format!(", last verified {date}"))
                .unwrap_or_default(),
            assessment.level.explain()
        );
        if assessment.critical {
            text.push_str("\nA critical entry: if it fails, the machine may not boot, show its login screen or display, or it changes disks or the whole system. Keep a snapshot or a live USB at hand.\n");
        }
        if !assessment.evidence.is_empty() {
            text.push_str("\nWhat shows it:\n");
            for line in &assessment.evidence {
                text.push_str(&format!("  {line}\n"));
            }
        }
        text.push_str(&format!(
            "\nv shows where {id} comes from and what checks it."
        ));
        self.popup = Some(Popup::Text(TextPopup::new(name.clone(), text, true)));
    }

    /// The trust inspector for an entry.
    fn inspect(&mut self, id: &str) {
        let Some(entry) = self.catalog.find(id).cloned() else {
            return;
        };
        let evidence = self.evidence();
        let records = self.run_history();
        let build = if self.examine_here {
            crate::inspect::build()
        } else {
            helixforge_engine::trust::Build {
                version: crate::VERSION.to_string(),
                release: false,
                commit: None,
                install: helixforge_engine::updates::Install::Other,
            }
        };
        let found = helixforge_engine::trust::inspect(
            &entry,
            self.catalog.root(),
            &self.platform,
            &build,
            &doctor::Here,
            &evidence,
            &records,
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Trust: {}", entry.name),
            crate::inspect::text(&found),
            true,
        )));
    }

    fn draw_reliability(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Reliability",
            "How far each entry has been tested; critical entries first. v shows where one comes from.",
        );
        let evidence = self.evidence();
        let counts: Vec<String> = {
            let records = self.run_history();
            let all = crate::inspect::rows(&self.catalog, &evidence, &records);
            crate::inspect::counts(&all)
                .into_iter()
                .map(|(level, count)| format!("{count} {}", level.label()))
                .collect()
        };
        let summary = evidence.summary();
        let counts = counts.join(" · ");
        // As tall as its text once wrapped to the card's width.
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let rows: usize = [summary.as_str(), counts.as_str()]
            .iter()
            .map(|text| text.chars().count().div_ceil(width).max(1))
            .sum();
        let lines = vec![
            Line::from(summary),
            Line::styled(counts, Style::default().fg(self.theme.muted())),
        ];
        let parts = Layout::vertical([
            Constraint::Length(rows as u16 + 2),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(inner);
        self.fill(frame, parts[0]);
        frame.render_widget(
            Paragraph::new(Text::from(lines))
                .block(self.card("Record"))
                .wrap(Wrap { trim: true }),
            parts[0],
        );
        let focused = self.focus == Focus::Main;
        let name_width = 34;
        let items: Vec<Line> = self
            .reliability_rows
            .iter()
            .map(|(_, name, assessment)| {
                let color = match assessment.level {
                    reliability::Level::NotYetValidated if assessment.critical => {
                        self.theme.failure()
                    }
                    reliability::Level::NotYetValidated => self.theme.muted(),
                    reliability::Level::StandInTested => self.theme.warning(),
                    _ => self.theme.success(),
                };
                let name: String = name.chars().take(name_width - 2).collect();
                let mut detail = assessment.systems.join(", ");
                if let Some(when) = assessment.here {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    detail.push_str(&format!("ran here {}", helixforge_engine::date(when)));
                }
                Line::from(vec![
                    Span::styled(
                        format!("{:<19}", assessment.level.label()),
                        Style::default().fg(color).bold(),
                    ),
                    Span::styled(
                        if assessment.critical { "! " } else { "  " },
                        Style::default().fg(self.theme.failure()).bold(),
                    ),
                    Span::styled(
                        format!("{name:<name_width$}"),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled(
                        format!("{:<12}", assessment.verified.as_deref().unwrap_or("")),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(detail, Style::default().fg(self.theme.faint())),
                ])
            })
            .collect();
        let title = match self.coverage {
            Coverage::All => "Entries".to_string(),
            coverage => format!("Entries · {}", coverage.label()),
        };
        self.fill(frame, parts[2]);
        let list = List::new(items)
            .block(
                self.card(&title).title(
                    Line::styled(
                        " ! CRITICAL · LAST VERIFIED ",
                        Style::default().fg(self.theme.muted()),
                    )
                    .right_aligned(),
                ),
            )
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        frame.render_stateful_widget(list, parts[2], &mut self.main_state);
    }

    fn reload_backups(&mut self) {
        self.backups = backups::discover(&backups::Roots::here());
        self.clamp_main();
    }

    fn selected_backup(&self) -> Option<&backups::Backup> {
        self.main_state
            .selected()
            .and_then(|index| self.backups.get(index))
    }

    fn describe_backup(&mut self) {
        let Some(backup) = self.selected_backup() else {
            return;
        };
        let text = format!(
            "Backup of: {}\nTaken: {}\nCopy: {}\nKind: {}, {}\n\nEnter restores it. The current {} is moved aside as its own backup first, so the restore can be undone the same way.",
            helixforge_core::printable(&backup.original.display().to_string()),
            backup.taken.as_deref().unwrap_or("unknown"),
            helixforge_core::printable(&backup.backup.display().to_string()),
            if backup.directory {
                "a folder"
            } else {
                "a file"
            },
            if backup.system {
                "a system file (restoring asks for your password)"
            } else {
                "your own file"
            },
            if backup.directory { "folder" } else { "file" },
        );
        self.popup = Some(Popup::Text(TextPopup::new("Backup", text, true)));
    }

    fn show_log(&mut self) {
        let Some(record) = self
            .main_state
            .selected()
            .and_then(|index| self.records.get(index))
        else {
            return;
        };
        let title = record.names.join(", ");
        let text = match &record.log {
            Some(path) => std::fs::read(path)
                .map(|bytes| strip_escapes(&String::from_utf8_lossy(&bytes)))
                .unwrap_or_else(|error| format!("The log cannot be read: {error}")),
            None => format!(
                "{}\n\nNo log was saved for this run. Press l in the running window after a run finishes to save one.",
                record.result
            ),
        };
        self.popup = Some(Popup::Text(TextPopup::new(title, text, false)));
    }

    fn change_setting(&mut self) {
        match self.main_state.selected().unwrap_or(0) {
            0 => {
                self.theme = self.theme.next();
                self.saved_theme = self.theme;
                self.save_look();
            }
            1 => {
                self.icons = !self.icons;
                self.save_look();
            }
            2 => self.skip_confirmation = !self.skip_confirmation,
            3 => {
                self.show_hidden = !self.show_hidden;
                match Catalog::builtin(!self.show_hidden) {
                    Ok(mut catalog) => {
                        if let Some(directory) = self.profiles_dir.clone().or_else(profiles::dir) {
                            profiles::add_to(&mut catalog, &directory);
                        }
                        self.catalog = catalog;
                        self.menu = build_menu(&self.catalog);
                        self.areas = vec![AreaState::default(); self.catalog.tabs.len()];
                        self.queue.clear();
                        self.update_rows();
                    }
                    Err(error) => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "Cannot reload the catalog",
                            error.to_string(),
                            true,
                        )));
                    }
                }
            }
            4 => {
                self.keep_going = !self.keep_going;
                self.save_look();
            }
            5 if self.snapshot_tool.is_some() => {
                self.snapshots = !self.snapshots;
                self.save_look();
            }
            6 => self.open_wizard(false),
            _ => {}
        }
    }

    /// Plan entries; ask before running what can run.
    fn confirm(&mut self, entries: Vec<Entry>) {
        if entries.is_empty() {
            return;
        }
        let planned = plan(entries.iter(), &self.platform, &|id| self.catalog.find(id));
        self.confirm_plan(planned);
    }

    /// Show a plan's confirmation, or why there is nothing to run.
    /// What the checks before a run find for a plan on this machine.
    fn findings(&self, planned: &Plan) -> Vec<preflight::Finding> {
        if !self.check_statuses {
            // Tests: no machine questions beyond what planning asked.
            return preflight::check(
                planned,
                &self.platform,
                &|id| self.catalog.find(id),
                &preflight::Facts {
                    present: &|_| true,
                    free_bytes: &|_| None,
                    gpus: &Vec::new,
                    repositories: &|| None,
                    sizes: &|_, _| None,
                    active_runs: &Vec::new,
                    ..preflight::Facts::real()
                },
            );
        }
        preflight::check(
            planned,
            &self.platform,
            &|id| self.catalog.find(id),
            &preflight::Facts::real(),
        )
    }

    /// Plan updating entries from the last status check, and ask.
    fn update(&mut self, entries: Vec<Entry>) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "HelixForge is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let planned = inventory.plan_update(entries.iter(), &self.platform, &which);
        self.confirm_plan(Ok(planned));
    }

    fn confirm_plan(&mut self, planned: Result<Plan, EngineError>) {
        let planned = match planned {
            Ok(planned) => planned,
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot plan",
                    error.to_string(),
                    true,
                )));
                return;
            }
        };
        if planned.is_empty() {
            let mut lines = planned.notes.clone();
            lines.extend(
                planned
                    .blocked
                    .iter()
                    .map(|blocked| format!("Not run: {blocked}")),
            );
            let text = if lines.is_empty() {
                "Nothing to run.".to_string()
            } else {
                lines.join("\n")
            };
            self.popup = Some(Popup::Text(TextPopup::new("Nothing to run", text, true)));
            return;
        }
        // A snapshot first when the run changes the system, where possible.
        let mut planned = planned;
        if self.snapshots
            && let Some(tool) = self.snapshot_tool
            && backups::worth_a_snapshot(&planned, &|id| {
                self.catalog.find(id).map(|entry| entry.flags.clone())
            })
        {
            backups::with_snapshot(&mut planned, tool);
        }
        let findings = self.findings(&planned);
        if preflight::blocked(&findings) {
            let lines: Vec<String> = findings.iter().map(preflight::Finding::line).collect();
            self.popup = Some(Popup::Text(TextPopup::new(
                "Cannot run yet",
                format!("{}\n\nNothing was changed.", lines.join("\n")),
                true,
            )));
            return;
        }
        let mut header = vec![format!(
            "Risk: {}{}",
            planned.risk.label(),
            if planned.privileged {
                " · asks for your password"
            } else {
                ""
            }
        )];
        header.extend(findings.iter().map(preflight::Finding::line));
        let evidence = self.evidence();
        header.extend(
            reliability::warnings(
                planned.steps.iter().map(|step| step.id()),
                &|id| self.catalog.find(id),
                &evidence,
            )
            .into_iter()
            .map(|warning| format!("Warning: {warning}")),
        );
        planned.keep_going = self.keep_going;
        if planned.steps.len() > 1 {
            header.push(format!(
                "After a failed step: {} (Settings changes it)",
                if self.keep_going {
                    "continue with what does not need it"
                } else {
                    "stop"
                }
            ));
        }
        let mut lines: Vec<(String, String)> = planned
            .steps
            .iter()
            .map(|step| (step.name().to_string(), step.display()))
            .collect();
        for note in &planned.notes {
            lines.push((note.clone(), String::new()));
        }
        for blocked in &planned.blocked {
            lines.push((format!("Not run: {blocked}"), String::new()));
        }
        self.pending = Some(planned);
        if self.skip_confirmation {
            self.run_pending();
            return;
        }
        self.popup = Some(Popup::Confirm(ConfirmPopup::new(&header, &lines)));
        self.confirming = Some((header, lines));
    }

    fn run_pending(&mut self) {
        let Some(planned) = self.pending.take() else {
            return;
        };
        let this =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("helixforge"));
        self.popup = Some(
            match RunningView::start(&planned, &self.catalog.lib_dir(), &this) {
                Ok(running) => {
                    // What ran from the plan screen leaves the queue when it
                    // finishes; what does not finish stays.
                    self.ran_from_plan = self.screen == Screen::Plan;
                    Popup::Running(Box::new(running))
                }
                Err(error) => Popup::Text(TextPopup::new("Cannot run", error, true)),
            },
        );
    }
}

/// The Hardware screen's lines, section by section.
fn hardware_rows(found: &helixforge_engine::hardware::Hardware) -> Vec<HardwareRow> {
    use helixforge_engine::hardware::size;
    let item = |text: String, attention: bool, details: String, act: Option<HardwareAct>| {
        HardwareRow::Item {
            text,
            attention,
            details,
            act,
        }
    };
    let mut rows = vec![HardwareRow::Heading("Firmware".to_string())];
    if let Some(note) = &found.firmware.note {
        rows.push(item(note.clone(), false, note.clone(), None));
    }
    for (index, update) in found.firmware.updates.iter().enumerate() {
        rows.push(item(
            format!(
                "Update for {}: {} → {}",
                update.device,
                update.from.as_deref().unwrap_or("?"),
                update.to
            ),
            true,
            format!(
                "{}\nNow: {}\nAvailable: {}\nUrgency: {}\n\n{}\n\nf updates it through fwupd after you type FIRMWARE and confirm the plan. Keep the machine on mains power; some updates finish at the next reboot.",
                update.device,
                update.from.as_deref().unwrap_or("unknown"),
                update.to,
                update.urgency.as_deref().unwrap_or("not given"),
                update.summary.as_deref().unwrap_or("No summary.")
            ),
            Some(HardwareAct::Firmware(index)),
        ));
    }
    if found.firmware.available {
        let updatable: Vec<&helixforge_engine::hardware::FirmwareDevice> = found
            .firmware
            .devices
            .iter()
            .filter(|device| device.updatable)
            .collect();
        let listing: Vec<String> = updatable
            .iter()
            .map(|device| {
                format!(
                    "{}{}",
                    device.name,
                    device
                        .version
                        .as_deref()
                        .map(|version| format!(" {version}"))
                        .unwrap_or_default()
                )
            })
            .collect();
        rows.push(item(
            format!(
                "{} devices, {} updatable through fwupd{}",
                found.firmware.devices.len(),
                updatable.len(),
                if found.firmware.updates.is_empty() {
                    "; no updates in the metadata fwupd has"
                } else {
                    ""
                }
            ),
            false,
            format!(
                "Updatable through fwupd:\n{}\n\nThe updates come from the metadata fwupd already has; fwupdmgr refresh fetches newer metadata.",
                listing.join("\n")
            ),
            None,
        ));
    }
    rows.push(HardwareRow::Heading("Disks".to_string()));
    for disk in &found.disks {
        let text = format!(
            "{}  {}  {}  {}{}",
            disk.name,
            disk.kind,
            disk.bytes.map(size).unwrap_or_default(),
            disk.model.as_deref().unwrap_or("unknown model"),
            disk.celsius
                .map(|celsius| format!("  {celsius:.0} °C"))
                .unwrap_or_default()
        );
        rows.push(item(
            text.clone(),
            false,
            format!(
                "{text}\nFirmware: {}\n\nSMART health needs root to read; s checks it after confirmation. It only reads the disk's own health record.",
                disk.firmware.as_deref().unwrap_or("unknown")
            ),
            Some(HardwareAct::Smart(disk.name.clone())),
        ));
    }
    rows.push(HardwareRow::Heading("Temperatures".to_string()));
    let hot: Vec<&helixforge_engine::hardware::Sensor> =
        found.sensors.iter().filter(|sensor| sensor.hot()).collect();
    let warmest = found
        .sensors
        .iter()
        .max_by(|a, b| a.celsius.total_cmp(&b.celsius));
    let all: Vec<String> = found
        .sensors
        .iter()
        .map(|sensor| {
            format!(
                "{} {}: {:.1} °C{}",
                sensor.chip,
                sensor.label,
                sensor.celsius,
                sensor
                    .critical
                    .map(|critical| format!(" (critical {critical:.0})"))
                    .unwrap_or_default()
            )
        })
        .collect();
    match warmest {
        Some(warmest) => rows.push(item(
            format!(
                "{} sensors; the warmest {} {} at {:.0} °C{}",
                found.sensors.len(),
                warmest.chip,
                warmest.label,
                warmest.celsius,
                if hot.is_empty() {
                    String::new()
                } else {
                    format!("; {} near their limit", hot.len())
                }
            ),
            !hot.is_empty(),
            all.join("\n"),
            None,
        )),
        None => rows.push(item(
            "No temperature sensors found".to_string(),
            false,
            String::new(),
            None,
        )),
    }
    if !found.batteries.is_empty() {
        rows.push(HardwareRow::Heading("Batteries".to_string()));
    }
    for battery in &found.batteries {
        let text = format!(
            "{}: {}{}{}",
            battery.name,
            battery
                .capacity
                .map(|capacity| format!("{capacity}% charged"))
                .unwrap_or_else(|| "charge unknown".to_string()),
            battery
                .health
                .map(|health| format!(", holds {health}% of its design capacity"))
                .unwrap_or_default(),
            battery
                .cycles
                .map(|cycles| format!(", {cycles} cycles"))
                .unwrap_or_default()
        );
        rows.push(item(
            text.clone(),
            battery.health.is_some_and(|health| health < 70),
            format!(
                "{text}\nStatus: {}\nModel: {}\nType: {}\n\nBelow about 70% of its design capacity a battery is usually worth replacing.",
                battery.status.as_deref().unwrap_or("unknown"),
                battery.model.as_deref().unwrap_or("unknown"),
                battery.technology.as_deref().unwrap_or("unknown")
            ),
            None,
        ));
    }
    for (title, devices) in [("PCI devices", &found.pci), ("USB devices", &found.usb)] {
        rows.push(HardwareRow::Heading(title.to_string()));
        for device in devices {
            let text = format!(
                "{}{}",
                device.name,
                device
                    .driver
                    .as_deref()
                    .map(|driver| format!("  [{driver}]"))
                    .unwrap_or_default()
            );
            rows.push(item(
                text.clone(),
                false,
                format!(
                    "{text}\nAddress: {}{}",
                    device.address,
                    device
                        .class
                        .as_deref()
                        .map(|class| format!("\nClass or id: {class}"))
                        .unwrap_or_default()
                ),
                None,
            ));
        }
    }
    rows
}

/// Whether queueing a difference's entry would put back what it names.
fn restorable(kind: drift::Kind) -> bool {
    !matches!(kind, drift::Kind::Added | drift::Kind::Version)
}

fn removable(entry: &Entry) -> bool {
    matches!(
        entry.action,
        Action::Package {
            removable: true,
            ..
        }
    )
}

fn build_menu(catalog: &Catalog) -> Vec<MenuItem> {
    let area = |id: &str| catalog.tabs.iter().position(|tab| tab.id == id);
    let mut menu = vec![MenuItem {
        label: "Dashboard".into(),
        screen: Screen::Dashboard,
        section: "MAIN MENU",
    }];
    if let Some(index) = area("applications") {
        menu.push(MenuItem {
            label: catalog.tabs[index].name.clone(),
            screen: Screen::Area(index),
            section: "MAIN MENU",
        });
    }
    menu.push(MenuItem {
        label: "Favorites".into(),
        screen: Screen::Favorites,
        section: "MAIN MENU",
    });
    menu.push(MenuItem {
        label: "Install Plan".into(),
        screen: Screen::Plan,
        section: "MAIN MENU",
    });
    for (index, tab) in catalog.tabs.iter().enumerate() {
        if tab.id != "applications" {
            menu.push(MenuItem {
                label: tab.name.clone(),
                // Helix Doctor opens the health checks, its reports below them.
                screen: if tab.id == "doctor" {
                    Screen::Doctor
                } else {
                    Screen::Area(index)
                },
                section: "MORE TOOLS",
            });
        }
        if tab.id == "configurations" {
            menu.push(MenuItem {
                label: "Desktop".into(),
                screen: Screen::Desktop,
                section: "MORE TOOLS",
            });
        }
        if tab.id == "doctor" {
            menu.push(MenuItem {
                label: "Hardware".into(),
                screen: Screen::Hardware,
                section: "MORE TOOLS",
            });
        }
    }
    menu.push(MenuItem {
        label: "Drift".into(),
        screen: Screen::Drift,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Reliability".into(),
        screen: Screen::Reliability,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Updates".into(),
        screen: Screen::Updates,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "History".into(),
        screen: Screen::History,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Backups".into(),
        screen: Screen::Backups,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Settings".into(),
        screen: Screen::Settings,
        section: "MORE TOOLS",
    });
    menu
}

/// A date and time for a list; UTC is fine there.
fn timestamp(unix: u64) -> String {
    crate::when(unix)
}

/// Terminal output without its escape sequences, for reading a saved log.
fn strip_escapes(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(character) = chars.next() {
        if character == '\u{1b}' {
            match chars.peek() {
                Some('[') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next.is_ascii_alphabetic() || next == '~' {
                            break;
                        }
                    }
                }
                Some(']') => {
                    chars.next();
                    for next in chars.by_ref() {
                        if next == '\u{7}' {
                            break;
                        }
                    }
                }
                _ => {
                    chars.next();
                }
            }
        } else if character != '\r' {
            result.push(character);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app() -> App {
        let args = TuiArgs {
            override_validation: true,
            bypass_root: true,
            ..TuiArgs::default()
        };
        let platform =
            LinuxInfo::from_os_release("ID=garuda\nID_LIKE=arch\nPRETTY_NAME=\"Garuda Linux\"\n")
                .unwrap();
        let mut app = App::new(Catalog::builtin(false).unwrap(), platform, &args);
        // Plain text keeps these screen tests readable; one test draws icons.
        app.icons = false;
        app
    }

    fn screen(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        let buffer = terminal.backend().buffer();
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
                    + "\n"
            })
            .collect()
    }

    fn press(app: &mut App, code: KeyCode) -> bool {
        app.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    fn type_text(app: &mut App, text: &str) {
        for character in text.chars() {
            press(app, KeyCode::Char(character));
        }
    }

    #[test]
    fn the_dashboard_is_the_first_screen() {
        let mut app = app();
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("HELIXFORGE"));
        assert!(text.contains("SYSTEM READY"));
        assert!(text.contains("MAIN MENU"));
        assert!(text.contains("MORE TOOLS"));
        assert!(text.contains("Dashboard"));
        assert!(text.contains("System Overview"));
        assert!(text.contains("Garuda Linux"));
        assert!(text.contains("Quick Actions"));
        assert!(text.contains("Browse Applications"));
        assert!(text.contains("Navigate"));
    }

    fn with_banner(mut app: App) -> App {
        #[allow(deprecated)]
        let mut picker = ratatui_image::picker::Picker::from_fontsize((9, 19).into());
        picker.set_protocol_type(ratatui_image::picker::ProtocolType::Kitty);
        app.icons = true;
        app.with_banner(banner::encode(&picker))
    }

    #[test]
    fn the_banner_replaces_the_text_header_where_it_can_be_drawn() {
        let mut app = with_banner(app());
        let text = screen(&mut app, 120, 40);
        let lines: Vec<&str> = text.lines().collect();
        assert!(lines[0].contains('\u{10EEEE}'), "{}", lines[0]);
        assert!(lines[3].contains('\u{10EEEE}'));
        assert!(lines[1].contains("SYSTEM READY"));
        assert!(!text.contains("BUILD • CONFIGURE"));
        // Four rows of picture, then the rule and the sidebar below it.
        assert!(lines[4].contains('─'));
        assert!(lines[6].contains("MAIN MENU"));
    }

    #[test]
    fn the_text_header_returns_without_icons_in_sixteen_colors_or_when_narrow() {
        let mut app = with_banner(app());
        app.icons = false;
        assert!(screen(&mut app, 120, 40).contains("BUILD • CONFIGURE"));
        app.icons = true;
        app.theme = Theme::Compatible;
        assert!(screen(&mut app, 120, 40).contains("BUILD • CONFIGURE"));
        app.theme = Theme::Solid;
        app.size_bypass = true;
        let narrow = screen(&mut app, 40, 30);
        assert!(!narrow.contains('\u{10EEEE}'));
        assert!(narrow.contains("HELIXFORGE"));
    }

    #[test]
    fn a_small_terminal_gets_a_warning_unless_bypassed() {
        let mut app = app();
        assert!(screen(&mut app, 60, 20).contains("needs 100 by 25"));
        app.size_bypass = true;
        assert!(screen(&mut app, 60, 20).contains("HELIXFORGE"));
    }

    #[test]
    fn the_sidebar_opens_areas_and_groups_remember_their_row() {
        let mut app = app();
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.screen, Screen::Area(0));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Main);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("[D]  Developer Tools"));
        assert!(text.contains("FLAGS"));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.areas[0].path.len(), 1);
        assert!(screen(&mut app, 120, 40).contains("[D]  .."));
        press(&mut app, KeyCode::Char('h'));
        assert!(app.at_root());
        assert_eq!(app.main_state.selected(), Some(1));
        press(&mut app, KeyCode::Char('h'));
        assert_eq!(app.focus, Focus::Sidebar);
    }

    #[test]
    fn the_install_plan_saves_as_a_profile_that_appears_under_profiles() {
        let mut app = app();
        let directory = tempfile::tempdir().unwrap();
        app.profiles_dir = Some(directory.path().to_path_buf());
        app.queue = vec![
            app.catalog.find("git").unwrap().clone(),
            app.catalog.find("meld").unwrap().clone(),
        ];
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        assert!(screen(&mut app, 140, 40).contains("s Save as profile"));
        press(&mut app, KeyCode::Char('s'));
        assert!(screen(&mut app, 140, 40).contains("Save as a profile"));
        // A blank name is not accepted; Escape would cancel.
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::ProfileName(_))));
        type_text(&mut app, "Dev Box");
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("Profile saved"), "{shown}");
        let saved = directory.path().join("dev-box.toml");
        assert!(saved.exists());
        let entry = app.catalog.find("profile-dev-box").unwrap();
        assert!(
            matches!(&entry.action, Action::Profile { entries } if entries == &["git", "meld"])
        );
    }

    #[test]
    fn quick_actions_open_screens() {
        let mut app = app();
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Doctor);
        assert!(screen(&mut app, 120, 40).contains("Machine Summary"));
    }

    #[test]
    fn the_doctor_shows_evidence_and_repairs_with_their_own_confirmation() {
        let mut app = app();
        app.diagnoses = vec![
            doctor::Diagnosis {
                id: "network".into(),
                title: "Network".into(),
                health: doctor::Health::Good,
                summary: "Connected through wlan0".into(),
                evidence: vec!["default routes: wlan0".into()],
                advice: None,
                repair: None,
                entry: None,
            },
            doctor::Diagnosis {
                id: "permissions".into(),
                title: "Home folder permissions".into(),
                health: doctor::Health::Problem,
                summary: "1 item(s) in your home folder belong to root".into(),
                evidence: vec!["owned by root: /home/me/.local/share/Shelly".into()],
                advice: Some("This usually comes from running a program with sudo.".into()),
                repair: Some(doctor::Repair {
                    label: "give them back to you".into(),
                    commands: vec![vec![
                        "sudo".into(),
                        "chown".into(),
                        "-R".into(),
                        "1000:1000".into(),
                        "--".into(),
                        "/home/me/.local/share/Shelly".into(),
                    ]],
                }),
                entry: None,
            },
        ];
        app.open_screen(Screen::Doctor);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 160, 40);
        assert!(shown.contains("OK      Network"), "{shown}");
        assert!(shown.contains("Problem Home folder permissions"));
        assert!(shown.contains("f repairs"));
        assert!(
            shown.contains("Report  Machine Summary"),
            "the reports stay below"
        );
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("owned by root: /home/me/.local/share/Shelly"),
            "{shown}"
        );
        assert!(shown.contains("What to do: This usually comes from"));
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('f'));
        assert!(
            matches!(app.popup, Some(Popup::Confirm(_))),
            "its own confirmation"
        );
        assert!(
            screen(&mut app, 160, 40)
                .contains("sudo chown -R 1000:1000 -- /home/me/.local/share/Shelly")
        );
        app.popup = None;
        app.pending = None;
        // A report below the checks runs like any entry.
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    }

    #[test]
    fn entries_show_how_they_stand() {
        let mut app = app();
        app.statuses.insert(
            "ripgrep".into(),
            Status::Update {
                source: "pacman".into(),
                version: Some("15.2.0-1".into()),
                latest: Some("15.3.0-1".into()),
                shadowed: None,
            },
        );
        app.statuses.insert(
            "fzf".into(),
            Status::Managed {
                by: "Home Manager".into(),
                version: Some("0.74.4".into()),
            },
        );
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        let wide = screen(&mut app, 140, 40);
        let row = wide.lines().find(|line| line.contains("Ripgrep")).unwrap();
        assert!(
            row.contains("Update  pacman 15.2.0-1 → 15.3.0-1  I"),
            "{row}"
        );
        assert!(wide.contains("STATUS · FLAGS"));
        // Short of room, the detail goes first and the word stays.
        app.size_bypass = true;
        let narrow = screen(&mut app, 70, 30);
        let row = narrow
            .lines()
            .find(|line| line.contains("Ripgrep"))
            .unwrap();
        assert!(row.contains("Update") && !row.contains("15.3.0-1"), "{row}");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "fzf");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(
            screen(&mut app, 140, 40).contains("Status: Managed (Home Manager 0.74.4)"),
            "the description says it too"
        );
    }

    #[test]
    fn backups_are_listed_described_and_restored_and_snapshots_go_first() {
        let mut app = app();
        app.backups = vec![backups::Backup {
            backup: "/home/me/.config/ghostty/config.commander-backup-20261001000000-1".into(),
            original: "/home/me/.config/ghostty/config".into(),
            taken: Some("2026-10-01 00:00:00".into()),
            system: false,
            directory: false,
        }];
        app.snapshot_tool = Some(backups::Snapshots::Snapper);
        app.screen = Screen::Backups;
        app.focus = Focus::Main;
        app.main_state.select(Some(0));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("2026-10-01 00:00:00"), "{shown}");
        assert!(shown.contains("ghostty/config"));
        assert!(shown.contains("Snapper is set up"));
        assert!(shown.contains("does not roll packages"), "{shown}");
        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 140, 40).contains("moved aside as its own backup"));
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("cp -a /home/me/.config/ghostty/config.commander-backup"),
            "{shown}"
        );
        assert!(
            !shown.contains("Take a Snapper snapshot"),
            "a user file needs no snapshot"
        );
        app.popup = None;
        app.pending = None;
        // A GRUB theme changes system files: the snapshot comes first.
        let grub = app.catalog.find("grub").unwrap().clone();
        app.confirm(vec![grub]);
        let shown = screen(&mut app, 160, 40);
        assert!(shown.contains("1. Take a Snapper snapshot"), "{shown}");
        app.popup = None;
        app.pending = None;
        app.snapshots = false;
        app.confirm(vec![app.catalog.find("grub").unwrap().clone()]);
        assert!(
            !screen(&mut app, 160, 40).contains("Snapper snapshot"),
            "off in Settings"
        );
    }

    #[test]
    fn a_blocked_plan_says_why_instead_of_asking() {
        let fixtures =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/packages");
        let args = TuiArgs {
            override_validation: true,
            bypass_root: true,
            ..TuiArgs::default()
        };
        let platform = LinuxInfo::from_os_release("ID=arch\nPRETTY_NAME=\"Arch Linux\"\n").unwrap();
        let mut app = App::new(
            Catalog::from_directory(&fixtures, false).unwrap(),
            platform,
            &args,
        );
        app.icons = false;
        for name in ["Conflict A", "Conflict B"] {
            // A fresh search for each.
            press(&mut app, KeyCode::Char('/'));
            press(&mut app, KeyCode::Esc);
            press(&mut app, KeyCode::Char('/'));
            type_text(&mut app, name);
            press(&mut app, KeyCode::Enter);
            press(&mut app, KeyCode::Char(' '));
        }
        assert_eq!(app.queue.len(), 2);
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        let plan = screen(&mut app, 140, 40);
        assert!(
            plan.contains("Blocked: Conflict A and Conflict B replace each other"),
            "{plan}"
        );
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Text(_))), "no confirmation");
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("Cannot run yet") && shown.contains("Nothing was changed"),
            "{shown}"
        );
        assert!(app.pending.is_none());
    }

    #[test]
    fn u_plans_an_update_and_waits_for_the_status_check() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('u'));
        assert!(screen(&mut app, 140, 40).contains("still checking what is installed"));
        press(&mut app, KeyCode::Esc);
        let inventory = Inventory::from_parts(
            Some(helixforge_core::PackageManager::Arch),
            Some([("ripgrep".to_string(), "15.2.0-1".to_string())].into()),
            [("ripgrep".to_string(), "15.3.0-1".to_string())].into(),
            HashMap::new(),
            Default::default(),
        );
        let ripgrep = app.catalog.find("ripgrep").unwrap().clone();
        let status = inventory
            .status(&ripgrep, &app.platform, &|_| None)
            .unwrap();
        assert_eq!(status.label(), "Update");
        app.statuses.insert("ripgrep".into(), status);
        app.inventory = Some(inventory);
        assert!(
            screen(&mut app, 140, 40).contains("u Update"),
            "the hint offers it"
        );
        press(&mut app, KeyCode::Char('u'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("sudo pacman -Syu"), "{shown}");
        assert!(shown.contains("partial upgrade"));
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('U'));
        assert!(
            matches!(app.popup, Some(Popup::Confirm(_))),
            "U finds it too"
        );
    }

    #[test]
    fn search_spans_every_area_and_escape_clears_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        assert!(matches!(app.screen, Screen::Area(_)));
        type_text(&mut app, "git");
        assert!(!app.rows.is_empty());
        // Names holding the text come before looser matches.
        let names = entry_names(&app);
        let holding = names
            .iter()
            .take_while(|name| name.to_lowercase().contains("git"))
            .count();
        assert!(holding > 0);
        assert!(
            names[holding..]
                .iter()
                .all(|name| !name.to_lowercase().contains("git")),
            "{names:?}"
        );
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.focus, Focus::Main);
        press(&mut app, KeyCode::Char('/'));
        press(&mut app, KeyCode::Esc);
        assert!(app.search.is_empty());
        assert!(app.rows.iter().any(|row| matches!(row, Row::Group { .. })));
    }

    #[test]
    fn entries_queue_into_the_install_plan_and_run_from_it() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(' '));
        assert!(
            matches!(app.popup, Some(Popup::Text(_))),
            "a solo entry explains it cannot be queued"
        );
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('h'));
        type_text(&mut app, "/Machine Summary");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char(' '));
        assert_eq!(app.queue.len(), 1);
        assert!(screen(&mut app, 120, 40).contains("Machine Summary *"));
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Install Plan"));
        assert!(text.contains("1 step, risk low"));
        assert!(text.contains("helixforge native system-report"));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 120, 40).contains("Risk: low"));
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('x'));
        assert!(app.queue.is_empty());
        assert!(screen(&mut app, 120, 40).contains("Nothing is queued"));
    }

    #[test]
    fn running_an_entry_asks_first_and_quitting_works() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 120, 40).contains("Print the distribution"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('p'));
        assert!(screen(&mut app, 120, 40).contains("HELIXFORGE_LIB"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("1. System Report (script)"), "{text}");
        assert!(text.contains("system-report.sh"), "{text}");
        press(&mut app, KeyCode::Char('n'));
        assert!(app.popup.is_none());
        assert!(!press(&mut app, KeyCode::Char('q')));
    }

    #[test]
    fn removable_entries_offer_removal_and_others_do_not() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "Tenacity");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("Remove"));
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("not installed") || text.contains("Remove Tenacity"),
            "{text}"
        );
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('h'));
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "Machine Summary");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('r'));
        assert!(
            app.popup.is_none(),
            "a non-package entry has nothing to remove"
        );
    }

    fn entry_names(app: &App) -> Vec<String> {
        app.rows
            .iter()
            .filter_map(|row| match row {
                Row::Entry(entry) => Some(entry.name.clone()),
                _ => None,
            })
            .collect()
    }

    fn area_of(app: &App, id: &str) -> usize {
        app.catalog
            .tabs
            .iter()
            .position(|tab| tab.entries().iter().any(|entry| entry.id == id))
            .unwrap()
    }

    #[test]
    fn search_puts_the_closest_names_first() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "git");
        let names = entry_names(&app);
        assert_eq!(names.first().map(String::as_str), Some("Git"), "{names:?}");
        // A loose spelling still finds it.
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "rpgrp");
        assert!(entry_names(&app).contains(&"Ripgrep".to_string()));
    }

    #[test]
    fn escaping_a_search_returns_to_the_row_it_left() {
        let mut app = app();
        app.open_screen(Screen::Area(area_of(&app, "ripgrep")));
        app.focus = Focus::Main;
        app.main_state.select(Some(2));
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "zzz");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.main_state.selected(), Some(2));
    }

    #[test]
    fn the_filter_shows_installed_entries_then_updates() {
        let mut app = app();
        app.statuses.insert(
            "ripgrep".into(),
            Status::Update {
                source: "pacman".into(),
                version: Some("15.2.0-1".into()),
                latest: Some("15.3.0-1".into()),
                shadowed: None,
            },
        );
        app.statuses.insert(
            "fzf".into(),
            Status::Managed {
                by: "Home Manager".into(),
                version: None,
            },
        );
        let index = area_of(&app, "ripgrep");
        assert_eq!(index, area_of(&app, "fzf"), "both share an area");
        app.open_screen(Screen::Area(index));
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(entry_names(&app), ["fzf", "Ripgrep"]);
        assert!(screen(&mut app, 120, 40).contains("Entries · installed"));
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(entry_names(&app), ["Ripgrep"]);
        press(&mut app, KeyCode::Char('i'));
        assert!(entry_names(&app).len() > 2, "back to everything");
    }

    #[test]
    fn pinned_and_recent_entries_are_listed_as_favorites() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("favourites.toml");
        let mut app = app();
        app.favorites_path = Some(path.clone());
        assert!(screen(&mut app, 120, 40).contains("Favorites"));
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('*'));
        assert_eq!(settings::load_favorites(&path), ["ripgrep"]);
        assert!(screen(&mut app, 120, 40).contains("Ripgrep ★"));

        app.search.clear();
        app.screen = Screen::Favorites;
        app.take_recent(vec![history::Record {
            unix_timestamp: 1,
            entries: vec!["fzf".into(), "ripgrep".into(), "gone".into()],
            names: Vec::new(),
            success: true,
            result: "exit 0".into(),
            log: None,
        }]);
        app.update_rows();
        app.main_state.select(Some(0));
        assert_eq!(entry_names(&app), ["Ripgrep", "fzf"], "pins come first");
        press(&mut app, KeyCode::Char('*'));
        assert!(settings::load_favorites(&path).is_empty());
        assert_eq!(entry_names(&app), ["fzf", "Ripgrep"]);
        app.recent.clear();
        app.update_rows();
        assert!(screen(&mut app, 120, 40).contains("Nothing here yet"));
    }

    fn channel(id: &str, name: &str, pending: Option<Vec<updates::Pending>>) -> updates::Channel {
        updates::Channel {
            id: id.into(),
            name: name.into(),
            pending,
            note: "from the cache".into(),
            commands: vec![vec!["sudo".into(), "pacman".into(), "-Syu".into()]],
            requires: vec!["pacman".into(), "sudo".into()],
            caveat: Some("updates the whole system".into()),
        }
    }

    #[test]
    fn the_updates_screen_lists_channels_and_updates_through_them() {
        let mut app = app();
        app.channels = vec![
            channel(
                "pacman",
                "Arch packages",
                Some(vec![updates::Pending {
                    name: "mesa".into(),
                    from: Some("1-1".into()),
                    to: Some("2-1".into()),
                }]),
            ),
            channel("home-manager", "Home Manager", None),
        ];
        app.open_screen(Screen::Updates);
        app.focus = Focus::Main;
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Arch packages"), "{text}");
        assert!(text.contains("1 update"));
        assert!(text.contains("not counted"));
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("mesa  1-1 → 2-1"), "{text}");
        assert!(text.contains("u runs: sudo pacman -Syu"));
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        press(&mut app, KeyCode::Char('U'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Update Arch packages"), "{text}");
        assert!(
            !text.contains("Update Home Manager"),
            "uncounted channels wait for u"
        );
        press(&mut app, KeyCode::Char('n'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('u'));
        assert!(screen(&mut app, 120, 40).contains("Update Home Manager"));
        press(&mut app, KeyCode::Char('n'));
        // A copy that updates itself has nothing to run.
        let mut launcher = channel("helixforge", "HelixForge", Some(Vec::new()));
        launcher.commands.clear();
        launcher.note = "1.0.4; it fetches the latest release each time it starts".into();
        app.channels.push(launcher);
        app.main_state.select(Some(2));
        press(&mut app, KeyCode::Char('u'));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Nothing to run"), "{text}");
        assert!(text.contains("each time it starts"), "{text}");
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        app.channels.truncate(1);
        app.channels[0].pending = Some(Vec::new());
        app.clamp_main();
        press(&mut app, KeyCode::Char('U'));
        assert!(screen(&mut app, 120, 40).contains("Nothing to update"));
    }

    #[test]
    fn the_history_screen_lists_records() {
        let state = tempfile::tempdir().unwrap();
        let mut app = app();
        history::record_in(
            state.path(),
            &[("git".into(), "Git".into())],
            false,
            "exit 1",
            None,
        )
        .unwrap();
        app.records = history::read_in(state.path(), 10).unwrap();
        app.screen = Screen::History;
        app.focus = Focus::Main;
        app.clamp_main();
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("failed"));
        assert!(text.contains("Git"));
        assert!(text.contains("exit 1"));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("No log was saved"));
    }

    #[test]
    fn settings_change_the_theme_and_the_confirmation() {
        let mut app = app();
        app.open_screen(Screen::Settings);
        app.focus = Focus::Main;
        assert!(screen(&mut app, 120, 40).contains("Ask before running"));
        assert_eq!(app.theme, Theme::Solid, "solid is the default");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.theme, Theme::Transparent);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(app.icons);
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(app.skip_confirmation);
    }

    #[test]
    fn help_and_flags_guide_open_as_popups() {
        let mut app = app();
        press(&mut app, KeyCode::Char('?'));
        assert!(screen(&mut app, 120, 40).contains("Install Plan"));
        press(&mut app, KeyCode::Esc);
        app.open_screen(Screen::Area(0));
        press(&mut app, KeyCode::Char('g'));
        assert!(screen(&mut app, 120, 40).contains("privileged"));
    }

    #[test]
    fn the_dashboard_draws_cards_and_icons_in_every_theme() {
        let mut app = app();
        app.icons = true;
        for theme in [
            Theme::Solid,
            Theme::Transparent,
            Theme::Compatible,
            Theme::Monochrome,
        ] {
            app.theme = theme;
            let text = screen(&mut app, 120, 40);
            assert!(text.contains("\u{f0e4}  Dashboard"), "{text}");
            assert!(text.contains("\u{f0ad}  Maintenance"), "{text}");
            assert!(text.contains("╭"), "{text}");
            assert!(text.contains("See what is queued and run it"), "{text}");
            assert!(text.contains("? Help"), "{text}");
        }
        // The smallest supported size still shows every quick action.
        let text = screen(&mut app, 100, 25);
        assert!(text.contains("View History"), "{text}");
        assert!(text.contains("Install Plan"), "{text}");
        app.icons = false;
        assert!(!screen(&mut app, 120, 40).contains('\u{f0e4}'));
    }

    #[test]
    fn monochrome_draws_no_color_and_still_shows_the_selection() {
        use ratatui::style::{Color, Modifier};
        let mut app = app();
        app.theme = Theme::Monochrome;
        app.open_screen(Screen::Area(0));
        app.focus = Focus::Main;
        for (width, height) in [(120, 40), (100, 25)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| app.draw(frame)).unwrap();
            let buffer = terminal.backend().buffer();
            let mut reversed = 0;
            for cell in buffer.content() {
                assert_eq!(cell.fg, Color::Reset, "{cell:?}");
                assert_eq!(cell.bg, Color::Reset, "{cell:?}");
                if cell.modifier.contains(Modifier::REVERSED) {
                    reversed += 1;
                }
            }
            assert!(reversed > 0, "the selection shows in reverse video");
        }
        // The confirmation's buttons too.
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal.draw(|frame| app.draw(frame)).unwrap();
        assert!(
            terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .all(|cell| cell.fg == Color::Reset && cell.bg == Color::Reset)
        );
    }

    #[test]
    fn no_color_is_for_one_run_and_the_chosen_theme_is_remembered() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.toml");
        let mut app = app();
        app.saved_theme = Theme::Transparent;
        app.theme = Theme::Monochrome;
        app.settings_path = Some(path.clone());
        app.icons = false;
        app.save_look();
        assert_eq!(settings::load(&path).theme, Theme::Transparent);
        // Choosing a theme remembers it.
        press(&mut app, KeyCode::Char('t'));
        assert_eq!(app.theme, Theme::Solid);
        assert_eq!(settings::load(&path).theme, Theme::Solid);
    }

    fn arch_packages(names: &[&str]) -> Inventory {
        Inventory::from_parts(
            Some(helixforge_core::PackageManager::Arch),
            Some(
                names
                    .iter()
                    .map(|name| (name.to_string(), "1-1".to_string()))
                    .collect(),
            ),
            HashMap::new(),
            HashMap::new(),
            Default::default(),
        )
    }

    #[test]
    fn drift_compares_with_a_saved_baseline_and_plans_what_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = app();
        app.baselines_dir = Some(directory.path().to_path_buf());
        app.inventory = Some(arch_packages(&["git", "ripgrep"]));
        app.open_screen(Screen::Drift);
        app.focus = Focus::Main;
        // Before any baseline, the profiles are there to compare with.
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Profile  Developer Basics"), "{text}");
        assert!(text.contains("Missing"), "{text}");
        press(&mut app, KeyCode::Char('s'));
        type_text(&mut app, "Old box");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("Baseline saved"));
        assert!(directory.path().join("old-box.toml").is_file());
        press(&mut app, KeyCode::Esc);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Baseline  Old box"), "{text}");
        assert!(text.contains("No differences"), "{text}");
        assert!(text.contains("b: another baseline or profile (1 of"));

        // Ripgrep goes away; Drift names it and plans it back.
        app.inventory = Some(arch_packages(&["git"]));
        press(&mut app, KeyCode::Char('r'));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Missing   Ripgrep"), "{text}");
        assert!(text.contains("a Plan all missing"), "{text}");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("The Ripgrep entry puts it back"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char(' '));
        assert!(screen(&mut app, 140, 40).contains("· planned"));
        press(&mut app, KeyCode::Char(' '));
        assert!(app.queue.is_empty());
        press(&mut app, KeyCode::Char('a'));
        assert_eq!(app.screen, Screen::Plan);
        let ids: Vec<&str> = app.queue.iter().map(|entry| entry.id.as_str()).collect();
        assert_eq!(ids, ["ripgrep"]);
        assert!(screen(&mut app, 140, 40).contains("Nothing has run"));
        // b moves on to the profiles.
        app.popup = None;
        app.open_screen(Screen::Drift);
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('b'));
        assert!(screen(&mut app, 140, 40).contains("Profile  "));
    }

    #[test]
    fn ctrl_k_finds_screens_actions_and_entries() {
        let mut app = app();
        let control_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::CONTROL);
        app.handle_key(&control_k);
        assert!(matches!(app.popup, Some(Popup::Palette(_))));
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("COMMANDS"), "{text}");
        assert!(text.contains("Check updates online"), "{text}");
        type_text(&mut app, "drift");
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none());
        assert_eq!(app.screen, Screen::Drift);
        assert_eq!(app.focus, Focus::Main);

        app.handle_key(&control_k);
        type_text(&mut app, "run ghostty");
        assert!(screen(&mut app, 120, 40).contains("Run Ghostty"));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        press(&mut app, KeyCode::Char('n'));

        app.handle_key(&control_k);
        type_text(&mut app, "dns");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Doctor);
        assert_eq!(app.doctor_wanted, Some("dns"));
        // The check's details open once the checks have run.
        app.diagnoses = vec![doctor::Diagnosis {
            id: "dns".into(),
            title: "Name resolution".into(),
            health: doctor::Health::Good,
            summary: "github.com resolves".into(),
            evidence: Vec::new(),
            advice: None,
            repair: None,
            entry: None,
        }];
        let wanted = app.doctor_wanted.take().unwrap();
        app.show_check(wanted);
        assert!(screen(&mut app, 120, 40).contains("github.com resolves"));

        // Escape closes the palette without doing anything; so does Ctrl-K.
        app.popup = None;
        app.handle_key(&control_k);
        press(&mut app, KeyCode::Esc);
        assert!(app.popup.is_none());
        app.handle_key(&control_k);
        app.handle_key(&control_k);
        assert!(app.popup.is_none());
    }

    #[test]
    fn reliability_lists_critical_entries_first_and_explains_each() {
        let mut app = app();
        app.open_screen(Screen::Reliability);
        app.focus = Focus::Main;
        let text = screen(&mut app, 140, 40);
        // A release build carries the Distributions record; others say they have none.
        assert!(
            text.contains("No container record")
                || text.contains("Container record: Distributions run"),
            "{text}"
        );
        assert!(text.contains("Stand-in Tested"), "{text}");
        // The first rows are critical: boot, login, drivers, whole systems.
        assert!(app.reliability_rows[0].2.critical);
        assert!(text.contains("! "), "{text}");
        press(&mut app, KeyCode::Char('i'));
        assert_eq!(app.coverage, Coverage::Critical);
        assert!(app.reliability_rows.iter().all(|(_, _, row)| row.critical));
        press(&mut app, KeyCode::Char('i'));
        assert!(
            app.reliability_rows
                .iter()
                .all(|(_, _, row)| row.level >= reliability::Level::StandInTested)
        );
        press(&mut app, KeyCode::Char('i'));
        let sddm = app
            .reliability_rows
            .iter()
            .position(|(id, _, _)| id == "sddm")
            .unwrap();
        app.main_state.select(Some(sddm));
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("A critical entry"), "{text}");
        assert!(text.contains("test_sddm"), "{text}");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('v'));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Trust: SDDM Theme"), "{text}");
        assert!(text.contains("Pinned revision"), "{text}");
    }

    #[test]
    fn v_inspects_an_entry_and_the_confirmation_warns_of_untested_boot_changes() {
        let mut app = app();
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("v Trust"));
        press(&mut app, KeyCode::Char('v'));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Package integrity"), "{text}");
        assert!(text.contains("built from source"), "{text}");
        press(&mut app, KeyCode::Esc);
        app.popup = None;
        // This build's own claims without a record, so a release build's
        // record (where the theme may be VM Tested) does not change the test.
        let mut evidence = reliability::Evidence::load(app.catalog.root());
        evidence.results = None;
        app.evidence = Some(evidence);
        let grub = app.catalog.find("grub-dracula").unwrap().clone();
        app.confirm(vec![grub]);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("only tested with stand-ins"), "{text}");
    }

    #[test]
    fn the_change_preview_opens_from_the_plan_the_confirmation_and_backups() {
        let mut app = app();
        app.queue
            .push(app.catalog.find("grub-dracula").unwrap().clone());
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        assert!(screen(&mut app, 140, 40).contains("e Changes"));
        press(&mut app, KeyCode::Char('e'));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("CHANGES"), "{text}");
        assert!(text.contains("Dracula GRUB Theme"), "{text}");
        assert!(text.contains("not known in advance"), "{text}");
        assert!(text.contains("grub-mkconfig"), "{text}");
        press(&mut app, KeyCode::Esc);
        assert!(app.popup.is_none());

        // From the confirmation, and back to it.
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 140, 40).contains("[E] Changes"));
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(app.popup, Some(Popup::Effects(_))));
        assert!(screen(&mut app, 140, 40).contains("Back to the confirmation"));
        press(&mut app, KeyCode::Esc);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(app.pending.is_some());
        press(&mut app, KeyCode::Char('n'));

        // A backup's restore, side by side with the file there now.
        let directory = tempfile::tempdir().unwrap();
        let original = directory.path().join("app.conf");
        std::fs::write(&original, "color = red\nsize = 2\n").unwrap();
        let copy = directory
            .path()
            .join("app.conf.helixforge-backup-20261016120000");
        std::fs::write(&copy, "color = blue\nsize = 2\n").unwrap();
        app.backups = vec![backups::Backup {
            backup: copy,
            original,
            taken: None,
            system: false,
            directory: false,
        }];
        app.screen = Screen::Backups;
        app.main_state.select(Some(0));
        press(&mut app, KeyCode::Char('e'));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("[side by side]"), "{text}");
        assert!(text.contains("Enter Side by side"), "{text}");
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("SIDE BY SIDE"), "{text}");
        assert!(text.contains("color = red"), "{text}");
        assert!(text.contains("color = blue"), "{text}");
        press(&mut app, KeyCode::Esc);
        assert!(screen(&mut app, 140, 40).contains("CHANGES"));
    }

    #[test]
    fn hardware_lists_firmware_disks_and_batteries_and_asks_before_changing() {
        use helixforge_engine::hardware::{Battery, Disk, Firmware, FirmwareUpdate, Hardware};
        let found = Hardware {
            firmware: Firmware {
                available: true,
                updates: vec![FirmwareUpdate {
                    device_id: "9ea4da9853eae668f5bd65b10d14e4363f619406".into(),
                    device: "Bolt Receiver".into(),
                    from: Some("MPR05.04".into()),
                    to: "MPR05.06".into(),
                    summary: Some("Fixes pairing".into()),
                    urgency: Some("medium".into()),
                }],
                ..Firmware::default()
            },
            disks: vec![Disk {
                name: "nvme0n1".into(),
                model: Some("Samsung SSD 990 PRO 1TB".into()),
                bytes: Some(1_000_204_886_016),
                kind: "NVMe".into(),
                firmware: Some("8B2QJXD7".into()),
                celsius: Some(38.0),
            }],
            batteries: vec![Battery {
                name: "BAT0".into(),
                capacity: Some(100),
                health: Some(65),
                cycles: Some(662),
                ..Battery::default()
            }],
            ..Hardware::default()
        };
        let mut app = app();
        app.hardware_rows = hardware_rows(&found);
        app.hardware = Some(found);
        app.open_screen(Screen::Hardware);
        app.focus = Focus::Main;
        let text = screen(&mut app, 140, 40);
        assert!(
            text.contains("Update for Bolt Receiver: MPR05.04 → MPR05.06"),
            "{text}"
        );
        assert!(
            text.contains("nvme0n1  NVMe  1.0 TB  Samsung SSD 990 PRO 1TB"),
            "{text}"
        );
        assert!(text.contains("holds 65% of its design capacity"), "{text}");
        let update = app
            .hardware_rows
            .iter()
            .position(|row| {
                matches!(
                    row,
                    HardwareRow::Item {
                        act: Some(HardwareAct::Firmware(_)),
                        ..
                    }
                )
            })
            .unwrap();
        app.main_state.select(Some(update));
        assert!(screen(&mut app, 140, 40).contains("f Update firmware"));
        // A firmware update needs the word before its plan is even shown.
        press(&mut app, KeyCode::Char('f'));
        assert!(matches!(app.popup, Some(Popup::FirmwareWord(_))));
        type_text(&mut app, "yes");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("Not confirmed"));
        assert!(app.pending.is_none());
        app.popup = None;
        press(&mut app, KeyCode::Char('f'));
        type_text(&mut app, "FIRMWARE");
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Risk: high"), "{text}");
        assert!(text.contains("sudo fwupdmgr update 9ea4da98"), "{text}");
        press(&mut app, KeyCode::Char('n'));
        let disk = app
            .hardware_rows
            .iter()
            .position(|row| {
                matches!(
                    row,
                    HardwareRow::Item {
                        act: Some(HardwareAct::Smart(_)),
                        ..
                    }
                )
            })
            .unwrap();
        app.main_state.select(Some(disk));
        press(&mut app, KeyCode::Char('s'));
        assert!(
            screen(&mut app, 140, 40).contains("sudo smartctl --health --attributes /dev/nvme0n1")
        );
    }

    #[test]
    fn the_wizard_suggests_a_workstation_and_queues_it_for_review() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = app();
        app.settings_path = Some(directory.path().join("settings.toml"));
        app.set_up = false;
        assert!(app.first_run());
        app.open_wizard(true);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("SET UP THIS MACHINE"), "{text}");
        assert!(text.contains("Welcome to HelixForge"), "{text}");
        assert!(text.contains("Garuda Linux"), "{text}");
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Developer"), "{text}");
        assert!(text.contains("Privacy"), "{text}");
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("WHAT IT ADDS"), "{text}");
        assert!(text.contains("[x] Git"), "{text}");
        // Untick the first, then queue the rest.
        press(&mut app, KeyCode::Char(' '));
        assert!(screen(&mut app, 140, 40).contains("[ ] Git"));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Plan);
        assert!(!app.queue.iter().any(|entry| entry.id == "git"));
        assert!(app.queue.iter().any(|entry| entry.id == "ripgrep"));
        assert!(screen(&mut app, 140, 40).contains("Nothing has run"));
        // Offered once: it is remembered.
        assert!(!app.first_run());
        assert!(settings::load(&directory.path().join("settings.toml")).set_up);
        // Custom starts with nothing ticked.
        app.popup = None;
        app.open_wizard(false);
        press(&mut app, KeyCode::Enter);
        for _ in 0..4 {
            press(&mut app, KeyCode::Char('j'));
        }
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("Custom"), "{text}");
        assert!(!text.contains("[x]"), "{text}");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Esc);
        assert!(app.popup.is_none());
    }

    #[test]
    fn the_desktop_screen_offers_its_entries_and_puts_a_saved_look_back() {
        use helixforge_engine::desktop::{Desktop, Kind, Value};
        let directory = tempfile::tempdir().unwrap();
        let mut app = app();
        app.looks_dir = Some(directory.path().to_path_buf());
        app.desktop = Some(Desktop {
            kind: Some(Kind::Kde),
            name: Some("KDE Plasma 6.8.0".into()),
            session: Some("wayland".into()),
            appearance: vec![Value {
                key: "color-scheme".into(),
                label: "Color scheme".into(),
                value: Some("BreezeDark".into()),
            }],
            owners: vec!["Home Manager runs a Plasma configuration step at every switch".into()],
            ..Desktop::default()
        });
        app.open_screen(Screen::Desktop);
        app.focus = Focus::Main;
        let text = screen(&mut app, 140, 40);
        assert!(text.contains("KDE Plasma 6.8.0, wayland session"), "{text}");
        assert!(text.contains("Color scheme    BreezeDark"), "{text}");
        assert!(text.contains("Note: Home Manager"), "{text}");
        assert!(text.contains("Commander Konsole Config"), "{text}");
        press(&mut app, KeyCode::Char('s'));
        type_text(&mut app, "Dark");
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("Look saved"));
        app.popup = None;
        // The look now differs from the desktop: applying it plans the change.
        let mut changed = app.desktop.clone().unwrap();
        changed.appearance[0].value = Some("Sweet".into());
        app.desktop = Some(changed);
        app.reload_desktop();
        let look = app
            .desktop_rows
            .iter()
            .position(|row| matches!(row, DesktopRow::Look { .. }))
            .unwrap();
        app.main_state.select(Some(look));
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 140, 40);
        assert!(
            text.contains("plasma-apply-colorscheme BreezeDark"),
            "{text}"
        );
        press(&mut app, KeyCode::Char('n'));
        let entry = app
            .desktop_rows
            .iter()
            .position(|row| matches!(row, DesktopRow::Entry { id, .. } if id == "plymouth-on"))
            .unwrap();
        app.main_state.select(Some(entry));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    }

    #[test]
    fn the_sidebar_keeps_every_screen_reachable_in_a_small_terminal() {
        let mut app = app();
        let text = screen(&mut app, 100, 25);
        assert!(text.contains("Dashboard"), "{text}");
        assert!(text.contains("MORE TOOLS"), "{text}");
        app.open_screen(Screen::Settings);
        let text = screen(&mut app, 100, 25);
        assert!(text.contains("Settings"), "{text}");
        assert!(text.contains("Backups"), "{text}");
        // With room, the main menu keeps its cards.
        app.open_screen(Screen::Dashboard);
        assert!(screen(&mut app, 120, 40).contains("╭"));
    }

    #[test]
    fn timestamps_and_escape_stripping_are_sane() {
        assert_eq!(timestamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(timestamp(1_700_000_000), "2023-11-14 22:13 UTC");
        assert_eq!(strip_escapes("a\u{1b}[31mb\u{1b}[0m\r\n"), "ab\n");
    }
}
