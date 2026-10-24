//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the Helix Toolbox dashboard design.
//!
//! [`App`] holds the state; this file has its startup, the shared state
//! helpers and the confirm-and-run path. The frame is in `layout`, keys and
//! the mouse in `input`, the catalog areas, search, Favorites, the Install
//! Plan and the command palette in `areas`, and each other screen, the
//! profiles, the reports and the setup wizard add their methods from a file
//! in `screens`.

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
    EngineError, Machine, Plan, Probe, backups, bundle,
    configs::{self, Checked},
    confirmation::{self, Prepared},
    doctor, drift, hardware_report, history, home_manager,
    inventory::{Inventory, Status, which},
    migration::{self, Standing, Translation},
    notifications,
    overview::Overview,
    performance, plan_removal, plan_with, preflight, profiles, recovery, reliability,
    repositories::{self as repos, Change, Details, Summary},
    restic::{self, Restic},
    services::{self, Verb},
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
use std::{
    cell::RefCell,
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

/// Which units the Services screen lists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ServiceView {
    System,
    Timers,
    User,
    Login,
    Failed,
}

impl ServiceView {
    fn next(self) -> Self {
        match self {
            ServiceView::System => ServiceView::Timers,
            ServiceView::Timers => ServiceView::User,
            ServiceView::User => ServiceView::Login,
            ServiceView::Login => ServiceView::Failed,
            ServiceView::Failed => ServiceView::System,
        }
    }

    fn label(self) -> &'static str {
        match self {
            ServiceView::System => "the machine's services",
            ServiceView::Timers => "timers and sockets",
            ServiceView::User => "your session's",
            ServiceView::Login => "started at login",
            ServiceView::Failed => "failed",
        }
    }
}

/// The selected line of the Updates screen.
#[derive(Clone, Debug)]
enum UpdateRow {
    Nothing,
    Channel,
    /// A package entry with an update.
    Entry(Box<Entry>),
    /// The line that updates every entry with an update.
    Every,
}

/// A line of the Services screen.
#[derive(Clone, Debug)]
enum ServiceRow {
    Heading(String),
    Unit(usize),
    Autostart(usize),
}

/// A line of the Storage screen.
#[derive(Clone, Debug)]
enum StorageRow {
    Heading(String),
    Filesystem(String),
    Item(usize),
    Kept(usize),
}

/// A line of the Performance screen.
#[derive(Clone, Debug)]
enum PerformanceRow {
    Heading(String),
    Text(String),
    /// A change from the earlier boot, and whether it is slower.
    Change(String, bool),
    /// A unit and how long it took.
    Unit(String, String),
    Hint(usize),
}

/// A reading of the performance, with the boots recorded so far.
type PerformanceReading = (performance::Performance, Vec<performance::Boot>);

/// A line of the Migration screen.
#[derive(Clone, Debug)]
enum MigrationRow {
    Heading(String),
    Text(String),
    /// A thing the opened migration names, by its place in the translation.
    Item(usize),
}

/// A line of the Configs screen.
#[derive(Clone, Debug)]
enum ConfigRow {
    Heading(String),
    Text(String),
    /// A field of a configuration, by its place in the configuration's fields.
    Field(configs::Kind, usize),
    /// The whole file, in the user's editor.
    Edit(configs::Kind),
    /// A backup kept beside the file, by its place in its history.
    Backup(configs::Kind, usize),
}

/// A configuration as read: the file to edit, its text, what its checks
/// say and the backups kept beside it.
struct ConfigPart {
    kind: configs::Kind,
    found: Result<configs::Located, String>,
    loaded: Option<Result<configs::Loaded, String>>,
    checked: Option<Checked>,
    history: Vec<backups::Backup>,
    /// Changes not saved yet, as the window's draft: fields changed one by
    /// one, then checked and saved once.
    draft: Option<String>,
    /// What the checks said of the draft, once asked.
    draft_checked: Option<Checked>,
}

/// A private copy of a configuration for the user's editor, which the
/// event loop opens with the terminal handed back.
pub struct EditRequest {
    pub copy: PathBuf,
    pub editor: String,
    located: configs::Located,
    loaded: configs::Loaded,
}

/// A migration file read here, or why it could not be.
type MigrationOpened = (PathBuf, Result<Translation, String>);

/// A line of the Home Manager screen.
#[derive(Clone, Debug)]
enum HomeRow {
    Heading(String),
    Text(String),
    /// A generation, by its place in the status.
    Generation(usize),
    /// A linked configuration file, by its place in the status.
    Link(usize),
}

/// A line of the Restic screen.
#[derive(Clone, Debug)]
enum ResticRow {
    Heading(String),
    Text(String),
    Snapshot(usize),
    /// The browsed folder's parent.
    Up,
    Node(usize),
    /// A version of a file the search found: the file, then the version.
    Version(usize, usize),
    Baseline(usize),
}

/// The repository as last read, and what restic said of it.
#[derive(Clone, Debug)]
struct ResticReading {
    found: restic::Found,
    snapshots: Result<Vec<restic::Snapshot>, String>,
    /// Whether this restic restores without overwriting (0.17 and later).
    restores: bool,
    timer: Option<restic::Every>,
    baselines: Vec<restic::Baseline>,
}

/// A folder of a snapshot, and what it holds.
#[derive(Clone, Debug)]
struct ResticBrowse {
    snapshot: restic::Snapshot,
    folder: String,
    nodes: Result<Vec<restic::Node>, String>,
}

/// A search for a file's history, and what it found.
type ResticSearch = (String, Result<Vec<restic::History>, String>);
/// A line of the Repositories screen: in the list, a repository; in one
/// repository, a file, branch, stash, commit or task, by its place.
#[derive(Clone, Debug)]
enum RepoRow {
    Heading(String),
    Text(String),
    Repository(usize),
    File(usize),
    Branch(usize),
    Stash(usize),
    Commit(usize),
    Task(usize),
}

/// One repository read, with its tasks, or why it could not be.
type RepoOpened = Result<(Details, Vec<Task>), String>;

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

/// A line of the Offline bundles screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BundleRow {
    MakeHeading,
    Entries,
    Folder,
    Complete,
    Contents,
    Make,
    InstallHeading,
    Source,
    Trust,
    Check,
    Install,
}

const BUNDLE_ROWS: [BundleRow; 11] = [
    BundleRow::MakeHeading,
    BundleRow::Entries,
    BundleRow::Folder,
    BundleRow::Complete,
    BundleRow::Contents,
    BundleRow::Make,
    BundleRow::InstallHeading,
    BundleRow::Source,
    BundleRow::Trust,
    BundleRow::Check,
    BundleRow::Install,
];

/// Bundle work that asks the package manager or reads every file: done on
/// the tick after its notice is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BundleTask {
    Contents,
    Check,
    Install,
}

/// A line of the Desktop screen.
#[derive(Clone, Debug)]
enum DesktopRow {
    Heading(String),
    Info {
        text: String,
        details: String,
    },
    /// A suggested entry, with what is worth knowing about it here.
    Entry {
        id: String,
        text: String,
        note: Option<String>,
    },
    Look {
        slug: String,
        text: String,
    },
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
    pending: Option<Plan>,
    popup: Option<Popup>,
    records: Vec<history::Record>,
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
    hardware: Option<helix_toolbox_engine::hardware::Hardware>,
    hardware_rows: Vec<HardwareRow>,
    /// A look at the hardware running in the background, if any.
    reading_hardware: Option<mpsc::Receiver<helix_toolbox_engine::hardware::Hardware>>,
    /// The firmware update waiting for its confirming word.
    firmware_pending: Option<Plan>,
    /// The desktop as last read, and the Desktop screen's rows.
    desktop: Option<helix_toolbox_engine::desktop::Desktop>,
    desktop_rows: Vec<DesktopRow>,
    /// Where looks are saved; the user's configuration unless a test says.
    looks_dir: Option<std::path::PathBuf>,
    /// Whether the wizard has been offered, remembered in the settings.
    set_up: bool,
    /// The wizard's workstations, by index, while it is open.
    wizard_ids: Vec<String>,
    /// Where the update notifications' timer is kept; none in tests unless
    /// one says.
    notices: Option<notifications::Places>,
    /// Runs `systemctl --user`; tests answer for it.
    systemctl: fn(&[&str]) -> Option<bool>,
    /// The folder to make a bundle in, as typed.
    bundle_folder: String,
    /// Every dependency, not only what this machine lacks.
    bundle_complete: bool,
    /// The bundle to install from, as typed.
    bundle_source: String,
    /// Take the bundle's word for what it holds.
    bundle_trust: bool,
    /// Bundle work waiting for its notice to be drawn.
    bundle_due: Option<BundleTask>,
    /// A bug report being gathered in the background, if any.
    reporting: Option<mpsc::Receiver<String>>,
    /// Where the bug report is saved; none in tests unless one says.
    reports_dir: Option<PathBuf>,
    /// Opens a link in the browser; tests answer for it.
    opener: fn(&str) -> Result<(), String>,
    /// Copies a report; tests answer for it.
    copier: fn(&str) -> Result<String, String>,
    /// The machine a hardware report names, in tests; read each time
    /// otherwise.
    machine: Option<hardware_report::Machine>,
    /// The units and autostart entries, as last read.
    services_found: Option<services::Services>,
    service_rows: Vec<ServiceRow>,
    service_view: ServiceView,
    service_filter: String,
    /// The storage, as last read, and its rows.
    storage_found: Option<storage::Storage>,
    storage_rows: Vec<StorageRow>,
    /// A look at the storage running in the background, if any.
    reading_storage: Option<mpsc::Receiver<storage::Storage>>,
    /// The performance as last read, the boots recorded, and which earlier
    /// boot this one is set beside (0 is the one before).
    performance_found: Option<PerformanceReading>,
    performance_against: usize,
    performance_rows: Vec<PerformanceRow>,
    reading_performance: Option<mpsc::Receiver<PerformanceReading>>,
    /// Recovery readiness, as last checked.
    recovery_found: Vec<doctor::Diagnosis>,
    /// What a migration of this machine would hold, as last read.
    migration_here: Option<migration::Migration>,
    reading_migration: Option<mpsc::Receiver<(migration::Migration, bool)>>,
    /// Whether the packages installed by hand can be told apart here.
    migration_packages_known: bool,
    /// The migration file opened, and what it means here.
    migration_opened: Option<(PathBuf, Translation)>,
    opening_migration: Option<mpsc::Receiver<MigrationOpened>>,
    migration_rows: Vec<MigrationRow>,
    /// Home Manager as last read, and the screen's lines.
    home_status: Option<home_manager::Status>,
    reading_home: Option<mpsc::Receiver<home_manager::Status>>,
    home_rows: Vec<HomeRow>,
    /// The restic repository as last read, and the screen's lines.
    restic_reading: Option<ResticReading>,
    reading_restic: Option<mpsc::Receiver<ResticReading>>,
    restic_browse: Option<ResticBrowse>,
    browsing_restic: Option<mpsc::Receiver<ResticBrowse>>,
    restic_search: Option<ResticSearch>,
    /// Whether a file's history is looked for ignoring upper and lower case.
    restic_ignore_case: bool,
    searching_restic: Option<mpsc::Receiver<ResticSearch>>,
    restic_rows: Vec<ResticRow>,
    /// What runs restic: restic itself, or a test's stand-in.
    restic: std::sync::Arc<dyn Restic + Send + Sync>,
    /// Where repositories are looked for; set by tests, else from the
    /// settings.
    repo_places: Option<repos::Places>,
    /// The user's workspaces file: beside the settings, or set by tests.
    workspaces_file: Option<PathBuf>,
    repos_found: Option<(repos::Places, Vec<Summary>)>,
    reading_repos: Option<mpsc::Receiver<(repos::Places, Vec<Summary>)>>,
    /// The repository open, with its tasks.
    repo_open: Option<(Details, Vec<Task>)>,
    opening_repo: Option<mpsc::Receiver<RepoOpened>>,
    repo_rows: Vec<RepoRow>,
    config_parts: Vec<ConfigPart>,
    config_rows: Vec<ConfigRow>,
    /// The home and configuration folders configurations are found in, for
    /// tests; this machine's otherwise.
    config_places: Option<(PathBuf, PathBuf)>,
    /// A configuration waiting to be opened in the user's editor.
    edit_request: Option<EditRequest>,
    /// The private folder for copies of configurations, for tests.
    config_folder: Option<PathBuf>,
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
            mouse: true,
            hits: RefCell::new(Hits::default()),
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
            notices: None,
            systemctl: |_| None,
            bundle_folder: "~/helix-toolbox-bundle".to_string(),
            bundle_complete: false,
            bundle_source: String::new(),
            bundle_trust: false,
            bundle_due: None,
            reporting: None,
            reports_dir: None,
            opener: |_| Err("there is no browser here".to_string()),
            copier: |_| Err("there is no clipboard here".to_string()),
            machine: None,
            services_found: None,
            service_rows: Vec::new(),
            service_view: ServiceView::System,
            service_filter: String::new(),
            storage_found: None,
            storage_rows: Vec::new(),
            reading_storage: None,
            performance_found: None,
            performance_against: 0,
            performance_rows: Vec::new(),
            reading_performance: None,
            recovery_found: Vec::new(),
            migration_here: None,
            reading_migration: None,
            migration_packages_known: true,
            migration_opened: None,
            opening_migration: None,
            migration_rows: Vec::new(),
            home_status: None,
            reading_home: None,
            home_rows: Vec::new(),
            restic_reading: None,
            reading_restic: None,
            restic_browse: None,
            browsing_restic: None,
            restic_search: None,
            restic_ignore_case: false,
            searching_restic: None,
            restic_rows: Vec::new(),
            restic: std::sync::Arc::new(restic::Here),
            repo_places: None,
            workspaces_file: None,
            repos_found: None,
            reading_repos: None,
            repo_open: None,
            opening_repo: None,
            repo_rows: Vec::new(),
            config_parts: Vec::new(),
            config_rows: Vec::new(),
            config_places: None,
            edit_request: None,
            config_folder: None,
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
        self.snapshot_tool = backups::Snapshots::here();
        self.settings_path = path;
        self.notices = notifications::Places::here();
        self.systemctl = notifications::systemctl;
        self.reports_dir = history::state_dir();
        self.opener = open_link;
        self.copier = super::clipboard::copy;
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
            let inventory = Inventory::gather(&doctor::Here, &platform);
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
            Screen::History => self.records.len(),
            Screen::Backups => self.backups.len(),
            Screen::Updates => {
                let waiting = self.entries_with_updates().len();
                self.channels.len() + if waiting > 0 { waiting + 1 } else { 0 }
            }
            Screen::Drift => self
                .drift_report
                .as_ref()
                .map_or(0, |(_, report)| report.differences.len()),
            Screen::Reliability => self.reliability_rows.len(),
            Screen::Hardware => self.hardware_rows.len(),
            Screen::Desktop => self.desktop_rows.len(),
            Screen::Bundles => BUNDLE_ROWS.len(),
            Screen::Services => self.service_rows.len(),
            Screen::Storage => self.storage_rows.len(),
            Screen::Performance => self.performance_rows.len(),
            Screen::Recovery => self.recovery_found.len(),
            Screen::Migration => self.migration_rows.len(),
            Screen::Restic => self.restic_rows.len(),
            Screen::Repositories => self.repo_rows.len(),
            Screen::Configs => self.config_rows.len(),
            Screen::HomeManager => self.home_rows.len(),
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
        // The Updates screen's entries come after its channels.
        if self.screen == Screen::Updates {
            let index = self
                .main_state
                .selected()?
                .checked_sub(self.channels.len())?;
            return self.entries_with_updates().into_iter().nth(index);
        }
        // The Doctor's report rows come after its checks.
        if self.screen == Screen::Doctor {
            let index = self
                .main_state
                .selected()?
                .checked_sub(self.diagnoses.len())?;
            return self
                .catalog
                .tabs
                .iter()
                .find(|tab| tab.id == "doctor")
                .and_then(|tab| tab.entries().into_iter().nth(index));
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
        if self.checking.is_some() {
            Some("Checking what is installed…".to_string())
        } else if self.check_statuses || !self.statuses.is_empty() {
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
        if self.checking.is_none() {
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
        if screen == Screen::Services {
            self.reload_services();
        }
        if screen == Screen::Storage {
            if self.storage_found.is_none() {
                self.read_storage();
            } else {
                self.build_storage_rows();
            }
        }
        if screen == Screen::Performance {
            if self.performance_found.is_none() {
                self.read_performance();
            } else {
                self.build_performance_rows();
            }
        }
        if screen == Screen::Recovery {
            self.reload_recovery();
        }
        if screen == Screen::Migration {
            if self.migration_here.is_none() {
                self.read_migration();
            } else {
                self.build_migration_rows();
            }
        }
        if screen == Screen::HomeManager {
            if self.home_status.is_none() {
                self.read_home();
            } else {
                self.build_home_rows();
            }
        }
        if screen == Screen::Restic {
            if self.restic_reading.is_none() {
                self.read_restic();
            } else {
                self.build_restic_rows();
            }
        }
        if screen == Screen::Configs {
            self.read_configs();
        }
        if screen == Screen::Repositories {
            if self.repos_found.is_none() {
                self.read_repos();
            } else {
                self.build_repo_rows();
            }
        }
        self.update_rows();
    }

    /// Whether something changed without a key press: a finished check, or
    /// a run's events.
    pub fn needs_redraw(&mut self) -> bool {
        if let Some(task) = self.bundle_due.take() {
            self.do_bundle_task(task);
            return true;
        }
        if let Some(receiver) = &self.reporting {
            match receiver.try_recv() {
                Ok(report) => {
                    self.reporting = None;
                    self.show_report(&report);
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reporting = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
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
        if let Some(receiver) = &self.reading_performance {
            match receiver.try_recv() {
                Ok(found) => {
                    self.performance_found = Some(found);
                    self.reading_performance = None;
                    self.build_performance_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_performance = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_migration {
            match receiver.try_recv() {
                Ok((found, known)) => {
                    self.migration_here = Some(found);
                    self.migration_packages_known = known;
                    self.reading_migration = None;
                    self.build_migration_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_migration = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_home {
            match receiver.try_recv() {
                Ok(found) => {
                    self.home_status = Some(found);
                    self.reading_home = None;
                    self.build_home_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_home = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_restic {
            match receiver.try_recv() {
                Ok(found) => {
                    self.restic_reading = Some(found);
                    self.reading_restic = None;
                    self.build_restic_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_restic = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.browsing_restic {
            match receiver.try_recv() {
                Ok(found) => {
                    self.restic_browse = Some(found);
                    self.browsing_restic = None;
                    self.build_restic_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.browsing_restic = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.searching_restic {
            match receiver.try_recv() {
                Ok(found) => {
                    self.restic_search = Some(found);
                    self.searching_restic = None;
                    self.build_restic_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.searching_restic = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.opening_migration {
            match receiver.try_recv() {
                Ok((path, read)) => {
                    self.opening_migration = None;
                    self.opened_migration(path, read);
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.opening_migration = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_repos {
            match receiver.try_recv() {
                Ok(found) => {
                    self.reading_repos = None;
                    self.repos_found = Some(found);
                    self.build_repo_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_repos = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.opening_repo {
            match receiver.try_recv() {
                Ok(read) => {
                    self.opening_repo = None;
                    self.opened_repo(read);
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.opening_repo = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(receiver) = &self.reading_storage {
            match receiver.try_recv() {
                Ok(found) => {
                    self.storage_found = Some(found);
                    self.reading_storage = None;
                    self.build_storage_rows();
                    return true;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.reading_storage = None,
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
                self.bundle_folder = text.trim().to_string();
                return;
            }
            Asked::InstallFolder => {
                self.bundle_source = text.trim().to_string();
                return;
            }
            Asked::ServiceFilter => {
                self.service_filter = text.trim().to_string();
                self.main_state.select(Some(0));
                self.build_service_rows();
                return;
            }
            Asked::CommitMessage => {
                self.plan_repo_change(Change::Commit(text.to_string()));
                return;
            }
            Asked::ConfigField(kind, key) => {
                self.set_config(kind, key, text);
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
            Asked::AddTask => ("Task kept", self.add_repo_task(text)),
            Asked::ExportMigration => ("Migration exported", self.export_migration(text)),
            Asked::ResticHistory => {
                self.search_restic(text.trim().to_string());
                return;
            }
            Asked::ResticRestore(snapshot, include) => {
                self.restore_restic(&snapshot, include.as_deref(), text);
                return;
            }
            Asked::ResticRestoreFrom => {
                // The snapshot, then what of it, as the window's two fields.
                let text = text.trim();
                let (snapshot, include) = match text.split_once(char::is_whitespace) {
                    Some((snapshot, include)) => (snapshot, Some(include.trim().to_string())),
                    None => (text, None),
                };
                self.ask_restore_into(snapshot.to_string(), include);
                return;
            }
            Asked::ResticBaseline => ("Baseline recorded", self.record_baseline(text)),
            Asked::ResticTest(baseline) => {
                self.test_restic(&baseline, text.trim());
                return;
            }
            Asked::OpenMigration => {
                match typed_path(text) {
                    Ok(path) => self.open_migration(path),
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
        if !self.check_statuses {
            // Tests: no machine questions beyond what planning asked.
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
        let evidence = self.evidence();
        let choices = confirmation::Choices {
            snapshot: self.snapshot_tool.filter(|_| self.snapshots),
            keep_going: self.keep_going,
        };
        let prepared = self.with_facts(|facts| {
            confirmation::prepare(
                planned,
                &self.platform,
                &|id| self.catalog.find(id),
                choices,
                facts,
                &evidence,
            )
        });
        let ready = match prepared {
            Prepared::Nothing(lines) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Nothing to run",
                    lines.join("\n"),
                    true,
                )));
                return;
            }
            Prepared::Blocked(lines) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot run yet",
                    format!("{}\n\nNothing was changed.", lines.join("\n")),
                    true,
                )));
                return;
            }
            Prepared::Ready(ready) => ready,
        };
        let header = ready.header;
        let lines: Vec<(String, String)> = ready
            .lines
            .into_iter()
            .map(|line| (line.name, line.detail))
            .collect();
        self.pending = Some(ready.plan);
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
            std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("helix-toolbox"));
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
fn hardware_rows(found: &helix_toolbox_engine::hardware::Hardware) -> Vec<HardwareRow> {
    use helix_toolbox_engine::hardware::size;
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
        let updatable: Vec<&helix_toolbox_engine::hardware::FirmwareDevice> = found
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
    let hot: Vec<&helix_toolbox_engine::hardware::Sensor> =
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

/// A task's command as typed, and the shell a "fish:" or "bash:" in front
/// of it names.
fn task_shell(text: &str) -> (workspaces::Shell, String) {
    let text = text.trim();
    for (prefix, shell) in [
        ("fish:", workspaces::Shell::Fish),
        ("bash:", workspaces::Shell::Bash),
    ] {
        if let Some(rest) = text.strip_prefix(prefix)
            && !rest.trim().is_empty()
        {
            return (shell, rest.trim().to_string());
        }
    }
    (workspaces::Shell::Direct, text.to_string())
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

/// How the Migration screen names a kind of thing.
fn migration_kind(kind: migration::Kind) -> &'static str {
    match kind {
        migration::Kind::Entry => "entry",
        migration::Kind::Flatpak => "Flatpak",
        migration::Kind::Package => "package",
    }
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
