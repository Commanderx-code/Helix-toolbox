//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the Helix Toolbox dashboard design.
//!
//! [`App`] holds the state; this file has its startup, the shared state
//! helpers and the confirm-and-run path. The frame is in `layout`, keys and
//! the mouse in `input`, and the catalog areas, search, Favorites, the
//! Install Plan and the command palette in `areas`. Each tool screen is a
//! module in `screens` that owns its state, its background reads, its rows,
//! keys and drawing; `App` holds them and hands the current one its keys,
//! its frame and the runs that end. Settings, the profiles, the reports and
//! the setup wizard add their methods to `App` from a file there.

use super::{
    banner,
    effects::{EffectsPopup, Outcome as EffectsOutcome},
    hints::Hint,
    icons::Icon,
    mouse::{self, Hits, Target},
    palette::{Choice, Outcome, Palette},
    popup::{Answer, ConfirmPopup, Decision, InputPopup, TextPopup, centered},
    running::RunningView,
    settings::{self, Look},
    theme::{self, Theme},
    wizard::{Outcome as WizardOutcome, Pick, Wizard},
};
use crate::TuiArgs;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use helix_toolbox_catalog::Catalog;
use helix_toolbox_core::{Action, Entry, Item};
use helix_toolbox_engine::{
    EngineError, Machine, Plan, RunSession, backups, bundle,
    configs::{self, Checked},
    confirmation::{self, Prepared, Verdict},
    doctor, drift, hardware_report, history, home_manager,
    host::Host,
    inventory::{Inventory, Status, which},
    migration::{self, Standing, Translation},
    notifications,
    overview::Overview,
    performance, plan_removal, plan_with, preflight, profiles, recovery, reliability,
    repositories::{self as repos, Change, Details, Summary},
    restic, rows,
    services::{self, Verb},
    session::Reading,
    storage, updates,
    watch::{self, Watch},
    workspaces::{self, Task, Workspaces},
};
use helix_toolbox_platform::LinuxInfo;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Style},
    symbols::border,
    text::{Line, Span, Text},
    widgets::{Block, List, ListItem, ListState, Padding, Paragraph, Wrap},
};
use ratatui_image::{Image, protocol::Protocol};
use screens::{Read, Screens, ToolScreen};
use std::{
    cell::{OnceCell, RefCell},
    collections::HashMap,
    path::{Path, PathBuf},
    sync::mpsc,
};

mod areas;
mod input;
mod layout;
mod screens;

const MIN_WIDTH: u16 = 100;
const MIN_HEIGHT: u16 = 25;
const SIDEBAR_WIDTH: u16 = 30;
const HEADER_HEIGHT: u16 = 4;
/// Room kept beside the banner for the status on the right.
const STATUS_WIDTH: u16 = 18;
/// The name beside the emblem, spaced out as on the logo.
const WORDMARK: [&str; 2] = ["H E L I X", "T O O L B O X"];
const TAGLINE: &str = "BUILD • CONFIGURE • OPTIMIZE";
/// Room the name and tagline take beside the emblem.
const WORDMARK_WIDTH: u16 = 30;
const HIGHLIGHT: &str = "> ";
const ROOT_WARNING: &str = "You are running Helix Toolbox as root.\n\nEvery entry then has full access to the system. The entries expect to run as your normal user and ask for your password when they need it.";
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
  Ctrl-K            commands, by name            q / Ctrl-C  quit        Ctrl-R  check installed again
  j / k             move in the sidebar          Enter / l   open the screen or quick action
  Mouse             click to open or select, again for Enter; wheel scrolls; Shift-drag selects text
Areas (Applications and the other catalog screens)
  Enter / l         open a group or run an entry o / x / Del  Profiles: import, export, delete
  h                 parent group, or the sidebar Space       add to or remove from the install plan
  d / e             details / profile into plan  p           preview the script or command
  r / c             remove a package / check it  v           where it comes from (trust)
  u                 update the entry             U           update every entry with an update
  g                 flags guide     *   pin a favorite     i   show all, installed or updates
Install Plan   Enter run it   x / Del remove one   K / J move   c clear   s save as profile   e changes
History, Backups, Doctor, Hardware, Desktop, Updates, Drift, Reliability, Settings
  Enter             log / restore / details / change   r   reload or check again; f repair; u / U update
  Hardware: f       update firmware (type FIRMWARE)    s   SMART check;  Backups: e  side by side
  Desktop: s        save the look; Enter puts a saved one back      Settings: wizard, bug report
  Drift: b          compare with another      s save this machine;  Space / a plan, R run the missing
  Doctor: d details, and Space p v * on its reports   Updates: Enter on an entry updates it
  Reliability: i    critical or untested only    v   trust;  w (and History)  report it worked here
  Services: s start/stop  e enable/disable  R restart  d info  i view  f filter   Storage: Enter cleans
  Performance: b an earlier boot  Enter acts on a hint   Recovery: c check snapshots  f set up
  Migration: e export  o open  a install   Home Manager: b build  s switch  Enter go back  p pull
  Restic: Enter browse  R/F restore  b back up  c check  s history  i case  n/v baseline/test  w/W timer
  Configs: Enter change a field (a draft) or put a backup back  c check  s save  x discard  E editor
  Repositories: Enter open/diff/run  Space / A stage  c commit  b branch  s stash  F p P sync  n task

Running window   Ctrl-C stop, again kill   Enter / q close   l save the log   Page up / down scroll
";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Screen {
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
    /// Offline bundles: made from the Install Plan, installed from a folder.
    Bundles,
    /// systemd's units and the applications started at login.
    Services,
    /// Where the space goes, and what can be cleaned.
    Storage,
    /// The boot's times, the machine now, and this boot beside another.
    Performance,
    /// Whether the machine could be brought back after a bad update.
    Recovery,
    /// What this machine has, to take elsewhere, and a migration brought here.
    Migration,
    /// Restic backups of your own files, and recovery tests.
    Restic,
    /// The repositories under the project folders, and one of them.
    Repositories,
    /// Ghostty's and Fastfetch's configuration, changed and saved as a plan.
    Configs,
    /// Home Manager's generations, building and switching.
    HomeManager,
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
    /// A path or a word, and what it answers.
    Ask(InputPopup, Asked),
    /// A hardware report to read, and the issue it fills in.
    HardwareReport(TextPopup, String),
}

/// What an `Ask` popup's answer is for.
#[derive(Clone, Debug)]
enum Asked {
    /// The profile file to import.
    ImportProfile,
    /// Where to export the profile with this id.
    ExportProfile(String),
    /// "yes", to delete the profile with this id.
    DeleteProfile(String),
    /// The folder to make a bundle in.
    MakeFolder,
    /// The bundle to install from.
    InstallFolder,
    /// What the Services screen's names must have.
    ServiceFilter,
    /// Where to write this machine's migration.
    ExportMigration,
    /// The migration file to read here.
    OpenMigration,
    /// The name or path whose history to look for in the snapshots.
    ResticHistory,
    /// The new folder to restore this snapshot (and only this path) into.
    ResticRestore(String, Option<String>),
    /// The snapshot to restore, and only this path of it, typed.
    ResticRestoreFrom,
    /// The file to record a baseline of.
    ResticBaseline,
    /// The snapshot to test the baseline with this id against.
    ResticTest(String),
    /// The open repository's commit message.
    CommitMessage,
    /// A branch to create in the open repository.
    NewBranch,
    /// The stash's message.
    StashMessage,
    /// A command of the user's to keep as a task of the open repository.
    AddTask,
    /// A new value for this field of this configuration.
    ConfigField(configs::Kind, &'static str),
}

/// A confirmation's header and its steps with their commands.
type Confirming = (Vec<String>, Vec<(String, String)>);

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
    ImportProfile,
    Report,
    UpdateEntries,
    /// Check again how package entries stand.
    CheckStatuses,
    Theme,
    Icons,
    Mouse,
    Help,
    Quit,
}

#[derive(Clone)]
enum Row {
    Up,
    Group {
        index: usize,
        name: String,
        description: String,
        /// How many items it holds, as the window counts them.
        count: usize,
    },
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

/// The screens `--screen` opens on by name; areas go by their id.
const SCREEN_NAMES: [(&str, Screen); 22] = [
    ("dashboard", Screen::Dashboard),
    ("plan", Screen::Plan),
    ("favorites", Screen::Favorites),
    ("history", Screen::History),
    ("backups", Screen::Backups),
    ("doctor", Screen::Doctor),
    ("updates", Screen::Updates),
    ("drift", Screen::Drift),
    ("reliability", Screen::Reliability),
    ("hardware", Screen::Hardware),
    ("desktop", Screen::Desktop),
    ("bundles", Screen::Bundles),
    ("services", Screen::Services),
    ("storage", Screen::Storage),
    ("performance", Screen::Performance),
    ("recovery", Screen::Recovery),
    ("migration", Screen::Migration),
    ("restic", Screen::Restic),
    ("repositories", Screen::Repositories),
    ("configs", Screen::Configs),
    ("home-manager", Screen::HomeManager),
    ("settings", Screen::Settings),
];

/// What a quick action on the dashboard opens.
#[derive(Clone, Copy)]
enum Quick {
    Applications,
    Plan,
    Wizard,
    Palette,
    Doctor,
    History,
    Favorites,
}

const QUICK_ACTIONS: [(Icon, &str, &str, Quick); 7] = [
    (
        Icon::Applications,
        "Browse Applications",
        "Search and install tools",
        Quick::Applications,
    ),
    (
        Icon::Plan,
        "Review the Install Plan",
        "See what is queued and run it",
        Quick::Plan,
    ),
    (
        Icon::Settings,
        "Set up this machine",
        "Choose a workstation to review",
        Quick::Wizard,
    ),
    (
        Icon::Search,
        "Find anything",
        "Any screen, action or entry",
        Quick::Palette,
    ),
    (
        Icon::Doctor,
        "Run System Doctor",
        "Health checks and reports",
        Quick::Doctor,
    ),
    (
        Icon::History,
        "View History",
        "Past runs and their logs",
        Quick::History,
    ),
    (
        Icon::Favorites,
        "Favorites",
        "Pinned and recently run entries",
        Quick::Favorites,
    ),
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
    /// The run session waiting for the user's yes.
    pending: Option<RunSession>,
    popup: Option<Popup>,
    overview: Overview,
    size_bypass: bool,
    skip_confirmation: bool,
    show_hidden: bool,
    /// Nerd Font icons, or plain text.
    icons: bool,
    /// Clicks and the wheel, through the terminal's mouse reporting.
    mouse: bool,
    /// What the last frame drew where, for the mouse.
    hits: RefCell<Hits>,
    /// After a failed step, continue with the steps that do not need it.
    keep_going: bool,
    /// Take a snapshot before system changes, where the machine can.
    snapshots: bool,
    /// The snapshot tool set up on the host, if any, as it was at the start.
    snapshot_tool: Option<backups::Snapshots>,
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
    /// The machine the screens read: this one, or one a test made up.
    host: Host,
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
    /// The check running in the background, if any.
    checking: Read<(Inventory, HashMap<String, Status>)>,
    /// What the palette's choices do, by index.
    palette_acts: Vec<Act>,
    /// What this build knows about how its entries were tested; read once.
    evidence: OnceCell<reliability::Evidence>,
    /// The confirmation's lines, to show it again after the change preview.
    confirming: Option<Confirming>,
    /// Whether the wizard has been offered, remembered in the settings.
    set_up: bool,
    /// The wizard's workstations, by index, while it is open.
    wizard_ids: Vec<String>,
    /// Where the update notifications' timer is kept; none in tests unless
    /// one says.
    notices: Option<notifications::Places>,
    /// Runs `systemctl --user`; tests answer for it.
    systemctl: fn(&[&str]) -> Option<bool>,
    /// A bug report being gathered in the background, if any.
    reporting: Read<String>,
    /// Where the bug report is saved; none in tests unless one says.
    reports_dir: Option<PathBuf>,
    /// Opens a link in the browser; tests answer for it.
    opener: fn(&str) -> Result<(), String>,
    /// Copies a report; tests answer for it.
    copier: fn(&str) -> Result<String, String>,
    /// The tool screens, each with its own state.
    screens: Screens,
}

/// A plan of the queue as it was, from when it was made, with what the
/// checks before a run found.
struct PlanCache {
    ids: Vec<String>,
    made: std::time::Instant,
    result: Result<Plan, String>,
    findings: Vec<preflight::Finding>,
}

/// How long a cached plan is trusted, for changes made outside Helix Toolbox.
const PLAN_KEPT: std::time::Duration = std::time::Duration::from_secs(3);

impl App {
    pub fn new(catalog: Catalog, platform: LinuxInfo, host: Host, args: &TuiArgs) -> Self {
        let menu = build_menu(&catalog);
        let areas = vec![AreaState::default(); catalog.tabs.len()];
        let overview = Overview::gather(&host, &platform);
        let snapshot_tool = host.snapshots();
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
            overview,
            size_bypass: args.size_bypass,
            skip_confirmation: args.skip_confirmation,
            show_hidden: args.override_validation,
            icons: !args.plain,
            mouse: true,
            hits: RefCell::new(Hits::default()),
            keep_going: false,
            snapshots: true,
            snapshot_tool,
            profiles_dir: None,
            favorites: Vec::new(),
            favorites_path: None,
            recent: Vec::new(),
            filter: Filter::All,
            before_search: None,
            host,
            ran_from_plan: false,
            settings_path: None,
            banner: None,
            planned: None,
            statuses: HashMap::new(),
            inventory: None,
            checking: Read::default(),
            palette_acts: Vec::new(),
            evidence: OnceCell::new(),
            confirming: None,
            set_up: true,
            wizard_ids: Vec::new(),
            notices: None,
            systemctl: |_| None,
            reporting: Read::default(),
            reports_dir: None,
            opener: |_| Err("there is no browser here".to_string()),
            copier: |_| Err("there is no clipboard here".to_string()),
            screens: Screens::default(),
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
        self.mouse = look.mouse;
        self.keep_going = look.keep_going;
        if let Some(path) = &path {
            let favorites = settings::favorites_path(path);
            self.favorites = settings::load_favorites(&favorites);
            self.favorites_path = Some(favorites);
        }
        self.snapshots = look.snapshots;
        self.set_up = look.set_up || path.is_none();
        self.settings_path = path;
        self.notices = notifications::Places::here();
        self.systemctl = notifications::systemctl;
        self.reports_dir = self.host.state().map(Path::to_path_buf);
        self.opener = open_link;
        self.copier = super::clipboard::copy;
        self
    }

    /// Check in the background how package entries stand, now; runs check
    /// again after they end.
    pub fn with_status_checks(mut self) -> Self {
        self.refresh_statuses();
        self
    }

    /// Start a fresh status check; the old answers stay shown until it ends.
    fn refresh_statuses(&mut self) {
        let entries: Vec<Entry> = self
            .catalog
            .entries()
            .into_iter()
            .filter(|entry| entry.identity().is_some())
            .cloned()
            .collect();
        let platform = self.platform.clone();
        self.checking.start(&self.host, move |host| {
            let inventory = Inventory::gather(&host, &platform);
            let statuses = entries
                .iter()
                .filter_map(|entry| {
                    inventory
                        .status(entry, &platform, &|tool| host.which(tool))
                        .map(|status| (entry.id.clone(), status))
                })
                .collect();
            (inventory, statuses)
        });
    }

    /// Draw this picture as the header where it fits.
    pub fn with_banner(mut self, banner: Option<Protocol>) -> Self {
        self.banner = banner;
        self
    }

    /// The banner, unless icons are off, a theme with few or no colors is in use,
    /// or the terminal is too narrow for it, the name and the status beside it.
    fn banner(&self, width: u16) -> Option<&Protocol> {
        self.banner.as_ref().filter(|banner| {
            self.icons
                && self.theme.colorful()
                && banner.size().width + 4 + WORDMARK_WIDTH + STATUS_WIDTH <= width
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
            mouse: self.mouse,
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
                    let score = helix_toolbox_core::search::score(
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
                        description: group.description.clone(),
                        count: group.items.len(),
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
            Screen::Settings => self.settings().len(),
            screen => self
                .screens
                .tool(screen)
                .map_or(0, |tool| tool.len(&self.view())),
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
        if let Some(tool) = self.screens.tool(self.screen) {
            return tool.entry(&self.view(), self.main_state.selected());
        }
        match self.selected_row() {
            Some(Row::Entry(entry)) => Some(entry.as_ref()),
            _ => None,
        }
    }

    /// The screen `--screen` names: a fixed screen's name, or an area's id.
    pub(super) fn screen_named(&self, name: &str) -> Option<Screen> {
        let name = name.trim().to_lowercase();
        SCREEN_NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, screen)| *screen)
            .or_else(|| {
                let index = self.catalog.tabs.iter().position(|tab| tab.id == name)?;
                // Only an area the menu shows.
                self.menu
                    .iter()
                    .any(|item| item.screen == Screen::Area(index))
                    .then_some(Screen::Area(index))
            })
    }

    /// The names `--screen` takes, for its error.
    pub(super) fn screen_names(&self) -> String {
        let mut names: Vec<&str> = SCREEN_NAMES.iter().map(|(name, _)| *name).collect();
        names.extend(self.menu.iter().filter_map(|item| match item.screen {
            Screen::Area(index) => Some(self.catalog.tabs[index].id.as_str()),
            _ => None,
        }));
        names.join(", ")
    }

    /// Open on the screen `--screen` named, with the focus on it.
    pub(super) fn open_on(&mut self, screen: Screen) {
        self.open_screen(screen);
        if screen != Screen::Dashboard {
            self.focus = Focus::Main;
        }
    }

    /// Where an entry is listed: its area, and the group that holds it.
    fn place(&self, id: &str) -> Option<(String, Option<String>)> {
        fn walk(items: &[Item], id: &str, group: Option<&str>) -> Option<Option<String>> {
            items.iter().find_map(|item| match item {
                Item::Entry(entry) if entry.id == id => Some(group.map(str::to_string)),
                Item::Group(inner) => walk(&inner.items, id, Some(&inner.name)),
                Item::Entry(_) => None,
            })
        }
        self.catalog
            .tabs
            .iter()
            .find_map(|tab| walk(&tab.items, id, None).map(|group| (tab.name.clone(), group)))
    }

    /// "Area › group", as the window writes where an entry is.
    fn place_label(&self, id: &str) -> Option<String> {
        self.place(id).map(|(area, group)| match group {
            Some(group) => format!("{area} › {group}"),
            None => area,
        })
    }

    /// Package entries found installed (or kept by Nix), and those with an
    /// update waiting, from the last check.
    fn installed_count(&self) -> usize {
        self.statuses
            .values()
            .filter(|status| {
                matches!(
                    status,
                    Status::Installed { .. } | Status::Elsewhere { .. } | Status::Managed { .. }
                )
            })
            .count()
    }

    fn update_count(&self) -> usize {
        self.statuses
            .values()
            .filter(|status| matches!(status, Status::Update { .. }))
            .count()
    }

    /// What the footer says of the check of package entries, once one has
    /// started or answered.
    fn check_line(&self) -> Option<String> {
        if self.checking.busy() {
            Some("Checking what is installed…".to_string())
        } else if self.inventory.is_some() || !self.statuses.is_empty() {
            Some(format!(
                "{} package entries checked · Ctrl-R check again",
                self.statuses.len()
            ))
        } else {
            None
        }
    }

    /// Check again how package entries stand, unless a check is running.
    fn check_again(&mut self) {
        if !self.checking.busy() {
            self.planned = None;
            self.refresh_statuses();
        }
    }

    /// The count the sidebar shows beside a screen, when there is one.
    fn menu_count(&self, screen: Screen) -> usize {
        match screen {
            Screen::Favorites => self.favorites.len(),
            Screen::Plan => self.queue.len(),
            Screen::Updates => self.update_count(),
            _ => 0,
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
        if screen == Screen::Favorites {
            self.reload_recent();
        }
        self.with_tool(|tool, cx| tool.open(cx));
        self.update_rows();
    }

    /// Whether something changed without a key press: a finished check, or
    /// a run's events.
    pub fn needs_redraw(&mut self) -> bool {
        // One background read is taken in a pass, in this order.
        if self.with_screens(|screens, cx| screens.bundles.poll(cx)) {
            return true;
        }
        if let Some(report) = self.reporting.take() {
            self.show_report(&report);
            return true;
        }
        if self.with_screens(|screens, cx| {
            screens.doctor.poll(cx)
                || screens.performance.poll(cx)
                || screens.migration.poll(cx)
                || screens.home.poll(cx)
                || screens.restic.poll(cx)
                || screens.repos.poll(cx)
                || screens.storage.poll(cx)
                || screens.hardware.poll(cx)
                || screens.updates.poll(cx)
        }) {
            return true;
        }
        if let Some((inventory, statuses)) = self.checking.take() {
            self.inventory = Some(inventory);
            self.statuses = statuses;
            if self.screen == Screen::Drift {
                self.with_screens(|screens, cx| screens.drift.check(cx));
            }
            return true;
        }
        match &mut self.popup {
            Some(Popup::Running(running)) => running.needs_redraw(),
            _ => false,
        }
    }

    /// Review a change to the open repository.
    fn plan_repo_change(&mut self, change: Change) {
        self.with_screens(|screens, cx| screens.repos.plan_change(cx, change));
    }

    /// Do what an `Ask` popup was answered for.
    fn answered(&mut self, asked: Asked, text: &str) {
        let (title, said) = match asked {
            Asked::ImportProfile => ("Profile imported", self.import_profile(text)),
            Asked::ExportProfile(id) => ("Profile exported", self.export_profile(&id, text)),
            Asked::DeleteProfile(id) if text.trim().eq_ignore_ascii_case("yes") => {
                ("Profile deleted", self.delete_profile(&id))
            }
            Asked::DeleteProfile(_) => (
                "Not deleted",
                Err("Deleting needs the word yes. The profile is kept.".to_string()),
            ),
            Asked::MakeFolder => {
                self.screens.bundles.set_folder(text);
                return;
            }
            Asked::InstallFolder => {
                self.screens.bundles.set_source(text);
                return;
            }
            Asked::ServiceFilter => {
                self.with_screens(|screens, cx| screens.services.set_filter(cx, text));
                return;
            }
            Asked::CommitMessage => {
                self.plan_repo_change(Change::Commit(text.to_string()));
                return;
            }
            Asked::ConfigField(kind, key) => {
                self.with_screens(|screens, cx| screens.configs.set_config(cx, kind, key, text));
                return;
            }
            Asked::NewBranch => {
                self.plan_repo_change(Change::CreateBranch(text.trim().to_string()));
                return;
            }
            Asked::StashMessage => {
                self.plan_repo_change(Change::Stash {
                    message: text.to_string(),
                    untracked: true,
                });
                return;
            }
            Asked::AddTask => (
                "Task kept",
                self.with_screens(|screens, cx| screens.repos.add_task(cx, text)),
            ),
            Asked::ExportMigration => (
                "Migration exported",
                self.with_screens(|screens, cx| screens.migration.export(cx, text)),
            ),
            Asked::ResticHistory => {
                let pattern = text.trim().to_string();
                self.with_screens(|screens, cx| screens.restic.search(cx, pattern));
                return;
            }
            Asked::ResticRestore(snapshot, include) => {
                self.with_screens(|screens, cx| {
                    screens
                        .restic
                        .restore(cx, &snapshot, include.as_deref(), text)
                });
                return;
            }
            Asked::ResticRestoreFrom => {
                // The snapshot, then what of it, as the window's two fields.
                let text = text.trim();
                let (snapshot, include) = match text.split_once(char::is_whitespace) {
                    Some((snapshot, include)) => (snapshot, Some(include.trim().to_string())),
                    None => (text, None),
                };
                self.with_screens(|screens, cx| {
                    screens
                        .restic
                        .ask_restore_into(cx, snapshot.to_string(), include)
                });
                return;
            }
            Asked::ResticBaseline => (
                "Baseline recorded",
                self.with_screens(|screens, cx| screens.restic.record_baseline(cx, text)),
            ),
            Asked::ResticTest(baseline) => {
                self.with_screens(|screens, cx| screens.restic.test(cx, &baseline, text.trim()));
                return;
            }
            Asked::OpenMigration => {
                match typed_path(text) {
                    Ok(path) => {
                        self.with_screens(|screens, cx| screens.migration.open_file(cx, path))
                    }
                    Err(error) => {
                        self.popup =
                            Some(Popup::Text(TextPopup::new("It did not work", error, true)))
                    }
                }
                return;
            }
        };
        self.popup = Some(Popup::Text(match said {
            Ok(text) => TextPopup::new(title, text, true),
            Err(error) => TextPopup::new(
                if title == "Not deleted" {
                    title
                } else {
                    "It did not work"
                },
                error,
                true,
            ),
        }));
    }

    /// Plan entries; ask before running what can run.
    fn confirm(&mut self, entries: Vec<Entry>) {
        if entries.is_empty() {
            return;
        }
        let planned = plan(entries.iter(), &self.platform, &|id| self.catalog.find(id));
        self.confirm_plan(planned);
    }

    /// What the checks before a run find for a plan on this machine.
    fn findings(&self, planned: &Plan) -> Vec<preflight::Finding> {
        self.with_facts(|facts| {
            preflight::check(planned, &self.platform, &|id| self.catalog.find(id), facts)
        })
    }

    /// Call `then` with what the checks before a run ask the machine.
    fn with_facts<R>(&self, then: impl FnOnce(&preflight::Facts) -> R) -> R {
        if !self.host.is_here() {
            // The checks before a run ask this machine itself; a made-up
            // one answers no more than planning asked.
            return then(&preflight::Facts {
                present: &|_| true,
                free_bytes: &|_| None,
                gpus: &Vec::new,
                repositories: &|| None,
                sizes: &|_, _| None,
                active_runs: &Vec::new,
                ..preflight::Facts::real()
            });
        }
        then(&preflight::Facts::real())
    }

    /// Plan updating entries from the last status check, and ask.
    fn update(&mut self, entries: Vec<Entry>) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "Helix Toolbox is still checking what is installed; try again in a moment.",
                true,
            )));
            return;
        };
        let planned = inventory.plan_update(entries.iter(), &self.platform, &which);
        self.confirm_plan(Ok(planned));
    }

    /// Show a plan's confirmation, or why there is nothing to run.
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
        let prepared = self.prepare(planned);
        let (header, lines) = match prepared.verdict {
            Verdict::Nothing(lines) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Nothing to run",
                    lines.join("\n"),
                    true,
                )));
                return;
            }
            Verdict::Blocked(lines) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot run yet",
                    format!("{}\n\nNothing was changed.", lines.join("\n")),
                    true,
                )));
                return;
            }
            Verdict::Ready { header, lines } => (header, lines),
        };
        let lines: Vec<(String, String)> = lines
            .into_iter()
            .map(|line| (line.name, line.detail))
            .collect();
        self.pending = Some(RunSession::new(prepared.plan));
        if self.skip_confirmation {
            self.run_pending();
            return;
        }
        self.popup = Some(Popup::Confirm(ConfirmPopup::new(&header, &lines)));
        self.confirming = Some((header, lines));
    }

    /// Get a plan ready to confirm, as every interface does, with the run
    /// choices in Settings.
    fn prepare(&mut self, planned: Plan) -> Prepared {
        let choices = helix_toolbox_engine::settings::RunChoices {
            keep_going: self.keep_going,
            snapshots: self.snapshots,
        };
        if self.host.is_here() {
            return confirmation::prepare(planned, &self.catalog, &self.platform, choices);
        }
        // A made-up host: its snapshot tool, no machine questions beyond
        // what planning asked.
        let evidence = self.evidence();
        let choices = confirmation::Choices {
            snapshot: if self.snapshots {
                confirmation::Snapshot::Wanted(self.snapshot_tool)
            } else {
                confirmation::Snapshot::Off
            },
            keep_going: self.keep_going,
        };
        self.with_facts(|facts| {
            confirmation::prepare_with(
                planned,
                &self.platform,
                &|id| self.catalog.find(id),
                choices,
                facts,
                &evidence,
            )
        })
    }

    /// Read again what a run left out of date, where this screen shows it.
    fn read_again(&mut self, reading: Reading) {
        if reading == Reading::Statuses {
            self.refresh_statuses();
        } else {
            self.with_tool(|tool, cx| tool.after_run(cx, reading));
        }
    }

    fn run_pending(&mut self) {
        let Some(session) = self.pending.take() else {
            return;
        };
        let this =
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("helix-toolbox"));
        self.popup = Some(
            match RunningView::start(session, &self.catalog.lib_dir(), &this) {
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

/// Open a link in the desktop's browser, without waiting for it.
fn open_link(url: &str) -> Result<(), String> {
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|mut child| {
            // Reaped when it exits, so that it leaves nothing behind.
            std::thread::spawn(move || child.wait());
        })
        .map_err(|error| format!("xdg-open could not run: {error}"))
}

/// One of the user's own profiles, which can be exported and deleted.
fn own_profile(entry: &Entry) -> bool {
    entry.id.starts_with(profiles::PREFIX) && matches!(entry.action, Action::Profile { .. })
}

/// A path as the user typed it: `~` is the home folder; anything else must
/// be a full path.
fn typed_path(text: &str) -> Result<PathBuf, String> {
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
        return Err(format!(
            "Give the full path, starting with / or ~/, not {}.",
            helix_toolbox_core::printable(text)
        ));
    }
    Ok(path)
}

/// Plan entries on this machine. The tests' fixture packages are in no
/// repository, so there the repositories are not asked.
fn plan<'a>(
    entries: impl IntoIterator<Item = &'a Entry>,
    platform: &LinuxInfo,
    lookup: &dyn Fn(&str) -> Option<&'a Entry>,
) -> Result<Plan, EngineError> {
    #[cfg(not(test))]
    let machine = Machine::real();
    #[cfg(test)]
    let machine = Machine {
        known: &|_, _| None,
        ..Machine::real()
    };
    plan_with(entries, platform, lookup, machine)
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

/// What kind of entry it is, in the window's words.
fn kind_label(action: &Action) -> &'static str {
    match action {
        Action::Package { .. } => "Package",
        Action::Script { .. } => "Script",
        Action::Command { .. } => "Command",
        Action::Nix { .. } => "Nix",
        Action::Native { .. } => "Built in",
        Action::Profile { .. } => "Profile",
    }
}

/// Each word of a flags field and what it means, as the flags guide says;
/// P before a known flag makes it privileged.
fn flag_meanings(field: &str) -> Vec<(String, String)> {
    const MEANINGS: [(&str, &str); 9] = [
        ("D", "disk changes, such as partitioning (privileged)"),
        ("FI", "Flatpak installation"),
        ("FM", "file changes"),
        ("I", "installation (privileged)"),
        ("K", "kernel changes (privileged)"),
        ("MP", "package manager actions"),
        ("SI", "full system installation"),
        ("SS", "service actions (privileged)"),
        ("RP", "package removal"),
    ];
    let meaning = |flag: &str| {
        MEANINGS
            .iter()
            .find(|(known, _)| *known == flag)
            .map(|(_, meaning)| *meaning)
    };
    field
        .split_whitespace()
        .map(|flag| {
            let privileged = flag
                .strip_prefix('P')
                .filter(|base| !base.is_empty())
                .and_then(meaning);
            let said = match (privileged, meaning(flag)) {
                (Some(base), _) => format!("{base}, privileged"),
                (None, Some(known)) => known.to_string(),
                (None, None) => "unknown flag".to_string(),
            };
            (flag.to_string(), said)
        })
        .collect()
}

/// The text cut to `width` characters, ending in "…" when it was cut.
fn clipped(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let mut cut: String = text.chars().take(width.saturating_sub(1)).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('…');
    cut
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
        label: "Services".into(),
        screen: Screen::Services,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Storage".into(),
        screen: Screen::Storage,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Performance".into(),
        screen: Screen::Performance,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Recovery".into(),
        screen: Screen::Recovery,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Migration".into(),
        screen: Screen::Migration,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Restic".into(),
        screen: Screen::Restic,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Repositories".into(),
        screen: Screen::Repositories,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Home Manager".into(),
        screen: Screen::HomeManager,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Configs".into(),
        screen: Screen::Configs,
        section: "MORE TOOLS",
    });
    menu.push(MenuItem {
        label: "Offline bundles".into(),
        screen: Screen::Bundles,
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

/// `text` cut to `width` columns at a word, with an ellipsis when it is cut.
fn clip(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        return text.to_string();
    }
    let room = width.saturating_sub(1);
    let cut: String = text.chars().take(room).collect();
    let at_word = match cut.rfind(' ') {
        Some(space) if space > room / 2 => cut[..space].trim_end().to_string(),
        _ => cut,
    };
    if width == 0 {
        String::new()
    } else {
        format!("{at_word}…")
    }
}

/// Draw a list in a card with the main selection, and note where its
/// items are, inside the border, for the mouse.
fn bordered_list(
    frame: &mut Frame,
    list: List,
    area: Rect,
    state: &mut ListState,
    hits: &RefCell<Hits>,
) {
    frame.render_stateful_widget(list, area, state);
    hits.borrow_mut()
        .list(Block::bordered().inner(area), state.offset());
}
#[cfg(test)]
mod tests;
