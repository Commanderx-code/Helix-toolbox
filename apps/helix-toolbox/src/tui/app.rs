//! The screens: a header, a sidebar menu, the current screen and a hint line,
//! after the Helix Toolbox dashboard design.

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

    fn reload_history(&mut self) {
        self.records = history::read(200).unwrap_or_default();
        self.clamp_main();
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
            Screen::Bundles => Icon::Bundles,
            Screen::Services => Icon::Services,
            Screen::Storage => Icon::Storage,
            Screen::Performance => Icon::Performance,
            Screen::Recovery => Icon::Recovery,
            Screen::Migration => Icon::Migration,
            Screen::Restic => Icon::Restic,
            Screen::Repositories => Icon::Repositories,
            Screen::Configs => Icon::Configs,
            Screen::HomeManager => Icon::HomeManager,
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
        self.hits.get_mut().clear();
        let area = frame.area();
        frame.render_widget(Block::default().style(self.theme.base()), area);
        let too_small = (area.width < MIN_WIDTH || area.height < MIN_HEIGHT)
            && !self.size_bypass
            && self.popup.is_none();
        if too_small {
            let warning = Paragraph::new(format!(
                "The terminal is {} by {}; Helix Toolbox needs {} by {}.\nEnlarge the window, or start with --size-bypass.",
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
        self.hits.get_mut().main(main);
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
            Screen::Bundles => self.draw_bundles(frame, main),
            Screen::Services => self.draw_services(frame, main),
            Screen::Storage => self.draw_storage(frame, main),
            Screen::Performance => self.draw_performance(frame, main),
            Screen::Recovery => self.draw_recovery(frame, main),
            Screen::Migration => self.draw_migration(frame, main),
            Screen::Restic => self.draw_restic(frame, main),
            Screen::Repositories => self.draw_repos(frame, main),
            Screen::Configs => self.draw_configs(frame, main),
            Screen::HomeManager => self.draw_home(frame, main),
            Screen::Doctor => self.draw_doctor(frame, main),
            Screen::Settings => self.draw_settings(frame, main),
        }
        match &mut self.popup {
            // The keys are wider than the screen beside the sidebar; under
            // the header, which the banner's picture stays drawn over.
            Some(Popup::Text(popup)) if popup.title() == "Keys" => {
                let below = Rect::new(
                    area.x,
                    rows[1].y,
                    area.width,
                    rows[1].height + rows[2].height,
                );
                popup.draw(frame, centered(below, 98, 98), self.theme)
            }
            Some(Popup::Text(popup) | Popup::HardwareReport(popup, _)) => {
                popup.draw(frame, centered(main, 95, 95), self.theme)
            }
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup)
                | Popup::Ask(popup, _),
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
            // The name and tagline in the middle two rows, beside the emblem.
            let top = area.y + size.height.saturating_sub(2) / 2;
            let text = Rect::new(
                area.x + size.width + 4,
                top,
                area.width.saturating_sub(size.width + 6),
                2,
            )
            .intersection(area);
            let [helix, toolbox] = WORDMARK;
            let brand = Text::from(vec![
                Line::from(vec![
                    Span::styled(helix, Style::default().fg(self.theme.focused()).bold()),
                    Span::raw("   "),
                    Span::styled(toolbox, Style::default().fg(self.theme.title()).bold()),
                ]),
                Line::styled(TAGLINE, Style::default().fg(self.theme.faint())),
            ]);
            frame.render_widget(Paragraph::new(brand), text);
            let middle = Rect::new(area.x, top, area.width.saturating_sub(2), 1).intersection(area);
            let room = (text.width as usize)
                .saturating_sub(helix.chars().count() + 3 + toolbox.chars().count());
            frame.render_widget(
                Paragraph::new(self.header_status(status, color, room)).alignment(Alignment::Right),
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
                Span::styled(" TOOLBOX", Style::default().fg(self.theme.title()).bold()),
            ]),
            Line::styled(TAGLINE, Style::default().fg(self.theme.faint())),
        ]);
        frame.render_widget(Paragraph::new(brand), text);
        let room = (text.width as usize).saturating_sub("HELIX TOOLBOX".len());
        frame.render_widget(
            Paragraph::new(self.header_status(status, color, room)).alignment(Alignment::Right),
            text,
        );
        self.rule(frame, area.x, area.bottom().saturating_sub(1), area.width);
    }

    /// READY or RUNNING at the right of the header, with the version before
    /// it when `room` columns beside the name hold both.
    fn header_status(&self, status: &'static str, color: Color, room: usize) -> Line<'static> {
        const VERSION: &str = concat!("v", env!("CARGO_PKG_VERSION"), "   ");
        let mut spans = Vec::new();
        if VERSION.chars().count() + status.chars().count() + 2 <= room {
            spans.push(Span::styled(
                VERSION,
                Style::default().fg(self.theme.faint()),
            ));
        }
        spans.push(Span::styled(status, Style::default().fg(color).bold()));
        Line::from(spans)
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
            let label = self.menu_label(item, current, inner.width.saturating_sub(4));
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
            let card = Rect::new(inner.x, y, inner.width, 3);
            self.hits.get_mut().menu(card.intersection(inner), index);
            let inside = self.framed(frame, card, border, fill);
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

    /// A menu item's icon and label, with its count at the right of
    /// `width` when it has one, as the window's sidebar shows.
    fn menu_label(&self, item: &MenuItem, current: bool, width: u16) -> Line<'static> {
        let icon = self.icon_gap(self.menu_icon(item.screen));
        let text_style = if current {
            Style::default().fg(self.theme.on_selected()).bold()
        } else {
            Style::default().fg(self.theme.entry())
        };
        let count = self.menu_count(item.screen);
        let used = icon.chars().count() + item.label.chars().count();
        let mut spans = vec![
            Span::styled(
                icon,
                Style::default().fg(if current {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                }),
            ),
            Span::styled(item.label.clone(), text_style),
        ];
        if count > 0 {
            let number = count.to_string();
            let gap = (width as usize).saturating_sub(used + number.chars().count());
            if gap >= 1 {
                // The Install Plan's count stands out, as in the window.
                let color = if item.screen == Screen::Plan {
                    self.theme.focused()
                } else {
                    self.theme.muted()
                };
                spans.push(Span::raw(" ".repeat(gap)));
                spans.push(Span::styled(number, Style::default().fg(color).bold()));
            }
        }
        Line::from(spans)
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
                    self.hits.borrow_mut().menu(area, *index);
                    let item = &self.menu[*index];
                    let current = item.screen == self.screen;
                    self.menu_row(
                        frame,
                        area,
                        self.menu_label(item, current, area.width.saturating_sub(3)),
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

    /// A list's items, their heights noted for the mouse.
    fn measure<'a, T: Into<ListItem<'a>>>(
        &self,
        items: impl IntoIterator<Item = T>,
    ) -> Vec<ListItem<'a>> {
        let items: Vec<ListItem> = items.into_iter().map(Into::into).collect();
        self.hits
            .borrow_mut()
            .measured(items.iter().map(|item| item.height() as u16).collect());
        items
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
    /// border; draw the card's widget over it afterward.
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
        // The quick actions in two columns where two of them fit side by
        // side: cards need more room than lines.
        let content_width = content.width;
        let columns = move |card: u16| -> u16 {
            let wide = if card == 1 { 56 } else { 72 };
            if content_width >= wide { 2 } else { 1 }
        };
        let overview_height = |spaced: bool| if spaced { rows * 2 + 1 } else { rows + 2 };
        let actions = |card: u16| (QUICK_ACTIONS.len() as u16).div_ceil(columns(card)) * card;
        // Roomy rows and two-line cards with the counts as tiles when they
        // fit, the counts as a line before smaller cards, tighter otherwise.
        // `tiles` is the counts' height: tiles with a blank line after them,
        // or one line.
        let (tiles, spaced, card) = [
            (4, true, 4),
            (4, false, 4),
            (1, false, 4),
            (4, false, 3),
            (1, false, 3),
            (0, false, 3),
            (1, false, 1),
            (0, false, 1),
        ]
        .into_iter()
        .find(|&(tiles, spaced, card)| {
            tiles + overview_height(spaced) + 3 + actions(card) <= content.height
        })
        .unwrap_or((0, false, 1));
        self.draw_counts(frame, Rect::new(content.x, content.y, content.width, tiles));
        let content = Rect::new(
            content.x,
            content.y + tiles,
            content.width,
            content.height.saturating_sub(tiles),
        );
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
        let gap = u16::from(overview_height(spaced) + 3 + actions(card) <= content.height);
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
        let selected = self.main_state.selected().unwrap_or(0);
        let columns = columns(card);
        // Side by side, in reading order, with a column between them.
        let column_width = (list.width + 1) / columns - 1;
        if card == 1 {
            let marker = self.marker(focused);
            let blank = " ".repeat(marker.chars().count());
            for (index, (_, title, detail, _)) in QUICK_ACTIONS.iter().enumerate() {
                let row = index as u16 / columns;
                let column = index as u16 % columns;
                if row >= list.height {
                    break;
                }
                let rect = Rect::new(
                    list.x + column * (column_width + 1),
                    list.y + row,
                    column_width,
                    1,
                );
                self.hits.get_mut().row(rect, index);
                let chosen = index == selected;
                let line = Line::from(vec![
                    Span::raw(if chosen { marker } else { blank.as_str() }.to_string()),
                    Span::styled(*title, Style::default().fg(self.theme.entry()).bold()),
                    Span::styled(
                        format!("   {detail}"),
                        Style::default().fg(self.theme.muted()),
                    ),
                ]);
                let style = if chosen {
                    self.highlight(focused)
                } else {
                    Style::default()
                };
                frame.render_widget(Paragraph::new(line).style(style), rect);
            }
            return;
        }
        for (index, (icon, title, detail, _)) in QUICK_ACTIONS.iter().enumerate() {
            let row = index as u16 / columns;
            let column = index as u16 % columns;
            let rect = Rect::new(
                list.x + column * (column_width + 1),
                list.y + row * card,
                column_width,
                card,
            );
            if rect.bottom() > list.bottom() {
                break;
            }
            self.hits.get_mut().row(rect, index);
            self.action_card(
                frame,
                rect,
                *icon,
                title,
                detail,
                index == selected,
                focused,
            );
        }
    }

    /// The dashboard's counts, as the window's tiles: installed, updates
    /// waiting, queued and pinned. Four tiles in three rows, else one line.
    fn draw_counts(&self, frame: &mut Frame, area: Rect) {
        if area.height == 0 {
            return;
        }
        let counts = [
            (self.installed_count(), "installed", self.theme.success()),
            (self.update_count(), "updates", self.theme.warning()),
            (self.queue.len(), "queued", self.theme.focused()),
            (self.favorites.len(), "pinned", self.theme.title()),
        ];
        if area.height < 4 {
            let mut spans = Vec::new();
            for (index, (count, label, color)) in counts.iter().enumerate() {
                if index > 0 {
                    spans.push(Span::styled(" · ", Style::default().fg(self.theme.faint())));
                }
                spans.push(Span::styled(
                    count.to_string(),
                    Style::default().fg(*color).bold(),
                ));
                spans.push(Span::styled(
                    format!(" {label}"),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            frame.render_widget(
                Paragraph::new(Line::from(spans)),
                Rect::new(area.x, area.y, area.width, 1),
            );
            return;
        }
        let tile = (area.width + 1) / 4 - 1;
        for (index, (count, label, color)) in counts.iter().enumerate() {
            let rect = Rect::new(area.x + index as u16 * (tile + 1), area.y, tile, 3);
            let inside = self.framed(frame, rect, self.theme.line(), self.theme.card());
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(count.to_string(), Style::default().fg(*color).bold()),
                    Span::styled(format!(" {label}"), Style::default().fg(self.theme.muted())),
                ]))
                .alignment(Alignment::Center),
                inside,
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
        // The chevron takes the last two columns.
        let text = Rect::new(
            inner.x,
            inner.y,
            inner.width.saturating_sub(2),
            inner.height,
        );
        let room = usize::from(text.width);
        let lines = if inner.height >= 2 {
            vec![
                Line::from(vec![icon, Span::styled(title.to_string(), title_style)]),
                Line::from(vec![
                    Span::raw(indent.clone()),
                    Span::styled(
                        clip(detail, room.saturating_sub(indent.chars().count())),
                        Style::default().fg(self.theme.muted()),
                    ),
                ]),
            ]
        } else {
            vec![Line::from(vec![
                icon,
                Span::styled(title.to_string(), title_style),
                Span::styled(
                    format!(
                        "   {}",
                        clip(
                            detail,
                            room.saturating_sub(indent.chars().count() + title.chars().count() + 3)
                        )
                    ),
                    Style::default().fg(self.theme.muted()),
                ),
            ])]
        };
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
        let searching = self.searching();
        let lines: Vec<Line> = self
            .rows
            .iter()
            .map(|row| match row {
                Row::Up => Line::from(format!("{group}  .."))
                    .style(Style::default().fg(self.theme.group())),
                Row::Group {
                    name,
                    description,
                    count,
                    ..
                } => {
                    // Its name, what it holds, and how many, as the window shows a group.
                    let left = format!("{group}  {name}");
                    let number = format!("{count} ");
                    let room = content_width
                        .saturating_sub(left.chars().count() + number.chars().count() + 2);
                    let about = if description.is_empty() || room < 8 {
                        String::new()
                    } else {
                        format!("  {}", clipped(description, room))
                    };
                    let gap = " ".repeat(content_width.saturating_sub(
                        left.chars().count() + about.chars().count() + number.chars().count(),
                    ));
                    Line::from(vec![
                        Span::styled(left, Style::default().fg(self.theme.group())),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::raw(gap),
                        Span::styled(number, Style::default().fg(self.theme.muted())),
                    ])
                }
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
                    // An entry that cannot be queued is marked as running on its own.
                    let icon = if queued {
                        self.glyph(Icon::Queued)
                    } else if entry.multi_select {
                        self.glyph(Icon::Entry)
                    } else {
                        self.glyph(Icon::Alone)
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
                    // Between them, where it fits: where a search result is
                    // listed, then the description, as the window's rows show.
                    let room = content_width.saturating_sub(left.chars().count() + right + 3);
                    let place = if searching {
                        self.place_label(&entry.id)
                    } else {
                        None
                    };
                    let about = match (place, entry.description.is_empty()) {
                        (Some(place), true) => place,
                        (Some(place), false) => format!("{place} · {}", entry.description),
                        (None, _) => entry.description.clone(),
                    };
                    let about = if about.is_empty() || room < 8 {
                        String::new()
                    } else {
                        format!("  {}", clipped(&about, room))
                    };
                    let gap = " ".repeat(
                        content_width
                            .saturating_sub(left.chars().count() + about.chars().count() + right),
                    );
                    Line::from(vec![
                        Span::styled(icon.to_string(), Style::default().fg(icon_color)),
                        Span::styled(format!("  {}{suffix}", entry.name), style),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(color)),
                        Span::styled(detail, Style::default().fg(self.theme.faint())),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ])
                }
            })
            .collect();
        self.fill(frame, area);
        let list = List::new(self.measure(lines))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(marker)
            .scroll_padding(1);
        bordered_list(frame, list, area, &mut self.main_state, &self.hits);
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
        let block = self.card(&format!("Queued entries · {}", self.queue.len()));
        let marker = self.marker(focused);
        let width = (block.inner(parts[2]).width as usize).saturating_sub(marker.chars().count());
        // Each entry in two lines, as the window's rows: its number, name,
        // status and flags, then where it is listed, its kind and its step.
        let items: Vec<ListItem> = self
            .queue
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let command = steps
                    .iter()
                    .find(|step| step.id() == entry.id)
                    .map(|step| step.display())
                    .unwrap_or_else(|| "nothing to do".to_string());
                let icon = self.icon_gap(Icon::Queued);
                let number = format!("{}. ", index + 1);
                let (word, color) = match self.statuses.get(&entry.id) {
                    Some(status) => (status.label().to_string(), self.status_color(status)),
                    None => (String::new(), self.theme.muted()),
                };
                let flags = format!("  {} ", entry.flags.trim());
                let left = icon.chars().count() + number.len() + entry.name.chars().count();
                let gap = " ".repeat(
                    width.saturating_sub(left + word.chars().count() + flags.chars().count()),
                );
                let indent = " ".repeat(icon.chars().count() + number.len());
                let mut about = vec![kind_label(&entry.action).to_string()];
                if let Some(place) = self.place_label(&entry.id) {
                    about.insert(0, place);
                }
                let about = about.join(" · ");
                let room = width.saturating_sub(indent.len() + about.chars().count() + 3);
                ListItem::new(Text::from(vec![
                    Line::from(vec![
                        Span::styled(icon, Style::default().fg(self.theme.success())),
                        Span::styled(number, Style::default().fg(self.theme.faint())),
                        Span::styled(
                            entry.name.clone(),
                            Style::default().fg(self.theme.entry()).bold(),
                        ),
                        Span::raw(gap),
                        Span::styled(word, Style::default().fg(color)),
                        Span::styled(flags, Style::default().fg(self.theme.muted())),
                    ]),
                    Line::from(vec![
                        Span::raw(indent),
                        Span::styled(about, Style::default().fg(self.theme.faint())),
                        Span::styled(
                            format!("   {}", clipped(&command, room)),
                            Style::default().fg(self.theme.muted()),
                        ),
                    ]),
                ]))
            })
            .collect();
        self.fill(frame, parts[2]);
        let list = List::new(self.measure(items))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }

    fn draw_backups(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Backups",
            "Configuration backups Helix Toolbox's entries left, newest first.",
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
        let closing =
            "Helix Toolbox restores configuration backups; it does not roll packages back.";
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
            let text = helix_toolbox_core::printable(&path.display().to_string());
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
        let list = List::new(self.measure(items))
            .block(self.card("Backups"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
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
        let list = List::new(self.measure(items))
            .block(self.card("Runs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
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
            (
                "Tell me when updates are waiting".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(watch) if watch.updates => format!("yes, {}", notifications::EVERY),
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Tell me of new health problems".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(watch) if watch.health => "yes".to_string(),
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Tell me when it drifts from a baseline".to_string(),
                match self.watching() {
                    None => "not available here".to_string(),
                    Some(Watch {
                        drift: Some(slug), ..
                    }) => format!("from {slug}"),
                    Some(_) if self.baseline_slugs().is_empty() => {
                        "no (save one in Drift)".to_string()
                    }
                    Some(_) => "no".to_string(),
                },
            ),
            (
                "Back up with restic on a timer".to_string(),
                self.restic_timer_label(),
            ),
            ("Setup wizard".to_string(), "Enter opens it".to_string()),
            (
                "Bug report, to paste into an issue".to_string(),
                "Enter makes one".to_string(),
            ),
            (
                "Log level (HELIX_TOOLBOX_LOG)".to_string(),
                crate::logging::requested().unwrap_or_else(|| "info".to_string()),
            ),
            (
                "Mouse".to_string(),
                if self.mouse {
                    "on (Shift+drag selects text)".to_string()
                } else {
                    "off".to_string()
                },
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
        // Where they are kept, as the window says.
        let mut block = self.card("Settings");
        if let Some(path) = &self.settings_path {
            block = block.title_bottom(Line::styled(
                format!(
                    " Kept in {} ",
                    helix_toolbox_core::printable(&path.display().to_string())
                ),
                Style::default().fg(self.theme.faint()),
            ));
        }
        let list = List::new(self.measure(items))
            .block(block)
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        self.rule(frame, area.x, area.y, area.width);
        // How the check of package entries stands, on the rule, as the
        // window's footer says it.
        if let Some(status) = self.check_line() {
            let width = status.chars().count() as u16 + 2;
            let x = area.right().saturating_sub(width + 2).max(area.x);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    format!(" {status} "),
                    Style::default().fg(self.theme.faint()),
                )),
                Rect::new(x, area.y, width.min(area.width), 1).intersection(area),
            );
        }
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
        let right_line = Line::from(spans(&right));
        let right_x = row.right().saturating_sub(right_line.width() as u16);
        self.hint_hits(&left, row.x, row);
        self.hint_hits(&right, right_x, row);
        frame.render_widget(Paragraph::new(Line::from(spans(&left))), row);
        frame.render_widget(Paragraph::new(right_line).alignment(Alignment::Right), row);
    }

    /// Where each of the hints drawn from `x` on is, for a click: each key
    /// of a hint where its name is, its description sending the first.
    fn hint_hits(&self, hints: &[&Hint], mut x: u16, row: Rect) {
        let mut hits = self.hits.borrow_mut();
        let width = |text: &str| Span::raw(text).width() as u16;
        for (index, hint) in hints.iter().enumerate() {
            if index > 0 {
                x += width(" · ");
            }
            let start = x;
            for (number, name) in hint.keys().iter().enumerate() {
                if number > 0 {
                    x += width("/");
                }
                if let Some(key) = mouse::key_named(name) {
                    hits.key(Rect::new(x, row.y, width(name), 1).intersection(row), key);
                }
                x += width(name);
            }
            let text = width(&format!(" {}", hint.text()));
            if let Some(key) = hint.keys().first().and_then(|name| mouse::key_named(name)) {
                hits.key(
                    Rect::new(start, row.y, x - start + text, 1).intersection(row),
                    key,
                );
            }
            x += text;
        }
    }

    fn hints(&self) -> (&'static str, Vec<Hint>) {
        match &self.popup {
            Some(Popup::Text(popup)) => return popup.hints(),
            Some(Popup::HardwareReport(popup, _)) => {
                let (title, mut list) = popup.hints();
                list.insert(0, Hint::new(&["o"], "Open the issue"));
                return (title, list);
            }
            Some(
                Popup::ProfileName(popup)
                | Popup::BaselineName(popup)
                | Popup::FirmwareWord(popup)
                | Popup::LookName(popup)
                | Popup::Ask(popup, _),
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
                            if matches!(entry.action, Action::Profile { .. }) {
                                list.push(Hint::new(&["e"], "Its entries to plan"));
                            }
                            list.push(Hint::new(&["d"], "Describe"));
                            list.push(Hint::new(&["p"], "Preview"));
                            list.push(Hint::new(&["v"], "Trust"));
                            list.push(Hint::new(&["c"], "Check"));
                            if own_profile(entry) {
                                list.push(Hint::new(&["x"], "Export"));
                                list.push(Hint::new(&["Del"], "Delete"));
                            }
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
                    if self.in_profiles() {
                        list.push(Hint::new(&["o"], "Import"));
                    }
                }
                Screen::Plan => {
                    list.push(Hint::new(&["Enter"], "Run plan"));
                    if !self.queue.is_empty() {
                        list.push(Hint::new(&["e"], "Changes"));
                        list.push(Hint::new(&["s"], "Save as profile"));
                    }
                    if self.queue.len() > 1 {
                        list.push(Hint::new(&["K", "J"], "Move"));
                    }
                    list.push(Hint::new(&["x"], "Remove"));
                    list.push(Hint::new(&["c"], "Clear"));
                }
                Screen::History => {
                    list.push(Hint::new(&["Enter"], "Show log"));
                    if self
                        .main_state
                        .selected()
                        .and_then(|index| self.records.get(index))
                        .is_some_and(|record| record.success)
                    {
                        list.push(Hint::new(&["w"], "Report it worked"));
                    }
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
                        None => {
                            list.push(Hint::new(&["Enter"], "Run"));
                            list.push(Hint::new(&["Space"], "Plan"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                    }
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Updates => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_update_row() {
                        UpdateRow::Channel => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            list.push(Hint::new(&["u"], "Update"));
                            list.push(Hint::new(&["U"], "Update all"));
                        }
                        UpdateRow::Entry(_) => {
                            list.push(Hint::new(&["Enter", "u"], "Update it"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        UpdateRow::Every => list.push(Hint::new(&["Enter"], "Update every entry")),
                        UpdateRow::Nothing => {}
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
                        list.push(Hint::new(&["R"], "Run them now"));
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
                    if let Some((_, _, assessment)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                    {
                        list.push(Hint::new(&["Enter"], "Details"));
                        list.push(Hint::new(&["v"], "Trust"));
                        if assessment.here.is_some() {
                            list.push(Hint::new(&["w"], "Report it worked"));
                        }
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
                Screen::Bundles => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    let verb = match self.selected_bundle_row() {
                        Some(BundleRow::Entries) => "Install Plan",
                        Some(BundleRow::Folder | BundleRow::Source) => "Change",
                        Some(BundleRow::Complete | BundleRow::Trust) => "Switch",
                        Some(BundleRow::Contents | BundleRow::Check) => "Check",
                        Some(BundleRow::Make) => "Make",
                        Some(BundleRow::Install) => "Install",
                        _ => "",
                    };
                    if !verb.is_empty() {
                        list.push(Hint::new(&["Enter"], verb));
                    }
                }
                Screen::Services => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_service_row() {
                        Some(ServiceRow::Unit(index)) => {
                            list.push(Hint::new(&["Enter"], "Details"));
                            if let Some(unit) = self.unit(index) {
                                list.push(Hint::new(
                                    &["s"],
                                    if unit.running() { "Stop" } else { "Start" },
                                ));
                                if unit.switchable() {
                                    list.push(Hint::new(
                                        &["e"],
                                        if unit.enabled() { "Disable" } else { "Enable" },
                                    ));
                                }
                                if unit.running() {
                                    list.push(Hint::new(&["R"], "Restart"));
                                }
                            }
                        }
                        Some(ServiceRow::Autostart(_)) => {
                            list.push(Hint::new(&["Enter"], "On or off at login"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        _ => {}
                    }
                    list.push(Hint::new(&["i"], "Show"));
                    list.push(Hint::new(&["f"], "Filter"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Storage => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if let Some(StorageRow::Item(index)) = self.selected_storage_row()
                        && self
                            .storage_found
                            .as_ref()
                            .and_then(|found| found.items.get(index))
                            .is_some_and(|item| item.not_offered.is_none())
                    {
                        list.push(Hint::new(&["Enter"], "Clean"));
                    }
                    list.push(Hint::new(&["d"], "Details"));
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Performance => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    match self.selected_performance_row() {
                        Some(PerformanceRow::Hint(_)) => {
                            list.push(Hint::new(&["Enter"], "Act on it"));
                            list.push(Hint::new(&["d"], "Details"));
                        }
                        Some(PerformanceRow::Unit(..)) => {
                            list.push(Hint::new(&["Enter"], "In Services"))
                        }
                        _ => {}
                    }
                    if self.earlier_boots().len() > 1 {
                        list.push(Hint::new(&["b"], "An earlier boot"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Recovery => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    list.push(Hint::new(&["Enter"], "Details"));
                    if self
                        .selected_readiness()
                        .and_then(|check| check.entry.as_deref())
                        .and_then(|id| self.catalog.find(id))
                        .is_some()
                    {
                        list.push(Hint::new(&["f"], "Set it up"));
                    }
                    if self.snapshots_here().is_some() {
                        list.push(Hint::new(&["c"], "Check snapshots"));
                    }
                    list.push(Hint::new(&["r"], "Check again"));
                }
                Screen::Migration => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if self.selected_migration_item().is_some() {
                        list.push(Hint::new(&["d"], "Details"));
                    }
                    list.push(Hint::new(&["e"], "Export"));
                    list.push(Hint::new(&["o"], "Open a file"));
                    if self
                        .migration_opened
                        .as_ref()
                        .is_some_and(|(_, translation)| translation.count(Standing::Install) > 0)
                    {
                        list.push(Hint::new(&["a"], "Install"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Restic => {
                    list.push(Hint::new(&["↑↓"], "Navigate"));
                    if self.restic_config().is_some() {
                        match self
                            .restic_rows
                            .get(self.main_state.selected().unwrap_or(0))
                        {
                            Some(ResticRow::Snapshot(_)) => {
                                list.push(Hint::new(&["Enter"], "Browse"));
                                list.push(Hint::new(&["R"], "Restore"));
                            }
                            Some(ResticRow::Node(_) | ResticRow::Version(..)) => {
                                list.push(Hint::new(&["R"], "Restore"));
                            }
                            Some(ResticRow::Baseline(_)) => list.push(Hint::new(&["v"], "Test")),
                            _ => list.push(Hint::new(&["F"], "Restore…")),
                        }
                        list.push(Hint::new(&["b"], "Back up"));
                        list.push(Hint::new(&["s"], "History"));
                        if self.restic_search.is_some() {
                            list.push(Hint::new(
                                &["i"],
                                if self.restic_ignore_case {
                                    "Match case"
                                } else {
                                    "Ignore case"
                                },
                            ));
                        }
                        list.push(Hint::new(&["n"], "Baseline"));
                    }
                    list.push(Hint::new(&["r"], "Read again"));
                }
                Screen::Repositories => self.repo_hints(&mut list),
                Screen::Configs => self.config_hints(&mut list),
                Screen::HomeManager => self.home_hints(&mut list),
                Screen::Settings => list.push(Hint::new(&["Enter"], "Change")),
            },
        }
        list.push(Hint::new(&["/"], "Search"));
        list.push(Hint::new(&["^K"], "Commands"));
        list.push(Hint::new(&["?"], "Help"));
        list.push(Hint::new(&["q"], "Quit"));
        ("Helix Toolbox", list)
    }

    // ----- keys -----

    /// Returns false when Helix Toolbox should exit.
    /// Whether the terminal should report the mouse.
    pub fn mouse(&self) -> bool {
        self.mouse
    }

    /// A click or the wheel, done as the keys that do the same, so nothing
    /// a click starts skips its review. Returns false to quit, as a click
    /// on the q hint asks.
    pub fn handle_mouse(&mut self, event: &MouseEvent) -> bool {
        if !self.mouse {
            return true;
        }
        let target = self.hits.borrow().at(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollDown => self.wheel(true, event),
            MouseEventKind::ScrollUp => self.wheel(false, event),
            MouseEventKind::Down(MouseButton::Left) => match target {
                Some(Target::Key(key)) => self.handle_key(&key),
                // Behind a popup only its hints answer.
                _ if self.popup.is_some() => true,
                Some(Target::Menu(index)) => {
                    let screen = self.menu[index].screen;
                    if screen == self.screen && self.focus == Focus::Sidebar {
                        self.press(KeyCode::Enter)
                    } else {
                        if screen != self.screen {
                            self.open_screen(screen);
                        }
                        self.focus = Focus::Sidebar;
                        true
                    }
                }
                Some(Target::Row(index)) => self.click_row(index),
                None => true,
            },
            _ => true,
        }
    }

    fn press(&mut self, code: KeyCode) -> bool {
        self.handle_key(&KeyEvent::new(code, KeyModifiers::NONE))
    }

    /// Give the main list the focus, as the keys would: Enter from the
    /// sidebar or the search line.
    fn focus_main(&mut self) -> bool {
        match self.focus {
            Focus::Main => true,
            Focus::Sidebar | Focus::Search => self.press(KeyCode::Enter),
        }
    }

    /// A click on a row selects it, by the arrow keys; a click on the row
    /// already selected in the focused list is Enter.
    fn click_row(&mut self, index: usize) -> bool {
        if self.focus != Focus::Main {
            return self.focus_main() && self.select_by_keys(index);
        }
        let selected = self.main_state.selected().unwrap_or(0);
        if index == selected {
            return self.press(KeyCode::Enter);
        }
        self.select_by_keys(index)
    }

    fn select_by_keys(&mut self, index: usize) -> bool {
        for _ in 0..self.main_len() {
            let before = self.main_state.selected().unwrap_or(0);
            if before == index {
                break;
            }
            let code = if index > before {
                KeyCode::Down
            } else {
                KeyCode::Up
            };
            if !self.press(code) {
                return false;
            }
            if self.main_state.selected().unwrap_or(0) == before {
                break;
            }
        }
        true
    }

    /// The wheel scrolls what is under it: a popup's text or list, the
    /// output of a run, or the main list's selection.
    fn wheel(&mut self, down: bool, event: &MouseEvent) -> bool {
        let code = if down { KeyCode::Down } else { KeyCode::Up };
        match &mut self.popup {
            // Keys go to the command running, so the wheel scrolls its output instead.
            Some(Popup::Running(running)) => {
                running.scroll(if down { -3 } else { 3 });
                true
            }
            // A typed answer has nothing to scroll.
            Some(
                Popup::ProfileName(_)
                | Popup::BaselineName(_)
                | Popup::FirmwareWord(_)
                | Popup::LookName(_)
                | Popup::Ask(..),
            ) => true,
            Some(_) => self.press(code),
            None if self.hits.borrow().in_main(event.column, event.row) => {
                self.focus_main() && self.press(code)
            }
            None => true,
        }
    }

    pub fn handle_key(&mut self, key: &KeyEvent) -> bool {
        let control_c =
            key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c');
        if let Some(Popup::Text(popup) | Popup::HardwareReport(popup, _)) = &mut self.popup
            && key.code == KeyCode::Char('c')
            && let Some(text) = popup.copyable()
        {
            let said = (self.copier)(text)
                .unwrap_or_else(|error| format!("The report could not be copied: {error}"));
            popup.noted(said);
            return true;
        }
        match &mut self.popup {
            Some(Popup::Text(popup)) => {
                if popup.handle_key(key) {
                    self.popup = None;
                }
                return true;
            }
            Some(Popup::HardwareReport(popup, url)) => {
                if matches!(key.code, KeyCode::Char('o') | KeyCode::Char('O')) {
                    let text = match (self.opener)(url) {
                        Ok(()) => "The issue is open in your browser, filled in from the report. Say there what you checked, then submit it.".to_string(),
                        Err(error) => format!(
                            "It could not be opened: {error}\n\nThe link is in ~/.local/state/helix-toolbox/hardware-report.md, or helix-toolbox report --worked prints it."
                        ),
                    };
                    self.popup = Some(Popup::Text(TextPopup::new("Hardware report", text, true)));
                } else if popup.handle_key(key) {
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
            Some(Popup::Ask(popup, asked)) => {
                if control_c {
                    return false;
                }
                match popup.handle_key(key) {
                    Answer::Typing => {}
                    Answer::Canceled => self.popup = None,
                    Answer::Given(text) => {
                        let asked = asked.clone();
                        self.popup = None;
                        self.answered(asked, &text);
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
                    if self.screen == Screen::Services {
                        self.reload_services();
                    }
                    if self.screen == Screen::Storage {
                        self.read_storage();
                    }
                    if self.screen == Screen::Recovery {
                        self.reload_recovery();
                    }
                    if self.screen == Screen::Migration {
                        self.reload_migration();
                    }
                    if self.screen == Screen::Restic {
                        self.read_restic();
                    }
                    if self.screen == Screen::Configs {
                        self.read_configs();
                    }
                    if self.screen == Screen::Repositories {
                        self.reload_repos();
                    }
                    if self.screen == Screen::HomeManager {
                        self.read_home();
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
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            self.check_again();
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
        // The Install Plan's order: K or Shift-Up earlier, J or Shift-Down later.
        if self.screen == Screen::Plan {
            let shifted = key.modifiers.contains(KeyModifiers::SHIFT);
            let by = match key.code {
                KeyCode::Char('K') => Some(-1),
                KeyCode::Char('J') => Some(1),
                KeyCode::Up if shifted => Some(-1),
                KeyCode::Down if shifted => Some(1),
                _ => None,
            };
            if let Some(by) = by {
                self.move_queued(by);
                return;
            }
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => self.move_selection(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_selection(-1),
            _ => {}
        }
        match self.screen {
            Screen::Dashboard => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    let selected = self.main_state.selected().unwrap_or(0);
                    let quick = QUICK_ACTIONS
                        .get(selected)
                        .map_or(Quick::Applications, |action| action.3);
                    let screen = match quick {
                        Quick::Applications => Screen::Area(self.first_area()),
                        Quick::Plan => Screen::Plan,
                        Quick::Doctor => Screen::Doctor,
                        Quick::Favorites => Screen::Favorites,
                        Quick::History => Screen::History,
                        Quick::Wizard => return self.open_wizard(false),
                        Quick::Palette => return self.open_palette(),
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
                KeyCode::Char('e') => self.queue_profile_entries(),
                KeyCode::Char('c') => self.check_selected(),
                KeyCode::Char('o') if self.in_profiles() => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Import a profile",
                            "The profile file to add to your profiles, such as ~/Downloads/work.toml:",
                        )
                        .path()
                        .answering("Import"),
                        Asked::ImportProfile,
                    ));
                }
                KeyCode::Char('x') => {
                    if let Some(entry) = self.selected_entry().filter(|entry| own_profile(entry)) {
                        let slug = entry.id.trim_start_matches(profiles::PREFIX);
                        self.popup = Some(Popup::Ask(
                            InputPopup::new(
                                format!("Export {}", entry.name),
                                "The file to write it to, to share or keep:",
                            )
                            .with_value(format!("~/{slug}.toml"))
                            .path()
                            .answering("Export"),
                            Asked::ExportProfile(entry.id.clone()),
                        ));
                    }
                }
                KeyCode::Delete | KeyCode::Char('X') => {
                    if let Some(entry) = self.selected_entry().filter(|entry| own_profile(entry)) {
                        self.popup = Some(Popup::Ask(
                            InputPopup::new(
                                format!("Delete {}", entry.name),
                                "Its file in ~/.config/helix-toolbox/profiles is removed. Type yes to delete it:",
                            )
                            .answering("Delete"),
                            Asked::DeleteProfile(entry.id.clone()),
                        ));
                    }
                }
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
                        "A name for this profile (it is saved in ~/.config/helix-toolbox/profiles):",
                    )));
                }
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::History => match key.code {
                KeyCode::Enter => self.show_log(),
                KeyCode::Char('w') => {
                    if let Some(record) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.records.get(index))
                        .cloned()
                    {
                        self.report_worked(&record, None);
                    }
                }
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
                KeyCode::Char('d') | KeyCode::Char('D') => match self.selected_diagnosis() {
                    Some(_) => self.describe_diagnosis(),
                    None => self.describe(),
                },
                // A report row is an entry like any other.
                KeyCode::Char('p') | KeyCode::Char('P') => self.preview(),
                KeyCode::Char('v') => {
                    if let Some(id) = self.selected_entry().map(|entry| entry.id.clone()) {
                        self.inspect(&id);
                    }
                }
                KeyCode::Char(' ') => self.toggle_queue(),
                KeyCode::Char('*') => self.toggle_favorite(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Updates => match (key.code, self.selected_update_row()) {
                (KeyCode::Enter | KeyCode::Char('u'), UpdateRow::Entry(entry)) => {
                    self.update(vec![*entry])
                }
                (KeyCode::Char('d') | KeyCode::Char('D'), UpdateRow::Entry(_)) => self.describe(),
                (KeyCode::Enter, UpdateRow::Every) => {
                    let waiting = self.entries_with_updates().into_iter().cloned().collect();
                    self.update(waiting);
                }
                (code, _) => match code {
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
                        "A name for the desktop's look (it is saved in ~/.config/helix-toolbox/looks):",
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
                        && let Some(planned) = helix_toolbox_engine::hardware::plan_firmware(update)
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
                        && let Some(planned) = helix_toolbox_engine::hardware::plan_smart(disk)
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
                KeyCode::Char('w') => {
                    if let Some((id, name, _)) = self
                        .main_state
                        .selected()
                        .and_then(|index| self.reliability_rows.get(index))
                        .cloned()
                    {
                        match hardware_report::last_success(&self.run_history(), &id).cloned() {
                            Some(record) => self.report_worked(&record, Some(&id)),
                            None => {
                                self.popup = Some(Popup::Text(TextPopup::new(
                                    "Not run here",
                                    format!(
                                        "A hardware report comes from a run that worked on this machine, and {name} has no successful run in this machine's history."
                                    ),
                                    true,
                                )))
                            }
                        }
                    }
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
                KeyCode::Char('R') => self.run_missing(),
                KeyCode::Char('b') => self.next_drift_target(1),
                KeyCode::Char('B') => self.next_drift_target(-1),
                KeyCode::Char('s') => {
                    self.popup = Some(Popup::BaselineName(InputPopup::new(
                        "Save this machine as a baseline",
                        "A name for the baseline (it is saved in ~/.config/helix-toolbox/baselines):",
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
            Screen::Services => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    match self.selected_service_row() {
                        Some(ServiceRow::Unit(index)) => self.describe_unit(index),
                        Some(ServiceRow::Autostart(index)) => self.switch_autostart(index),
                        _ => {}
                    }
                }
                KeyCode::Char('d') | KeyCode::Char('D') => match self.selected_service_row() {
                    Some(ServiceRow::Unit(index)) => self.describe_unit(index),
                    Some(ServiceRow::Autostart(index)) => self.describe_autostart(index),
                    _ => {}
                },
                KeyCode::Char('s') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row()
                        && let Some(unit) = self.unit(index)
                    {
                        let verb = if unit.running() {
                            Verb::Stop
                        } else {
                            Verb::Start
                        };
                        self.change_unit(index, verb);
                    }
                }
                KeyCode::Char('e') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row()
                        && let Some(unit) = self.unit(index)
                        && unit.switchable()
                    {
                        let verb = if unit.enabled() {
                            Verb::Disable
                        } else {
                            Verb::Enable
                        };
                        self.change_unit(index, verb);
                    }
                }
                KeyCode::Char('R') => {
                    if let Some(ServiceRow::Unit(index)) = self.selected_service_row() {
                        self.change_unit(index, Verb::Restart);
                    }
                }
                KeyCode::Char('i') => {
                    self.service_view = self.service_view.next();
                    self.main_state.select(Some(0));
                    self.build_service_rows();
                }
                KeyCode::Char('f') => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Filter",
                            "Only units and applications whose name or description has (blank for all):",
                        )
                        .with_value(self.service_filter.clone())
                        .answering("Filter"),
                        Asked::ServiceFilter,
                    ));
                }
                KeyCode::Char('r') => self.reload_services(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Storage => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.clean_selected()
                }
                KeyCode::Char('d') => self.describe_storage(),
                KeyCode::Char('r') => self.read_storage(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Performance => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.act_on_performance()
                }
                KeyCode::Char('d') => self.describe_performance(),
                KeyCode::Char('b') => {
                    let earlier = self.earlier_boots().len();
                    if earlier > 1 {
                        self.performance_against = (self.performance_against + 1) % earlier;
                        self.build_performance_rows();
                    }
                }
                KeyCode::Char('r') => self.read_performance(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Recovery => match key.code {
                KeyCode::Enter | KeyCode::Char('d') | KeyCode::Char('l') | KeyCode::Right => {
                    if let Some(check) = self.selected_readiness().cloned() {
                        self.show_diagnosis(&check);
                    }
                }
                KeyCode::Char('f') => {
                    if let Some(entry) = self
                        .selected_readiness()
                        .and_then(|check| check.entry.as_deref())
                        .and_then(|id| self.catalog.find(id))
                        .cloned()
                    {
                        self.confirm(vec![entry]);
                    }
                }
                KeyCode::Char('c') => match self.snapshots_here() {
                    Some(tool) => self.confirm_plan(Ok(recovery::plan_check(tool))),
                    None => {
                        self.popup = Some(Popup::Text(TextPopup::new(
                            "No snapshots",
                            "Neither Snapper (with a root configuration) nor Timeshift is set up, so there are no snapshots to check.",
                            true,
                        )))
                    }
                },
                KeyCode::Char('r') => self.reload_recovery(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Migration => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                    match self.selected_migration_item() {
                        Some(item) if item.standing == Standing::Install => self.apply_migration(),
                        Some(_) => self.describe_migration(),
                        None => {}
                    }
                }
                KeyCode::Char('d') => self.describe_migration(),
                KeyCode::Char('e') => {
                    let name = self
                        .migration_here
                        .as_ref()
                        .map(migration::file_name)
                        .unwrap_or_else(|| "helix-toolbox-migration.toml".to_string());
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Export this machine",
                            "The file to write its entries, Flatpaks and packages to, to take to the other machine:",
                        )
                        .with_value(format!("~/{name}"))
                        .path()
                        .answering("Export"),
                        Asked::ExportMigration,
                    ));
                }
                KeyCode::Char('o') => {
                    let mut ask = InputPopup::new(
                        "Open a migration",
                        "The migration file made on the other machine, such as ~/helix-toolbox-migration.toml:",
                    )
                    .path()
                    .answering("Open");
                    if let Some((path, _)) = &self.migration_opened {
                        ask = ask.with_value(path.display().to_string());
                    }
                    self.popup = Some(Popup::Ask(ask, Asked::OpenMigration));
                }
                KeyCode::Char('a') => self.apply_migration(),
                KeyCode::Char('r') => self.reload_migration(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Restic => match key.code {
                KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.open_restic_row(),
                KeyCode::Char('R') => self.ask_restore(),
                KeyCode::Char('F') => self.ask_restore_from(),
                KeyCode::Char('i') => {
                    self.restic_ignore_case = !self.restic_ignore_case;
                    match self
                        .restic_search
                        .as_ref()
                        .map(|(pattern, _)| pattern.clone())
                    {
                        Some(pattern) => self.search_restic(pattern),
                        None => self.build_restic_rows(),
                    }
                }
                KeyCode::Char('d') => self.describe_restic(),
                KeyCode::Char('b') => self.plan_restic(restic::plan_backup),
                KeyCode::Char('c') => self.plan_restic(restic::plan_check),
                KeyCode::Char('s') if self.restic_config().is_some() => {
                    let mut ask = InputPopup::new(
                        "A file's history",
                        "A file name or path to look for in every snapshot, with * wildcards, such as notes.txt:",
                    )
                    .answering("Look");
                    if let Some((pattern, _)) = &self.restic_search {
                        ask = ask.with_value(pattern.clone());
                    }
                    self.popup = Some(Popup::Ask(ask, Asked::ResticHistory));
                }
                KeyCode::Char('n') => {
                    self.popup = Some(Popup::Ask(
                        InputPopup::new(
                            "Record a baseline",
                            "A file of your home folder whose SHA-256 to record, to test restores of it against:",
                        )
                        .path()
                        .answering("Record"),
                        Asked::ResticBaseline,
                    ));
                }
                KeyCode::Char('v') => self.ask_test(),
                KeyCode::Char('w') => self.toggle_restic_timer(restic::Every::Day),
                KeyCode::Char('W') => self.toggle_restic_timer(restic::Every::Week),
                KeyCode::Char('r') => self.read_restic(),
                KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
                _ => {}
            },
            Screen::Repositories => self.repo_key(key.code),
            Screen::Configs => self.config_key(key.code),
            Screen::HomeManager => self.home_key(key.code),
            Screen::Bundles => match key.code {
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('l') | KeyCode::Right => {
                    self.bundle_enter()
                }
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

    /// Move the selected Install Plan entry earlier (-1) or later (1), as
    /// the window's arrows do; the selection follows it.
    fn move_queued(&mut self, by: isize) {
        let Some(selected) = self.main_state.selected() else {
            return;
        };
        let Some(target) = selected.checked_add_signed(by) else {
            return;
        };
        if selected >= self.queue.len() || target >= self.queue.len() {
            return;
        }
        self.queue.swap(selected, target);
        self.main_state.select(Some(target));
    }

    /// Queue the selected profile's entries one by one, those not queued
    /// yet, to change the plan before it runs; then show the Install Plan.
    fn queue_profile_entries(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let Action::Profile { entries } = &entry.action else {
            return;
        };
        let mut added = 0;
        for id in entries {
            if let Some(found) = self.catalog.find(id).cloned()
                && found.multi_select
                && !self.queue.iter().any(|queued| queued.id == found.id)
            {
                self.queue.push(found);
                added += 1;
            }
        }
        self.open_screen(Screen::Plan);
        self.focus = Focus::Main;
        let text = if added == 0 {
            "Its entries are already in the Install Plan.".to_string()
        } else {
            format!("{added} of its entries were added to the Install Plan. Nothing has run.")
        };
        self.popup = Some(Popup::Text(TextPopup::new(entry.name.clone(), text, true)));
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
        // As the window's details: where it is, what kind, whether it asks.
        let mut tags = vec![kind_label(&entry.action).to_string()];
        if matches!(
            entry.action,
            Action::Script {
                interactive: true,
                ..
            }
        ) {
            tags.push("Asks questions".to_string());
        }
        if !entry.multi_select {
            tags.push("Runs on its own".to_string());
        }
        let mut text = String::new();
        if let Some(place) = self.place_label(&entry.id) {
            text.push_str(&format!("{place}\n"));
        }
        text.push_str(&tags.join(" · "));
        text.push_str("\n\n");
        text.push_str(if entry.description.is_empty() {
            "This entry has no description."
        } else {
            &entry.description
        });
        if let Some(status) = self.statuses.get(&entry.id) {
            let detail = status.detail();
            text.push_str(&format!("\n\nStatus: {}", status.label()));
            if !detail.is_empty() {
                text.push_str(&format!("\nInstalled from: {detail}"));
            }
            if let Status::Update { .. } = status {
                text.push_str(
                    "\nThe repository metadata on this machine has a newer version: u updates it (on Arch, with the whole system).",
                );
            }
        }
        if let Action::Profile { entries } = &entry.action
            && !entries.is_empty()
        {
            text.push_str("\n\nRuns, in order:");
            for (number, id) in entries.iter().enumerate() {
                let name = self
                    .catalog
                    .find(id)
                    .map_or(id.as_str(), |found| found.name.as_str());
                text.push_str(&format!("\n  {}. {name}", number + 1));
            }
            text.push_str("\n\ne puts its entries in the Install Plan one by one, to change the plan before it runs.");
        }
        let meanings = flag_meanings(&entry.flags);
        if !meanings.is_empty() {
            text.push_str("\n\nWhat it does:");
            for (flag, meaning) in meanings {
                text.push_str(&format!("\n  {flag:<4}{meaning}"));
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
                        "{} ({} entries) is saved as {}.\n\nIt is under Profiles, Your profiles: Enter runs it, Space queues it, x exports it to a file to share.",
                        profile.name,
                        profile.entries.len(),
                        path.display(),
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
        if let Some(diagnosis) = self.selected_diagnosis().cloned() {
            self.show_diagnosis(&diagnosis);
        }
    }

    /// A check in full: what was found, what to do, and what f runs.
    fn show_diagnosis(&mut self, diagnosis: &doctor::Diagnosis) {
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
                "\nRepair: {}. f runs it after its own confirmation:\n",
                repair.label
            ));
            for command in &repair.commands {
                text.push_str(&format!("  {}\n", command.join(" ")));
            }
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
        if diagnosis.id == "snapshots" && self.screen == Screen::Recovery {
            text.push_str(
                "\nc lists the snapshots as root and looks inside the newest, reading only.\n",
            );
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
            // Queued and pinned, marked as in the entry lists.
            let queued = self.is_queued(&entry);
            let marks = format!(
                "{}{}",
                if queued { " *" } else { "" },
                if self.favorites.contains(&entry.id) {
                    " ★"
                } else {
                    ""
                }
            );
            let status = self
                .statuses
                .get(&entry.id)
                .map(|status| format!("  {}", status.label()))
                .unwrap_or_default();
            items.push(Line::from(vec![
                Span::styled("Report  ", Style::default().fg(self.theme.group())),
                Span::styled(
                    format!("{}{marks}", entry.name),
                    if queued {
                        Style::default().fg(self.theme.focused()).bold()
                    } else {
                        Style::default().fg(self.theme.entry())
                    },
                ),
                Span::styled(status, Style::default().fg(self.theme.muted())),
            ]));
        }
        self.fill(frame, inner);
        let title = if self.diagnoses.is_empty() {
            "Checks".to_string()
        } else {
            let count = |health: doctor::Health| {
                self.diagnoses
                    .iter()
                    .filter(|diagnosis| diagnosis.health == health)
                    .count()
            };
            let (problems, notices) = (
                count(doctor::Health::Problem),
                count(doctor::Health::Notice),
            );
            format!(
                "Checks · {problems} {} · {notices} {} · {} OK{}",
                if problems == 1 { "problem" } else { "problems" },
                if notices == 1 { "notice" } else { "notices" },
                count(doctor::Health::Good),
                if self.examining.is_some() {
                    " (checking again…)"
                } else {
                    ""
                }
            )
        };
        let list = List::new(self.measure(items))
            .block(self.card(&title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
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

    /// Package entries whose status says an update waits.
    fn entries_with_updates(&self) -> Vec<&Entry> {
        self.catalog
            .entries()
            .into_iter()
            .filter(|entry| matches!(self.statuses.get(&entry.id), Some(Status::Update { .. })))
            .collect()
    }

    /// What the selected row of the Updates screen is: a channel, then the
    /// entries with an update, then the row that updates them all.
    fn selected_update_row(&self) -> UpdateRow {
        let Some(index) = self.main_state.selected() else {
            return UpdateRow::Nothing;
        };
        if index < self.channels.len() {
            return UpdateRow::Channel;
        }
        let waiting = self.entries_with_updates();
        match waiting.get(index - self.channels.len()) {
            Some(entry) => UpdateRow::Entry(Box::new((*entry).clone())),
            None if !waiting.is_empty() && index == self.channels.len() + waiting.len() => {
                UpdateRow::Every
            }
            None => UpdateRow::Nothing,
        }
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
                    helix_toolbox_core::printable(&item.name),
                    helix_toolbox_core::printable(&versions)
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
                    "No package manager, Flatpak or Nix setup Helix Toolbox knows was found. r looks again, online."
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
        let waiting = self.entries_with_updates();
        if !waiting.is_empty() {
            let width = waiting
                .iter()
                .map(|entry| entry.name.chars().count())
                .max()
                .unwrap_or(0)
                .max(name_width.saturating_sub(8))
                + 2;
            for entry in &waiting {
                let detail = self
                    .statuses
                    .get(&entry.id)
                    .map(|status| status.detail())
                    .unwrap_or_default();
                items.push(Line::from(vec![
                    Span::styled("Entry   ", Style::default().fg(self.theme.group())),
                    Span::styled(
                        format!("{:<width$}", entry.name),
                        Style::default().fg(self.theme.entry()),
                    ),
                    Span::styled("Update  ", Style::default().fg(self.theme.warning()).bold()),
                    Span::styled(detail, Style::default().fg(self.theme.muted())),
                ]));
            }
            items.push(Line::styled(
                format!("▶ Update every entry ({})", waiting.len()),
                Style::default().fg(self.theme.focused()).bold(),
            ));
        }
        let title = match (self.gathering.is_some(), self.channels_online) {
            (true, _) if !self.channels.is_empty() => "Channels (looking again…)",
            (_, true) => "Channels · checked online",
            _ => "Channels",
        };
        self.fill(frame, inner);
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
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
        let actions: [(&str, &str, Act); 18] = [
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
            ("Import a profile", "profile file", Act::ImportProfile),
            ("Make a bug report", "issue problem report", Act::Report),
            (
                "Update every entry with an update",
                "upgrade packages",
                Act::UpdateEntries,
            ),
            (
                "Check again what is installed",
                "status refresh packages installed",
                Act::CheckStatuses,
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
            (
                if self.mouse {
                    "Turn the mouse off"
                } else {
                    "Turn the mouse on"
                },
                "mouse click wheel scroll select text",
                Act::Mouse,
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
            "Quit Helix Toolbox".to_string(),
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
                    "A name for the baseline (it is saved in ~/.config/helix-toolbox/baselines):",
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
                        "A name for this profile (it is saved in ~/.config/helix-toolbox/profiles):",
                    )));
                }
            }
            Act::ImportProfile => {
                if let Some(index) = self
                    .catalog
                    .tabs
                    .iter()
                    .position(|tab| tab.id == "profiles")
                {
                    self.open_screen(Screen::Area(index));
                    self.focus = Focus::Main;
                    self.handle_main_key(&KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE));
                }
            }
            Act::Report => self.make_report(),
            Act::CheckStatuses => self.check_again(),
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
            Act::Mouse => {
                self.mouse = !self.mouse;
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
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
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

    /// Review every entry that puts back something missing as one run, as
    /// the window's Run now does; nothing runs before the confirmation.
    fn run_missing(&mut self) {
        let Some((_, report)) = &self.drift_report else {
            return;
        };
        let (restore, _) =
            crate::drift::available(&report.restore(), &self.catalog, &self.platform);
        let entries: Vec<Entry> = restore
            .iter()
            .filter_map(|id| self.catalog.find(id))
            .cloned()
            .collect();
        if entries.is_empty() {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to put back",
                "No entry here puts back what differs.",
                true,
            )));
            return;
        }
        self.confirm(entries);
    }

    /// Record this machine as a baseline and compare with it.
    fn save_baseline(&mut self, name: &str) {
        let Some(inventory) = &self.inventory else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Still checking",
                "Helix Toolbox is still checking what is installed; try again in a moment.",
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
                helix_toolbox_engine::preview::render_with(&scripts, arguments)
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
                        "{} is saved as {}: {} entries, {} configuration paths, {} settings and {} runs.\n\nDrift compares with it from now on; b chooses another. To compare another machine with it: helix-toolbox drift export \"{}\", then helix-toolbox drift check <file> there.",
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
                    helix_toolbox_core::printable(&baseline.name),
                    Style::default().fg(self.theme.entry()).bold(),
                ),
                Span::styled(
                    format!(
                        " · saved {} on {}",
                        timestamp(baseline.taken),
                        helix_toolbox_core::printable(&baseline.system)
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
        if let Some((_, report)) = &self.drift_report
            && target.is_some()
        {
            if !report.unknown.is_empty() {
                lines.push(Line::styled(
                    format!("Not compared: {}", report.unknown.join(", ")),
                    Style::default().fg(self.theme.muted()),
                ));
            }
            let (_, unavailable) =
                crate::drift::available(&report.restore(), &self.catalog, &self.platform);
            if !unavailable.is_empty() {
                let names: Vec<&str> = unavailable
                    .iter()
                    .map(|id| {
                        self.catalog
                            .find(id)
                            .map_or(id.as_str(), |entry| &entry.name)
                    })
                    .collect();
                lines.push(Line::styled(
                    format!("Not on this machine: {}", names.join(", ")),
                    Style::default().fg(self.theme.muted()),
                ));
            }
        }
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
            frame.render_widget(
                Paragraph::new("Everything recorded is as it was.")
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
                    // Was and now, as the window's columns show them.
                    Span::styled(
                        if difference.was.is_some() || difference.now.is_some() {
                            format!(
                                "  {} → {}",
                                difference.was.as_deref().unwrap_or("–"),
                                difference.now.as_deref().unwrap_or("–")
                            )
                        } else {
                            String::new()
                        },
                        Style::default().fg(self.theme.faint()),
                    ),
                    Span::styled(
                        if queued { "  · planned" } else { "" },
                        Style::default().fg(self.theme.success()),
                    ),
                ])
            })
            .collect();
        let list = List::new(self.measure(items))
            .block(self.card("Differences"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
    }

    /// The change preview of a plan.
    fn show_effects(&mut self, planned: &Plan, from_confirmation: bool) {
        let scripts = self.catalog.root().join("scripts");
        let render =
            |arguments: &[String]| helix_toolbox_engine::preview::render_with(&scripts, arguments);
        let look = helix_toolbox_engine::preview::Look {
            system: &doctor::Here,
            render: &render,
            pinned: &|name| helix_toolbox_engine::preview::pinned_source(&scripts, name),
        };
        let steps = helix_toolbox_engine::preview::preview(
            planned,
            &|id| self.catalog.find(id),
            &self.platform,
            &look,
        );
        let download = helix_toolbox_engine::preview::sources_for(planned)
            .into_iter()
            .any(|name| {
                matches!(
                    helix_toolbox_engine::preview::pinned_source(&scripts, name),
                    helix_toolbox_engine::preview::Pinned::Missing { .. }
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
        for name in helix_toolbox_engine::preview::sources_for(planned) {
            if let Err(why) = helix_toolbox_engine::preview::fetch_source(&scripts, name) {
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
        let setup = match helix_toolbox_engine::setup::load(self.catalog.root()) {
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
            helix_toolbox_engine::setup::survey(
                &self.platform,
                &desktop,
                Vec::new(),
                false,
                &|_| false,
            )
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
        let Ok(setup) = helix_toolbox_engine::setup::load(self.catalog.root()) else {
            return Vec::new();
        };
        let desktop = self.desktop_here();
        let survey = if self.examine_here {
            crate::setup::survey_here(&self.platform, &desktop)
        } else {
            helix_toolbox_engine::setup::survey(
                &self.platform,
                &desktop,
                Vec::new(),
                false,
                &|_| false,
            )
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
        helix_toolbox_engine::setup::suggestions(&ids, &self.catalog, &self.platform)
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

    fn desktop_here(&self) -> helix_toolbox_engine::desktop::Desktop {
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
            .or_else(helix_toolbox_engine::desktop::dir)
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
            .map(|directory| helix_toolbox_engine::desktop::load_all(&directory).0)
            .unwrap_or_default();
        if !looks.is_empty() {
            rows.push(DesktopRow::Heading("Saved looks".to_string()));
        }
        for (slug, look) in looks {
            let other = found.kind.map(|kind| kind.id()) != Some(look.desktop.as_str());
            rows.push(DesktopRow::Look {
                text: format!(
                    "{}{}",
                    helix_toolbox_core::printable(&look.name),
                    if other {
                        format!(
                            "  (saved on {})",
                            helix_toolbox_core::printable(&look.desktop)
                        )
                    } else {
                        format!("  ({} settings)", look.settings.len())
                    }
                ),
                slug,
            });
        }
        let offered = helix_toolbox_engine::setup::load(self.catalog.root())
            .map(|setup| setup.for_desktop(found.kind))
            .unwrap_or_default();
        let offered: Vec<_> =
            helix_toolbox_engine::setup::suggestions(&offered, &self.catalog, &self.platform)
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
                note: suggestion.note,
            });
        }
        self.desktop = Some(found);
        self.desktop_rows = rows;
        self.clamp_main();
    }

    fn save_look_as(&mut self, name: &str) {
        let found = self.desktop_here();
        let saved = helix_toolbox_engine::desktop::capture(name, &found)
            .ok_or_else(|| {
                "no desktop whose appearance Helix Toolbox can read is running here".to_string()
            })
            .and_then(|look| {
                let directory = self
                    .looks_directory()
                    .ok_or_else(|| "cannot find the configuration folder".to_string())?;
                helix_toolbox_engine::desktop::save(&directory, &look)
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
        let Some((_, look)) = helix_toolbox_engine::desktop::load_all(&directory)
            .0
            .into_iter()
            .find(|(known, _)| known == slug)
        else {
            return;
        };
        let found = self.desktop_here();
        match helix_toolbox_engine::desktop::plan_apply(&look, &found) {
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
                DesktopRow::Entry { text, note, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", self.glyph(Icon::Entry)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(self.theme.entry())),
                    Span::styled(
                        note.as_ref()
                            .map(|note| format!("  {note}"))
                            .unwrap_or_default(),
                        Style::default().fg(self.theme.faint()),
                    ),
                ]),
                DesktopRow::Look { text, .. } => Line::from(vec![
                    Span::styled(
                        format!("  {} ", self.glyph(Icon::Entry)),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(text.clone(), Style::default().fg(self.theme.entry())),
                ]),
            })
            .collect();
        let list = List::new(self.measure(items))
            .block(self.card("This machine"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Hardware -----

    /// Read the hardware in the background; fwupd can take a moment.
    fn read_hardware(&mut self) {
        if !self.examine_here {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(helix_toolbox_engine::hardware::gather(&doctor::Here));
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
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
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
        if assessment.here.is_some() && assessment.level != reliability::Level::HardwareTested {
            text.push_str(
                "\nIt ran here: w reports that it worked, so that it can become Hardware Tested.\n",
            );
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
            helix_toolbox_engine::trust::Build {
                version: crate::VERSION.to_string(),
                release: false,
                commit: None,
                install: helix_toolbox_engine::updates::Install::Other,
            }
        };
        let found = helix_toolbox_engine::trust::inspect(
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
        // What could not be read of the record, as the window's warning card.
        let problems: Vec<String> = evidence
            .problems
            .iter()
            .map(|problem| format!("⚠ {problem}"))
            .collect();
        let rows: usize = [summary.as_str(), counts.as_str()]
            .into_iter()
            .chain(problems.iter().map(String::as_str))
            .map(|text| text.chars().count().div_ceil(width).max(1))
            .sum();
        let mut lines = vec![
            Line::from(summary),
            Line::styled(counts, Style::default().fg(self.theme.muted())),
        ];
        lines.extend(
            problems
                .into_iter()
                .map(|problem| Line::styled(problem, Style::default().fg(self.theme.warning()))),
        );
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
                    detail.push_str(&format!("ran here {}", helix_toolbox_engine::date(when)));
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
        let list = List::new(self.measure(items))
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
        bordered_list(frame, list, parts[2], &mut self.main_state, &self.hits);
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
            helix_toolbox_core::printable(&backup.original.display().to_string()),
            backup.taken.as_deref().unwrap_or("unknown"),
            helix_toolbox_core::printable(&backup.backup.display().to_string()),
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
            6 => self.change_watch(|watch| watch.updates = !watch.updates),
            7 => self.change_watch(|watch| watch.health = !watch.health),
            8 => {
                // The next saved baseline, then none.
                let slugs = self.baseline_slugs();
                self.change_watch(|watch| {
                    let next = match &watch.drift {
                        None => 0,
                        Some(slug) => slugs
                            .iter()
                            .position(|other| other == slug)
                            .map_or(0, |at| at + 1),
                    };
                    watch.drift = slugs.get(next).cloned();
                })
            }
            // Off, every day, every week, then off again.
            9 if self.examine_here => {
                let units = restic::units_here();
                let next = match units.as_deref().and_then(restic::timer) {
                    None => Some(restic::Every::Day),
                    Some(restic::Every::Day) => Some(restic::Every::Week),
                    Some(restic::Every::Week) => None,
                };
                self.set_restic_timer(next);
            }
            10 => self.open_wizard(false),
            11 => self.make_report(),
            13 => {
                self.mouse = !self.mouse;
                self.save_look();
            }
            _ => {}
        }
    }

    /// What the scheduled check watches; none where it cannot run.
    fn watching(&self) -> Option<Watch> {
        let places = self.notices.as_ref()?;
        self.settings_path.as_ref()?;
        Some(watch::read(places, self.settings_path.as_deref()))
    }

    /// The saved baselines' file names, for the Drift choice.
    fn baseline_slugs(&self) -> Vec<String> {
        self.drift_dir()
            .map(|directory| {
                drift::load_all(&directory)
                    .0
                    .into_iter()
                    .map(|(slug, _)| slug)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Change what the scheduled check watches, and its timer with it.
    fn change_watch(&mut self, how: impl FnOnce(&mut Watch)) {
        let (Some(places), Some(path)) = (self.notices.clone(), self.settings_path.clone()) else {
            return;
        };
        let before = watch::read(&places, Some(&path));
        let mut chosen = before.clone();
        how(&mut chosen);
        if chosen == before {
            return;
        }
        let applied = std::env::current_exe()
            .map_err(|error| format!("cannot tell where this program is: {error}"))
            .and_then(|program| {
                watch::apply(
                    &places,
                    &path,
                    &chosen,
                    &program,
                    notifications::window_here().as_deref(),
                    &self.systemctl,
                )
            });
        let turned_on = (chosen.updates && !before.updates)
            || (chosen.health && !before.health)
            || (chosen.drift.is_some() && chosen.drift != before.drift);
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
        let text = match applied {
            Err(error) => format!("They could not be turned on or off: {error}"),
            Ok(Some(notifications::Started::AtLogin)) => "The checks are set up, but systemd's user manager did not answer, so they start at your next login.".to_string(),
            Ok(_) if !turned_on => return,
            Ok(_) => format!(
                "Helix Toolbox now checks {}, and soon after you log in, and shows a notification when {}. It never updates, repairs or puts back anything itself.\n\nThe same settings turn it off again.",
                notifications::EVERY,
                what.join(", or when ")
            ),
        };
        self.popup = Some(Popup::Text(TextPopup::new("Notifications", text, true)));
    }

    // ----- Profiles -----

    /// Whether the Profiles area is open.
    fn in_profiles(&self) -> bool {
        self.current_area()
            .is_some_and(|index| self.catalog.tabs[index].id == "profiles")
    }

    fn profiles_directory(&self) -> Option<PathBuf> {
        self.profiles_dir.clone().or_else(profiles::dir)
    }

    /// Read the user's profiles into the catalog again. The Profiles area
    /// stays in the group it was in, wherever that group moved, or goes
    /// back to the top when the group is gone.
    fn refresh_profiles(&mut self, directory: &Path) {
        let group_id = |catalog: &Catalog, index: usize, group: usize| match catalog.tabs[index]
            .items
            .get(group)
        {
            Some(Item::Group(group)) => Some(group.id.clone()),
            _ => None,
        };
        let area = self
            .catalog
            .tabs
            .iter()
            .position(|tab| tab.id == "profiles");
        let inside = area.and_then(|index| {
            let (group, _) = *self.areas[index].path.first()?;
            group_id(&self.catalog, index, group)
        });
        profiles::add_to(&mut self.catalog, directory);
        self.menu = build_menu(&self.catalog);
        if let (Some(index), Some(inside)) = (area, inside) {
            let moved = (0..self.catalog.tabs[index].items.len())
                .find(|&group| group_id(&self.catalog, index, group).as_deref() == Some(&inside));
            match moved {
                Some(group) => self.areas[index].path[0].0 = group,
                None => {
                    self.areas[index].path.clear();
                    self.main_state.select(Some(0));
                }
            }
        }
        self.update_rows();
    }

    /// One of the user's profiles by its catalog id: its file and what it
    /// holds.
    fn read_own_profile(&self, id: &str) -> Result<(PathBuf, profiles::Profile), String> {
        let directory = self
            .profiles_directory()
            .ok_or("cannot find the configuration folder")?;
        let slug = id
            .strip_prefix(profiles::PREFIX)
            .and_then(profiles::slug)
            .filter(|slug| id == format!("{}{slug}", profiles::PREFIX))
            .ok_or_else(|| {
                format!(
                    "{} is not one of your profiles",
                    helix_toolbox_core::printable(id)
                )
            })?;
        let path = directory.join(format!("{slug}.toml"));
        let profile = profiles::read(&path, &self.catalog).map_err(|error| error.to_string())?;
        Ok((path, profile))
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

    /// Add a profile file to the user's profiles, as `helix-toolbox profile
    /// import` does.
    fn import_profile(&mut self, text: &str) -> Result<String, String> {
        let path = typed_path(text)?;
        let profile = profiles::read(&path, &self.catalog).map_err(|error| error.to_string())?;
        let directory = self
            .profiles_directory()
            .ok_or("cannot find the configuration folder")?;
        let saved = profiles::save(&directory, &profile, &self.catalog)
            .map_err(|error| error.to_string())?;
        self.refresh_profiles(&directory);
        Ok(format!(
            "{} ({} entries) is imported as {}.\n\nIt is under Profiles, Your profiles: Enter runs it, Space queues it.",
            helix_toolbox_core::printable(&profile.name),
            profile.entries.len(),
            saved.display()
        ))
    }

    /// Write one of the user's profiles to a file, as `helix-toolbox profile
    /// export` does.
    fn export_profile(&mut self, id: &str, text: &str) -> Result<String, String> {
        let (_, profile) = self.read_own_profile(id)?;
        let path = typed_path(text)?;
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A profile is written to a .toml file.".to_string());
        }
        profiles::write(&path, &profile).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} is exported to {}.\n\nOn another machine: o in Profiles imports it, or helix-toolbox profile import.",
            helix_toolbox_core::printable(&profile.name),
            path.display()
        ))
    }

    /// Delete one of the user's profiles, as `helix-toolbox profile delete`
    /// does; it leaves the Install Plan and the favorites too.
    fn delete_profile(&mut self, id: &str) -> Result<String, String> {
        let (path, profile) = self.read_own_profile(id)?;
        std::fs::remove_file(&path)
            .map_err(|error| format!("cannot delete {}: {error}", path.display()))?;
        self.queue.retain(|entry| entry.id != id);
        if let Some(directory) = self.profiles_directory() {
            self.refresh_profiles(&directory);
        }
        Ok(format!(
            "{} is deleted.",
            helix_toolbox_core::printable(&profile.name)
        ))
    }

    // ----- Bug report -----

    /// Gather a bug report in the background: the health checks take a few
    /// seconds.
    fn make_report(&mut self) {
        let platform = self.platform.clone();
        let checks = self.examine_here;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(crate::report::markdown(&crate::report::gather(
                &platform, checks,
            )));
        });
        self.reporting = Some(receiver);
        self.popup = Some(Popup::Text(TextPopup::new(
            "Bug report",
            "Gathering the report; the health checks take a few seconds…",
            true,
        )));
    }

    /// Show the gathered report, and keep a copy to paste from.
    fn show_report(&mut self, report: &str) {
        let saved = self.reports_dir.as_ref().map(|directory| {
            let path = directory.join("bug-report.md");
            std::fs::create_dir_all(directory)
                .and_then(|()| std::fs::write(&path, report))
                .map(|()| path)
        });
        let kept = match saved {
            Some(Ok(path)) => {
                let shown = path.display().to_string();
                let home = std::env::var("HOME").unwrap_or_default();
                let shown = match shown.strip_prefix(&home) {
                    Some(rest) if home.len() > 1 => format!("~{rest}"),
                    _ => shown,
                };
                format!("It is saved as {shown}, to copy from or attach.\n\n")
            }
            Some(Err(error)) => format!("It could not be saved: {error}\n\n"),
            None => String::new(),
        };
        let text = format!(
            "Paste it into a new issue at github.com/Commanderx-code/Helix-toolbox/issues, with what happened and how to make it happen again. It shows your home folder as ~ and holds no host name or logs, but read it before posting. {kept}{report}"
        );
        // Over anything but a run or a question.
        if matches!(self.popup, None | Some(Popup::Text(_))) {
            self.popup = Some(Popup::Text(
                TextPopup::new("Bug report", text, true).copying(report),
            ));
        }
    }

    // ----- Checks -----

    /// Check the selected entry now, read-only: what it installs and its
    /// checks of files, settings and services.
    fn check_selected(&mut self) {
        let Some(entry) = self.selected_entry().cloned() else {
            return;
        };
        let rebooted = helix_toolbox_engine::verification::rebooted_since(
            &doctor::Here,
            &self.run_history(),
            &entry.id,
        );
        let checks = helix_toolbox_engine::verification::check_entry(
            &entry,
            &self.platform,
            rebooted,
            &helix_toolbox_engine::Probe::holds,
            &helix_toolbox_platform::binary_exists,
            &doctor::Here,
        );
        let text = if checks.is_empty() {
            format!(
                "{} declares no checks and installs nothing Helix Toolbox can find, so there is nothing to check.",
                entry.name
            )
        } else {
            let failed = checks.iter().filter(|check| !check.passed).count();
            let mut text = match failed {
                0 => "Everything that can be checked here is as it should be.\n\n".to_string(),
                1 => "One check found it not as it should be.\n\n".to_string(),
                count => format!("{count} checks found it not as it should be.\n\n"),
            };
            for check in &checks {
                text.push_str(&format!(
                    "{}: {}\n  {}\n",
                    check.label(),
                    check.name,
                    check.detail
                ));
            }
            text
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("Checks: {}", entry.name),
            text,
            true,
        )));
    }

    // ----- Hardware reports -----

    /// A report that a run worked on this machine, to read, then send as an
    /// issue from the browser.
    fn report_worked(&mut self, record: &history::Record, only: Option<&str>) {
        let machine = match (&self.machine, self.examine_here) {
            (Some(machine), _) => machine.clone(),
            (None, true) => hardware_report::machine(
                &doctor::Here,
                &self.platform,
                &crate::setup::desktop_here(),
            ),
            (None, false) => hardware_report::Machine::default(),
        };
        let report = match hardware_report::prepare(record, only, machine, crate::VERSION, "") {
            Ok(report) => report,
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("No hardware report", why, true)));
                return;
            }
        };
        let url = report.issue_url();
        let text = report.text();
        if let Some(directory) = &self.reports_dir {
            let kept = std::fs::create_dir_all(directory).and_then(|()| {
                std::fs::write(
                    directory.join("hardware-report.md"),
                    format!("{text}\nThe issue, filled in:\n\n{url}\n"),
                )
            });
            if let Err(error) = kept {
                tracing::warn!(%error, "the hardware report was not saved");
            }
        }
        self.popup = Some(Popup::HardwareReport(
            TextPopup::new(
                "Hardware report",
                format!(
                    "{text}\no opens the issue in your browser, filled in; GitHub asks you to sign in, and the form asks what you checked. Nothing is sent until you submit it there. c copies the report."
                ),
                true,
            )
            .copying(format!("{text}\nThe issue, filled in:\n\n{url}\n")),
            url,
        ));
    }

    // ----- Services -----

    /// Read the units and the autostart entries again; quick, so not in the
    /// background. Tests keep what they put there.
    fn reload_services(&mut self) {
        if self.examine_here {
            let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok();
            self.services_found = Some(services::gather(&doctor::Here, desktop.as_deref()));
        }
        self.build_service_rows();
    }

    fn unit(&self, index: usize) -> Option<&services::Unit> {
        self.services_found.as_ref()?.units.get(index)
    }

    fn selected_service_row(&self) -> Option<ServiceRow> {
        self.service_rows.get(self.main_state.selected()?).cloned()
    }

    /// The rows of the chosen view, those the filter lets through.
    fn build_service_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(found) = &self.services_found {
            let filter = self.service_filter.to_lowercase();
            let wanted = |text: &str, more: &str| {
                filter.is_empty()
                    || text.to_lowercase().contains(&filter)
                    || more.to_lowercase().contains(&filter)
            };
            if self.service_view == ServiceView::Login {
                rows.push(ServiceRow::Heading(format!(
                    "Started at login ({})",
                    found.autostart.len()
                )));
                rows.extend(
                    found
                        .autostart
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| wanted(&item.name, &item.file))
                        .map(|(index, _)| ServiceRow::Autostart(index)),
                );
            } else {
                let units: Vec<usize> = found
                    .units
                    .iter()
                    .enumerate()
                    .filter(|(_, unit)| match self.service_view {
                        ServiceView::System => !unit.user && unit.kind() == "service",
                        ServiceView::Timers => unit.kind() != "service",
                        ServiceView::User => unit.user && unit.kind() == "service",
                        ServiceView::Failed => unit.failed(),
                        ServiceView::Login => false,
                    })
                    .filter(|(_, unit)| wanted(&unit.name, &unit.description))
                    .map(|(index, _)| index)
                    .collect();
                rows.push(ServiceRow::Heading(format!(
                    "{} ({})",
                    match self.service_view {
                        ServiceView::System => "The machine's services",
                        ServiceView::Timers => "Timers and sockets",
                        ServiceView::User => "Your session's services",
                        ServiceView::Failed => "Failed units",
                        ServiceView::Login => "",
                    },
                    units.len()
                )));
                rows.extend(units.into_iter().map(ServiceRow::Unit));
            }
        }
        self.service_rows = rows;
        self.clamp_main();
    }

    fn describe_unit(&mut self, index: usize) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        let needed = if self.examine_here {
            services::needed_by(&doctor::Here, &unit.name, unit.user)
        } else {
            Vec::new()
        };
        let mut text = format!(
            "{}\n\nEnabled: {}\nRunning: {}{}\n",
            if unit.description.is_empty() {
                "No description."
            } else {
                unit.description.as_str()
            },
            unit.enablement
                .as_deref()
                .unwrap_or("no unit file of its own"),
            unit.active.as_deref().unwrap_or("not loaded"),
            unit.sub
                .as_deref()
                .filter(|sub| Some(*sub) != unit.active.as_deref())
                .map(|sub| format!(" ({sub})"))
                .unwrap_or_default()
        );
        if !needed.is_empty() {
            text.push_str(&format!("Needed by: {}\n", needed.join(", ")));
        }
        if let Some(cost) = services::consequence(
            &unit.name,
            unit.user,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ) {
            text.push_str(&format!("\nBefore stopping or disabling it: {cost}\n"));
        }
        if !unit.switchable() {
            text.push_str(
                "\nIt is not enabled or disabled itself: another unit or a request starts it.\n",
            );
        }
        text.push_str(
            "\ns starts or stops it, e enables or disables it, R restarts it; each asks first.",
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("{}{}", if unit.user { "Your " } else { "" }, unit.name),
            text,
            true,
        )));
    }

    /// Plan starting, stopping, restarting, enabling or disabling a unit.
    fn change_unit(&mut self, index: usize, verb: Verb) {
        let Some(unit) = self.unit(index).cloned() else {
            return;
        };
        let needed = if self.examine_here {
            services::needed_by(&doctor::Here, &unit.name, unit.user)
        } else {
            Vec::new()
        };
        match services::plan(
            &unit.name,
            unit.user,
            verb,
            &needed,
            std::env::var_os("SSH_CONNECTION").is_some(),
        ) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    /// An application started at login, as the window's side pane shows it.
    fn describe_autostart(&mut self, index: usize) {
        let Some(item) = self
            .services_found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
        else {
            return;
        };
        let mut text = String::new();
        if !item.comment.is_empty() {
            text.push_str(&format!("{}\n\n", item.comment));
        }
        text.push_str(&format!(
            "Runs: {}\nFrom: {}{}\nAt login: {}\n\n",
            if item.exec.is_empty() {
                "nothing named"
            } else {
                &item.exec
            },
            if item.system {
                "the system"
            } else {
                "your own"
            },
            if item.own.is_some() && item.system {
                " · your file overrides it"
            } else {
                ""
            },
            if !item.here {
                "not in this desktop"
            } else if item.enabled {
                "started"
            } else {
                "not started"
            }
        ));
        text.push_str(&format!(
            "Enter plans {}, reviewed before it changes anything. For you only: the system's file stays as it is.",
            if item.enabled {
                "not starting it at login"
            } else {
                "starting it at login"
            }
        ));
        self.popup = Some(Popup::Text(TextPopup::new(item.name.clone(), text, true)));
    }

    /// Plan turning an autostart entry off, or on again.
    fn switch_autostart(&mut self, index: usize) {
        let Some(item) = self
            .services_found
            .as_ref()
            .and_then(|found| found.autostart.get(index))
            .cloned()
        else {
            return;
        };
        let dirs = services::AutostartDirs::here(&doctor::Here);
        match services::plan_autostart(&item, !item.enabled, &dirs, &doctor::Here) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    fn draw_services(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Services",
            "systemd's units and what starts at login; each change asks first and says what it costs.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        let mut title = format!("Showing {}", self.service_view.label());
        if !self.service_filter.is_empty() {
            title.push_str(&format!(", with \"{}\"", self.service_filter));
        }
        match &self.services_found {
            None => items.push(Line::styled(
                "Press r to read this machine's services.",
                Style::default().fg(self.theme.muted()),
            )),
            Some(found) if !found.systemd && self.service_view != ServiceView::Login => {
                items.push(Line::styled(
                    "systemd does not run this machine, so there are no units; i shows what starts at login.",
                    Style::default().fg(self.theme.muted()),
                ))
            }
            Some(found) => {
                let width = inner.width.saturating_sub(8) as usize;
                for row in &self.service_rows {
                    items.push(match row {
                        ServiceRow::Heading(text) => Line::styled(
                            text.clone(),
                            Style::default().fg(self.theme.group()).bold(),
                        ),
                        ServiceRow::Unit(index) => {
                            let unit = &found.units[*index];
                            let state = format!(
                                "{:<9} {:<9}",
                                unit.enablement.as_deref().unwrap_or("-"),
                                unit.active.as_deref().unwrap_or("-")
                            );
                            let color = if unit.failed() {
                                self.theme.failure()
                            } else if unit.running() {
                                self.theme.success()
                            } else {
                                self.theme.muted()
                            };
                            let name: String = unit.name.chars().take(40).collect();
                            let room = width.saturating_sub(42 + state.len());
                            Line::from(vec![
                                Span::styled(format!("  {name:<40} "), Style::default().fg(self.theme.entry())),
                                Span::styled(state, Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", unit.description.chars().take(room).collect::<String>()),
                                    Style::default().fg(self.theme.muted()),
                                ),
                            ])
                        }
                        ServiceRow::Autostart(index) => {
                            let item = &found.autostart[*index];
                            let (word, color) = match (item.here, item.enabled) {
                                (false, _) => ("not here", self.theme.muted()),
                                (true, true) => ("on", self.theme.success()),
                                (true, false) => ("off", self.theme.warning()),
                            };
                            Line::from(vec![
                                Span::styled(format!("  {:<36} ", item.name), Style::default().fg(self.theme.entry())),
                                Span::styled(format!("{word:<9}"), Style::default().fg(color)),
                                Span::styled(
                                    format!("  {}", item.exec.chars().take(width.saturating_sub(48)).collect::<String>()),
                                    Style::default().fg(self.theme.muted()),
                                ),
                            ])
                        }
                    });
                }
            }
        }
        let list = List::new(self.measure(items))
            .block(self.card(&title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Storage -----

    /// Read the storage in the background: sizing caches takes a moment.
    fn read_storage(&mut self) {
        if !self.examine_here {
            self.build_storage_rows();
            return;
        }
        let manager = self.platform.manager;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(storage::gather(&doctor::Here, manager));
        });
        self.reading_storage = Some(receiver);
    }

    fn build_storage_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(found) = &self.storage_found {
            rows.push(StorageRow::Heading("Space".to_string()));
            for filesystem in &found.filesystems {
                rows.push(StorageRow::Filesystem(format!(
                    "{}: {} free of {}",
                    filesystem.path,
                    storage::shown(filesystem.free),
                    storage::shown(filesystem.total)
                )));
            }
            rows.push(StorageRow::Heading("What can be cleaned".to_string()));
            rows.extend((0..found.items.len()).map(StorageRow::Item));
            if !found.kept.is_empty() {
                rows.push(StorageRow::Heading("Kept, never cleaned here".to_string()));
                rows.extend((0..found.kept.len()).map(StorageRow::Kept));
            }
        }
        self.storage_rows = rows;
        self.clamp_main();
    }

    fn selected_storage_row(&self) -> Option<StorageRow> {
        self.storage_rows.get(self.main_state.selected()?).cloned()
    }

    fn describe_storage(&mut self) {
        let Some(found) = &self.storage_found else {
            return;
        };
        let (title, text) = match self.selected_storage_row() {
            Some(StorageRow::Item(index)) => {
                let item = &found.items[index];
                let mut text = item.detail.clone();
                if let Some(size) = item.size {
                    text.push_str(&format!("\n\nIt takes {}.", storage::shown(size)));
                }
                match (item.frees, &item.not_offered) {
                    (_, Some(why)) => text.push_str(&format!("\nNot offered: {why}.")),
                    (Some(frees), None) => text.push_str(&format!(
                        "\nCleaning it frees about {}. Enter shows the plan, then asks.",
                        storage::shown(frees)
                    )),
                    (None, None) => text.push_str(
                        "\nWhat cleaning it frees is known only when it runs. Enter shows the plan, then asks.",
                    ),
                }
                if !item.packages.is_empty() {
                    text.push_str(&format!("\n\nPackages: {}", item.packages.join(", ")));
                }
                (item.title.clone(), text)
            }
            Some(StorageRow::Kept(index)) => {
                let kept = &found.kept[index];
                (kept.title.clone(), kept.detail.clone())
            }
            _ => return,
        };
        self.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    fn clean_selected(&mut self) {
        let Some(StorageRow::Item(index)) = self.selected_storage_row() else {
            if matches!(self.selected_storage_row(), Some(StorageRow::Kept(_))) {
                self.describe_storage();
            }
            return;
        };
        let Some(item) = self
            .storage_found
            .as_ref()
            .and_then(|found| found.items.get(index))
            .cloned()
        else {
            return;
        };
        match storage::plan(&item, self.platform.manager, &doctor::Here) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(item.title, why, true)));
            }
        }
    }

    fn draw_storage(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Storage",
            "Where the space goes and what can be cleaned; each cleanup asks first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.storage_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_storage.is_some() {
                    "Reading the storage…"
                } else {
                    "Press r to read this machine's storage."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        if let Some(found) = &self.storage_found {
            for row in &self.storage_rows {
                items.push(match row {
                    StorageRow::Heading(text) => {
                        Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                    }
                    StorageRow::Filesystem(text) => {
                        Line::styled(format!("  {text}"), Style::default().fg(self.theme.entry()))
                    }
                    StorageRow::Item(index) => {
                        let item = &found.items[*index];
                        let size = item.size.map(storage::shown).unwrap_or_else(|| "?".into());
                        let (said, color) = match (&item.not_offered, item.frees) {
                            (Some(why), _) => (why.clone(), self.theme.muted()),
                            (None, Some(frees)) => (
                                format!("frees {}", storage::shown(frees)),
                                self.theme.success(),
                            ),
                            (None, None) => {
                                ("frees what it finds".to_string(), self.theme.success())
                            }
                        };
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", item.title),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(
                                format!("{size:>9}  "),
                                Style::default().fg(self.theme.muted()),
                            ),
                            Span::styled(said, Style::default().fg(color)),
                        ])
                    }
                    StorageRow::Kept(index) => {
                        let kept = &found.kept[*index];
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<26} ", kept.title),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(
                                kept.size.map(storage::shown).unwrap_or_default(),
                                Style::default().fg(self.theme.muted()),
                            ),
                        ])
                    }
                });
            }
        }
        let list = List::new(self.measure(items))
            .block(self.card("Storage"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Performance -----

    /// Read the boot and the machine in the background, recording this boot.
    fn read_performance(&mut self) {
        if !self.examine_here {
            self.build_performance_rows();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let state = history::state_dir();
            let history = history::read(1000).unwrap_or_default();
            let found = performance::gather(&doctor::Here, state.as_deref(), &history);
            let boots = state
                .map(|state| performance::load(&performance::boots_path(&state)))
                .unwrap_or_default();
            let _ = sender.send((found, boots));
        });
        self.reading_performance = Some(receiver);
    }

    /// The recorded boots before this one, newest first.
    fn earlier_boots(&self) -> Vec<&performance::Boot> {
        let Some((found, boots)) = &self.performance_found else {
            return Vec::new();
        };
        let Some(boot) = &found.boot else {
            return Vec::new();
        };
        boots
            .iter()
            .filter(|other| other.id != boot.id && other.booted <= boot.booted)
            .collect()
    }

    /// This boot beside the chosen earlier one.
    fn performance_comparison(&self) -> Option<performance::Comparison> {
        let (found, _) = self.performance_found.as_ref()?;
        if self.performance_against == 0 {
            return found.previous.clone();
        }
        let before = self
            .earlier_boots()
            .get(self.performance_against)
            .copied()?;
        let history = if self.examine_here {
            history::read(1000).unwrap_or_default()
        } else {
            Vec::new()
        };
        Some(performance::compare(before, found.boot.as_ref()?, &history))
    }

    fn build_performance_rows(&mut self) {
        let mut rows = Vec::new();
        let comparison = self.performance_comparison();
        if let Some((found, _)) = &self.performance_found {
            rows.push(PerformanceRow::Heading("This boot".into()));
            match &found.boot {
                Some(boot) => {
                    rows.push(PerformanceRow::Text(format!(
                        "{}, kernel {}: {}",
                        timestamp(boot.booted),
                        boot.kernel,
                        performance::shown(boot.phases.total)
                    )));
                    rows.push(PerformanceRow::Text(
                        boot.phases
                            .parts()
                            .iter()
                            .map(|(name, took)| format!("{name} {}", performance::shown(*took)))
                            .collect::<Vec<_>>()
                            .join(" · "),
                    ));
                }
                None => rows.push(PerformanceRow::Text(
                    found.no_boot.clone().unwrap_or_default(),
                )),
            }
            match &comparison {
                Some(compared) => {
                    rows.push(PerformanceRow::Heading(format!(
                        "Against the boot of {}: {}",
                        timestamp(compared.before.booted),
                        performance::shown_change(compared.total())
                    )));
                    for change in compared.phases.iter().skip(1).chain(&compared.units) {
                        let now = change
                            .after
                            .map(performance::shown)
                            .unwrap_or_else(|| "not now".into());
                        let (how, slower) = match change.difference() {
                            Some(difference) if difference.unsigned_abs() < 100 => continue,
                            Some(difference) => {
                                (performance::shown_change(difference), difference > 0)
                            }
                            None if change.before.is_none() => ("new".to_string(), true),
                            None => ("gone".to_string(), false),
                        };
                        rows.push(PerformanceRow::Change(
                            format!("{:<40} {now:>10}  ({how})", change.name),
                            slower,
                        ));
                    }
                    if compared.before.kernel != compared.after.kernel
                        && !compared.before.kernel.is_empty()
                    {
                        rows.push(PerformanceRow::Text(format!(
                            "The kernel changed: {} to {}",
                            compared.before.kernel, compared.after.kernel
                        )));
                    }
                    if compared.runs.is_empty() {
                        rows.push(PerformanceRow::Text(
                            "No Helix Toolbox runs in between.".into(),
                        ));
                    }
                    for run in &compared.runs {
                        rows.push(PerformanceRow::Text(format!(
                            "Ran in between, {}: {}{}",
                            timestamp(run.at),
                            run.names.join(", "),
                            if run.success { "" } else { " (failed)" }
                        )));
                    }
                }
                None if found.boot.is_some() => rows.push(PerformanceRow::Text(
                    "No earlier boot is recorded yet; after the next boot, this one is compared with it."
                        .into(),
                )),
                None => {}
            }
            if let Some(boot) = &found.boot {
                let waited: Vec<&performance::Link> = boot
                    .chain
                    .iter()
                    .filter(|link| link.took.is_some())
                    .collect();
                if !waited.is_empty() {
                    rows.push(PerformanceRow::Heading(
                        "The login screen waited for".into(),
                    ));
                    for link in waited {
                        rows.push(PerformanceRow::Unit(
                            link.name.clone(),
                            performance::shown(link.took.unwrap_or(0)),
                        ));
                    }
                }
                if !boot.slowest.is_empty() {
                    rows.push(PerformanceRow::Heading(
                        "Slowest services since the boot".into(),
                    ));
                    for timed in &boot.slowest {
                        rows.push(PerformanceRow::Unit(
                            timed.name.clone(),
                            performance::shown(timed.took),
                        ));
                    }
                }
            }
            let now = &found.now;
            rows.push(PerformanceRow::Heading("Now".into()));
            if let Some([one, five, fifteen]) = now.load {
                rows.push(PerformanceRow::Text(format!(
                    "Load {one:.2} {five:.2} {fifteen:.2} on {} processors",
                    now.cpus
                )));
            }
            if now.memory_total > 0 {
                rows.push(PerformanceRow::Text(format!(
                    "Memory: {} free of {}",
                    storage::shown(now.memory_available),
                    storage::shown(now.memory_total)
                )));
            }
            rows.push(PerformanceRow::Text(if now.swaps.is_empty() {
                "Swap: none".to_string()
            } else {
                format!(
                    "Swap: {} used of {}{}",
                    storage::shown(now.swap_used()),
                    storage::shown(now.swap_total()),
                    if now.swaps.iter().any(|swap| swap.zram) {
                        " (zram)"
                    } else {
                        ""
                    }
                )
            }));
            let pressure: Vec<String> = [
                ("processor", now.pressure.cpu),
                ("memory", now.pressure.memory),
                ("disk", now.pressure.io),
            ]
            .into_iter()
            .filter_map(|(what, waited)| Some(format!("{what} {:.0}%", waited?)))
            .collect();
            if !pressure.is_empty() {
                rows.push(PerformanceRow::Text(format!(
                    "Waiting in the last minute: {}",
                    pressure.join(", ")
                )));
            }
            if let Some(governor) = &now.governor {
                rows.push(PerformanceRow::Text(format!(
                    "Governor {governor}{}",
                    now.power_profile
                        .as_ref()
                        .map(|profile| format!(", power profile {profile}"))
                        .unwrap_or_default()
                )));
            }
            if !now.heaviest.is_empty() {
                rows.push(PerformanceRow::Text(format!(
                    "Most memory: {}",
                    now.heaviest
                        .iter()
                        .take(5)
                        .map(|process| format!(
                            "{} {}",
                            helix_toolbox_core::printable(&process.name),
                            storage::shown(process.memory)
                        ))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
            if !found.hints.is_empty() {
                rows.push(PerformanceRow::Heading("Worth knowing".into()));
                rows.extend((0..found.hints.len()).map(PerformanceRow::Hint));
            }
        }
        self.performance_rows = rows;
        self.clamp_main();
    }

    fn selected_performance_row(&self) -> Option<PerformanceRow> {
        self.performance_rows
            .get(self.main_state.selected()?)
            .cloned()
    }

    fn selected_hint(&self) -> Option<performance::Hint> {
        let Some(PerformanceRow::Hint(index)) = self.selected_performance_row() else {
            return None;
        };
        let (found, _) = self.performance_found.as_ref()?;
        found.hints.get(index).cloned()
    }

    fn describe_performance(&mut self) {
        let (title, text) = match self.selected_performance_row() {
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint() else {
                    return;
                };
                let mut text = hint.detail.clone();
                if let Some(command) = &hint.command {
                    text.push_str(&format!("\n\n{command}\nEnter plans it here, then asks."));
                    if command.contains(" show ") {
                        text = text.replace(
                            "Enter plans it here, then asks.",
                            "Enter shows it in Services.",
                        );
                    }
                }
                if let Some(entry) = hint.entry.as_deref().and_then(|id| self.catalog.find(id)) {
                    text.push_str(&format!(
                        "\n\nThe {} entry sets it up; Enter runs it after confirmation.",
                        entry.name
                    ));
                }
                (hint.title, text)
            }
            Some(PerformanceRow::Unit(name, took)) => (
                name.clone(),
                format!(
                    "{name} took {took}.\n\nEnter shows it in Services, with what needs it and what stopping it costs."
                ),
            ),
            _ => return,
        };
        self.popup = Some(Popup::Text(TextPopup::new(title, text, true)));
    }

    /// Show a unit in the Services screen, filtered to it.
    fn show_unit(&mut self, name: &str) {
        self.service_filter = name.to_string();
        self.service_view = ServiceView::System;
        self.open_screen(Screen::Services);
    }

    /// Enter on a hint plans what changes it; on a unit, shows it in
    /// Services.
    fn act_on_performance(&mut self) {
        match self.selected_performance_row() {
            Some(PerformanceRow::Unit(name, _)) => self.show_unit(&name),
            Some(PerformanceRow::Hint(_)) => {
                let Some(hint) = self.selected_hint() else {
                    return;
                };
                match (&hint.unit, &hint.command, &hint.entry) {
                    (Some(unit), Some(command), _) if command.contains(" disable ") => {
                        let needed = if self.examine_here {
                            services::needed_by(&doctor::Here, unit, false)
                        } else {
                            Vec::new()
                        };
                        match services::plan(
                            unit,
                            false,
                            Verb::Disable,
                            &needed,
                            std::env::var_os("SSH_CONNECTION").is_some(),
                        ) {
                            Ok(planned) => self.confirm_plan(Ok(planned)),
                            Err(why) => {
                                self.popup =
                                    Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
                            }
                        }
                    }
                    (Some(unit), _, _) => {
                        let unit = unit.clone();
                        self.show_unit(&unit);
                    }
                    (None, _, Some(entry)) => match self.catalog.find(entry).cloned() {
                        Some(entry) => self.confirm(vec![entry]),
                        None => self.describe_performance(),
                    },
                    _ => self.describe_performance(),
                }
            }
            _ => {}
        }
    }

    fn draw_performance(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Performance",
            "How long the boot took and where the time went, and the machine now. It only reads.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let mut items: Vec<Line> = Vec::new();
        if self.performance_rows.is_empty() {
            items.push(Line::styled(
                if self.reading_performance.is_some() {
                    "Reading the boot…"
                } else {
                    "Press r to read this boot and the machine now."
                },
                Style::default().fg(self.theme.muted()),
            ));
        }
        let hints = self
            .performance_found
            .as_ref()
            .map(|(found, _)| found.hints.clone())
            .unwrap_or_default();
        for row in &self.performance_rows {
            items.push(match row {
                PerformanceRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                PerformanceRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(self.theme.entry()))
                }
                PerformanceRow::Change(text, slower) => Line::styled(
                    format!("  {text}"),
                    Style::default().fg(if *slower {
                        self.theme.warning()
                    } else {
                        self.theme.success()
                    }),
                ),
                PerformanceRow::Unit(name, took) => Line::from(vec![
                    Span::styled(
                        format!("  {took:>10}  "),
                        Style::default().fg(self.theme.muted()),
                    ),
                    Span::styled(name.clone(), Style::default().fg(self.theme.entry())),
                ]),
                PerformanceRow::Hint(index) => {
                    let hint = &hints[*index];
                    let offer = match (&hint.command, &hint.entry) {
                        (Some(command), _) if command.contains(" disable ") => {
                            "  · Enter turns it off"
                        }
                        (Some(_), _) => "  · Enter shows it",
                        (None, Some(_)) => "  · Enter sets it up",
                        _ => "  · d details",
                    };
                    Line::from(vec![
                        Span::styled(
                            format!("  {}", hint.title),
                            Style::default().fg(self.theme.warning()),
                        ),
                        Span::styled(offer, Style::default().fg(self.theme.focused())),
                    ])
                }
            });
        }
        let title = if self.reading_performance.is_some() && !self.performance_rows.is_empty() {
            "Boot and machine (reading again…)"
        } else {
            "Boot and machine"
        };
        let list = List::new(self.measure(items))
            .block(self.card(title))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Recovery -----

    fn snapshots_here(&self) -> Option<backups::Snapshots> {
        if self.examine_here {
            backups::Snapshots::here()
        } else {
            self.snapshot_tool
        }
    }

    /// The restic repository for the recovery checks, with its newest
    /// snapshot when the Restic screen read them already.
    fn restic_repository(&self) -> Option<recovery::Repository> {
        let reading = self.restic_reading.as_ref();
        match reading.map(|reading| &reading.found) {
            Some(restic::Found::Ready(config)) => Some(recovery::Repository {
                name: config.repository.clone(),
                newest: reading
                    .and_then(|reading| reading.snapshots.as_ref().ok()?.first())
                    .map(restic::Snapshot::when),
            }),
            Some(_) => None,
            None => restic::repository_here(),
        }
    }

    fn reload_recovery(&mut self) {
        if self.examine_here {
            let found_backups = backups::discover(&backups::Roots::here());
            let baselines = drift::dir()
                .map(|directory| drift::load_all(&directory).0)
                .unwrap_or_default();
            self.recovery_found = recovery::examine(
                &doctor::Here,
                &recovery::Known {
                    manager: self.platform.manager,
                    backups: &found_backups,
                    baselines: &baselines,
                    now: history::now(),
                    restic: self.restic_repository(),
                },
            );
        }
        self.clamp_main();
    }

    fn selected_readiness(&self) -> Option<&doctor::Diagnosis> {
        self.recovery_found.get(self.main_state.selected()?)
    }

    fn draw_recovery(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Recovery",
            "Whether this machine could be brought back after a bad update or a lost disk. It only reads.",
        );
        let focused = self.focus == Focus::Main;
        let title_width = self
            .recovery_found
            .iter()
            .map(|check| check.title.chars().count())
            .max()
            .unwrap_or(0)
            + 2;
        let can_check = self.snapshots_here().is_some();
        let mut items: Vec<Line> = Vec::new();
        if self.recovery_found.is_empty() {
            items.push(Line::styled(
                "Press r to check this machine.",
                Style::default().fg(self.theme.muted()),
            ));
        }
        for check in &self.recovery_found {
            let color = match check.health {
                doctor::Health::Good => self.theme.success(),
                doctor::Health::Notice => self.theme.warning(),
                doctor::Health::Problem => self.theme.failure(),
            };
            let offer = if check.id == "snapshots" && can_check {
                "  · c checks them"
            } else if check
                .entry
                .as_deref()
                .and_then(|id| self.catalog.find(id))
                .is_some()
            {
                "  · f sets it up"
            } else {
                ""
            };
            items.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", check.health.label()),
                    Style::default().fg(color).bold(),
                ),
                Span::styled(
                    format!("{:<title_width$}", check.title),
                    Style::default().fg(self.theme.entry()),
                ),
                Span::styled(
                    check.summary.clone(),
                    Style::default().fg(self.theme.muted()),
                ),
                Span::styled(offer, Style::default().fg(self.theme.focused())),
            ]));
        }
        self.fill(frame, inner);
        let list = List::new(self.measure(items))
            .block(self.card("Readiness"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused));
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Migration -----

    /// What planning asks of the machine in tests: nothing is installed
    /// but Flatpak, so what a test sees never depends on this machine.
    fn test_machine() -> Machine<'static> {
        Machine {
            holds: &|_: &Probe| false,
            on_path: &|tool: &str| tool == "flatpak",
            ..Machine::blind()
        }
    }

    /// Read in the background what a migration of this machine would hold.
    fn read_migration(&mut self) {
        if !self.examine_here {
            self.build_migration_rows();
            return;
        }
        let entries: Vec<Entry> = self.catalog.entries().into_iter().cloned().collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let refs: Vec<&Entry> = entries.iter().collect();
            // As migration::gather, keeping whether the packages chosen by
            // hand could be read.
            let inventory = Inventory::gather(&platform);
            let chosen = migration::chosen(platform.manager);
            let found = migration::export(
                &refs,
                &platform,
                &inventory,
                chosen.as_deref(),
                history::now(),
            );
            let _ = sender.send((found, chosen.is_some()));
        });
        self.reading_migration = Some(receiver);
        self.build_migration_rows();
    }

    /// Read this machine again, and the opened migration with it.
    fn reload_migration(&mut self) {
        self.read_migration();
        if let Some((path, _)) = self.migration_opened.clone() {
            self.open_migration(path);
        }
    }

    /// Read a migration file and what it means here, in the background.
    fn open_migration(&mut self, path: PathBuf) {
        if !self.examine_here {
            let read = migration::read(&path)
                .map(|read| {
                    migration::translate(
                        read,
                        &self.catalog.entries(),
                        &self.platform,
                        Self::test_machine(),
                        None,
                    )
                })
                .map_err(|error| error.to_string());
            self.opened_migration(path, read);
            return;
        }
        let entries: Vec<Entry> = self.catalog.entries().into_iter().cloned().collect();
        let platform = self.platform.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let refs: Vec<&Entry> = entries.iter().collect();
            let read =
                migration::read_here(&path, &refs, &platform).map_err(|error| error.to_string());
            let _ = sender.send((path, read));
        });
        self.opening_migration = Some(receiver);
        self.build_migration_rows();
    }

    fn opened_migration(&mut self, path: PathBuf, read: Result<Translation, String>) {
        match read {
            Ok(translation) => {
                self.migration_opened = Some((path, translation));
                self.build_migration_rows();
            }
            Err(error) => {
                self.build_migration_rows();
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot open the migration",
                    error,
                    true,
                )));
            }
        }
    }

    /// Write what this machine has to a file, as `helix-toolbox migrate
    /// export` does.
    fn export_migration(&mut self, text: &str) -> Result<String, String> {
        let path = typed_path(text)?;
        if path.extension().is_none_or(|extension| extension != "toml") {
            return Err("A migration is written to a .toml file.".to_string());
        }
        let found = match &self.migration_here {
            Some(found) => found.clone(),
            None if self.examine_here => {
                migration::gather(&self.catalog.entries(), &self.platform, history::now())
            }
            None => return Err("What this machine has is still being read.".to_string()),
        };
        migration::write(&path, &found).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} entries, {} Flatpaks and {} packages of yours are written to {}.\n\nOn the other machine: o in Migration opens it, or helix-toolbox migrate show.",
            found.entries.len(),
            found.flatpaks.len(),
            found.packages.len(),
            path.display()
        ))
    }

    /// Plan what the opened migration marks Install, to review and run.
    fn apply_migration(&mut self) {
        let Some((_, translation)) = &self.migration_opened else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "No migration open",
                "o opens a migration file made on the other machine; then a installs what it can here.",
                true,
            )));
            return;
        };
        if translation.count(Standing::Install) == 0 {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing to install",
                "Everything the migration names is here already, cannot be installed here, or is left to you.",
                true,
            )));
            return;
        }
        let entries = self.catalog.entries();
        let planned = if self.examine_here {
            migration::plan_here(translation, &entries, &self.platform)
        } else {
            migration::plan(translation, &entries, &self.platform, Self::test_machine())
        };
        self.confirm_plan(planned);
    }

    fn selected_migration_item(&self) -> Option<&migration::Item> {
        let MigrationRow::Item(index) = self.migration_rows.get(self.main_state.selected()?)?
        else {
            return None;
        };
        self.migration_opened.as_ref()?.1.items.get(*index)
    }

    fn describe_migration(&mut self) {
        let Some(item) = self.selected_migration_item() else {
            return;
        };
        let kind = migration_kind(item.kind);
        let next = match item.standing {
            Standing::Install => "a installs it with the rest, after you review the plan.",
            Standing::Here => "Nothing to do: it is here already.",
            Standing::Unavailable => "Helix Toolbox has no way to install it here.",
            Standing::Yours => {
                "It is left to you: install what this distribution calls it, if you want it."
            }
        };
        let text = format!(
            "{} ({kind}): {}.\n\n{next}",
            helix_toolbox_core::printable(&item.id),
            helix_toolbox_core::printable(&item.detail)
        );
        self.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&item.name),
            text,
            true,
        )));
    }

    fn build_migration_rows(&mut self) {
        let mut rows = vec![MigrationRow::Heading("This machine".into())];
        rows.push(MigrationRow::Text(match &self.migration_here {
            Some(found) => format!(
                "{} entries, {} Flatpaks and {} packages of yours, from {}",
                found.entries.len(),
                found.flatpaks.len(),
                found.packages.len(),
                found.from
            ),
            None if self.reading_migration.is_some() => "Reading what this machine has…".into(),
            None => "Press r to read what this machine has.".into(),
        }));
        if self.migration_here.is_some() && !self.migration_packages_known {
            rows.push(MigrationRow::Text(
                "The packages you installed yourself cannot be told apart here, so only the entries and Flatpaks go in the file.".into(),
            ));
        }
        rows.push(MigrationRow::Text(
            "e writes it to a file to take to another machine; o opens one made elsewhere.".into(),
        ));
        if self.opening_migration.is_some() {
            rows.push(MigrationRow::Heading("Opening the migration…".into()));
        }
        if let Some((path, translation)) = &self.migration_opened {
            let read = &translation.migration;
            rows.push(MigrationRow::Heading(format!(
                "From {} ({}), made {}{}",
                read.from,
                read.architecture,
                timestamp(read.made),
                if translation.same_family {
                    ", the same family"
                } else {
                    ""
                }
            )));
            rows.push(MigrationRow::Text(path.display().to_string()));
            for standing in [
                Standing::Install,
                Standing::Here,
                Standing::Unavailable,
                Standing::Yours,
            ] {
                let count = translation.count(standing);
                if count == 0 {
                    continue;
                }
                rows.push(MigrationRow::Heading(format!(
                    "{} ({count})",
                    match standing {
                        Standing::Install => "To install",
                        Standing::Here => "Here already",
                        Standing::Unavailable => "Cannot be installed here",
                        Standing::Yours => "Left to you",
                    }
                )));
                rows.extend(
                    translation
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| item.standing == standing)
                        .map(|(index, _)| MigrationRow::Item(index)),
                );
            }
        }
        self.migration_rows = rows;
        self.clamp_main();
    }

    fn draw_migration(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Migration",
            "Take this machine's applications to another machine or distribution. Exporting only reads.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let items_of = self
            .migration_opened
            .as_ref()
            .map(|(_, translation)| translation.items.as_slice())
            .unwrap_or_default();
        let name_width = items_of
            .iter()
            .map(|item| item.name.chars().count())
            .max()
            .unwrap_or(0)
            .min(36)
            + 2;
        let mut items: Vec<Line> = Vec::new();
        for row in &self.migration_rows {
            items.push(match row {
                MigrationRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                MigrationRow::Text(text) => {
                    Line::styled(format!("  {text}"), Style::default().fg(self.theme.muted()))
                }
                MigrationRow::Item(index) => {
                    let item = &items_of[*index];
                    let color = match item.standing {
                        Standing::Install => self.theme.entry(),
                        Standing::Here => self.theme.success(),
                        Standing::Unavailable => self.theme.failure(),
                        Standing::Yours => self.theme.warning(),
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "  {:<name_width$}",
                                helix_toolbox_core::printable(&item.name)
                            ),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            format!("{:<9}", migration_kind(item.kind)),
                            Style::default().fg(self.theme.muted()),
                        ),
                        Span::styled(
                            helix_toolbox_core::printable(&item.detail),
                            Style::default().fg(self.theme.muted()),
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Migration"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Home Manager -----

    /// Read Home Manager in the background: its generations, its
    /// configuration's repository and the files it links.
    fn read_home(&mut self) {
        if !self.examine_here {
            self.build_home_rows();
            return;
        }
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = sender.send(home_manager::status(&doctor::Here, &repos::Here));
        });
        self.reading_home = Some(receiver);
        self.build_home_rows();
    }

    fn selected_generation(&self) -> Option<&home_manager::Generation> {
        let HomeRow::Generation(index) = self.home_rows.get(self.main_state.selected()?)? else {
            return None;
        };
        self.home_status.as_ref()?.generations.get(*index)
    }

    fn selected_link(&self) -> Option<&home_manager::Link> {
        let HomeRow::Link(index) = self.home_rows.get(self.main_state.selected()?)? else {
            return None;
        };
        self.home_status.as_ref()?.links.get(*index)
    }

    /// The configuration to build, or a popup saying why there is none.
    fn home_config(&mut self) -> Option<home_manager::Config> {
        let why = match self.home_status.as_ref() {
            None => "Home Manager has not been read yet; r reads it.".to_string(),
            Some(status) if !status.installed => "home-manager is not installed here.".to_string(),
            Some(home_manager::Status {
                config: Some(config),
                ..
            }) => return Some(config.clone()),
            Some(home_manager::Status {
                problem: Some(problem),
                ..
            }) => problem.clone(),
            Some(_) => "No Home Manager configuration was found: name its folder as flake under [home_manager] in settings.toml.".to_string(),
        };
        self.popup = Some(Popup::Text(TextPopup::new("Home Manager", why, true)));
        None
    }

    fn confirm_home(&mut self, planned: Result<Plan, String>) {
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    fn home_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('b') => {
                if let Some(config) = self.home_config() {
                    self.confirm_home(Ok(home_manager::plan_build(&config)));
                }
            }
            KeyCode::Char('s') => {
                if let Some(config) = self.home_config() {
                    self.confirm_home(Ok(home_manager::plan_switch(&config)));
                }
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.home_enter(),
            KeyCode::Char('p') => self.pull_home(),
            KeyCode::Char('d') => self.describe_home(),
            KeyCode::Char('r') => self.read_home(),
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    /// Enter on an earlier generation plans going back to it; on anything
    /// else it tells of it.
    fn home_enter(&mut self) {
        match self.selected_generation().cloned() {
            Some(generation) if !generation.current => {
                let planned = home_manager::plan_activate(&doctor::Here, &generation);
                self.confirm_home(planned);
            }
            _ => self.describe_home(),
        }
    }

    /// Pull the configuration's repository, fast-forward only.
    fn pull_home(&mut self) {
        if self.home_config().is_none() {
            return;
        }
        let Some(repository) = self
            .home_status
            .as_ref()
            .and_then(|status| status.repository.clone())
        else {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Home Manager",
                "The configuration is not in a Git repository.",
                true,
            )));
            return;
        };
        if !self.examine_here {
            return;
        }
        let planned = repos::details(&repos::Here, &repository.path)
            .and_then(|details| repos::plan(&repos::Here, &details, &Change::Pull));
        self.confirm_home(planned);
    }

    fn describe_home(&mut self) {
        let (title, text) = if let Some(generation) = self.selected_generation() {
            (
                format!("Generation {}", generation.id),
                format!(
                    "Built {}.\n{}\n\n{}",
                    generation.built,
                    generation.path.display(),
                    if generation.current {
                        "It is the active one."
                    } else {
                        "Enter goes back to it through its own activate script, after you review the plan."
                    }
                ),
            )
        } else if let Some(link) = self.selected_link() {
            (
                link.name.clone(),
                match (&link.target, link.managed) {
                    (Some(target), true) => format!(
                        "{} links to {}.\n\nHome Manager manages it: change it in the configuration, then switch.",
                        link.path.display(),
                        target.display()
                    ),
                    (Some(target), false) => {
                        format!("{} links to {}.", link.path.display(), target.display())
                    }
                    (None, _) => format!(
                        "{} is a file of its own; Home Manager does not manage it.",
                        link.path.display()
                    ),
                },
            )
        } else {
            return;
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            helix_toolbox_core::printable(&title),
            helix_toolbox_core::printable(&text),
            true,
        )));
    }

    fn home_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        if self
            .selected_generation()
            .is_some_and(|generation| !generation.current)
        {
            list.push(Hint::new(&["Enter"], "Go back to it"));
        }
        if let Some(status) = &self.home_status
            && status.installed
            && status.config.is_some()
        {
            list.push(Hint::new(&["b"], "Build"));
            list.push(Hint::new(&["s"], "Switch"));
            if status.repository.is_some() {
                list.push(Hint::new(&["p"], "Pull"));
            }
        }
        if self.selected_generation().is_some() || self.selected_link().is_some() {
            list.push(Hint::new(&["d"], "Details"));
        }
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn build_home_rows(&mut self) {
        let mut rows = vec![HomeRow::Heading("Home Manager".into())];
        let Some(status) = &self.home_status else {
            rows.push(HomeRow::Text(if self.reading_home.is_some() {
                "Reading Home Manager…".into()
            } else {
                "Press r to read Home Manager.".into()
            }));
            self.home_rows = rows;
            self.clamp_main();
            return;
        };
        rows.push(HomeRow::Text(match (&status.version, status.installed) {
            (Some(version), _) => format!("Home Manager {version}"),
            (None, true) => "Home Manager is installed".into(),
            (None, false) => "home-manager is not installed here".into(),
        }));
        rows.push(HomeRow::Text(match (&status.config, &status.problem) {
            (Some(config), _) => format!(
                "Configuration: {} ({}, from {})",
                config
                    .flake_reference()
                    .unwrap_or_else(|| config.folder.join("home.nix").display().to_string()),
                if config.flake { "a flake" } else { "home.nix" },
                config.origin.label()
            ),
            (None, Some(problem)) => format!("Configuration: {problem}"),
            (None, None) => {
                "No configuration found: name its folder as flake under [home_manager] in settings.toml".into()
            }
        }));
        if let Some(repository) = &status.repository {
            rows.push(HomeRow::Text(match &repository.problem {
                Some(problem) => format!("Repository {}: {problem}", repository.path.display()),
                None => format!(
                    "Repository {}: {}, {} changed{}",
                    repository.path.display(),
                    repository.branch.as_deref().unwrap_or("detached"),
                    repository.changed,
                    match (repository.ahead, repository.behind) {
                        (Some(ahead), Some(behind)) if ahead + behind > 0 => {
                            format!(", {ahead} ahead and {behind} behind")
                        }
                        (Some(_), Some(_)) => ", up to date".to_string(),
                        _ => String::new(),
                    }
                ),
            }));
        }
        if !status.generations.is_empty() {
            rows.push(HomeRow::Heading(format!(
                "Generations ({})",
                status.generations.len()
            )));
            rows.extend((0..status.generations.len()).map(HomeRow::Generation));
        }
        if !status.links.is_empty() {
            rows.push(HomeRow::Heading("Configuration files".into()));
            rows.extend((0..status.links.len()).map(HomeRow::Link));
        }
        self.home_rows = rows;
        self.clamp_main();
    }

    fn draw_home(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Home Manager",
            "Its generations and configuration; building, switching and going back are plans you review.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.home_rows {
            items.push(match row {
                HomeRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                HomeRow::Text(text) => {
                    Line::styled(format!("  {}", helix_toolbox_core::printable(text)), muted)
                }
                HomeRow::Generation(index) => {
                    let generation = &self.home_status.as_ref().unwrap().generations[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:>5}  {}", generation.id, generation.built),
                            Style::default().fg(if generation.current {
                                self.theme.success()
                            } else {
                                self.theme.entry()
                            }),
                        ),
                        Span::styled(
                            if generation.current {
                                "  active".to_string()
                            } else {
                                String::new()
                            },
                            Style::default().fg(self.theme.success()),
                        ),
                    ])
                }
                HomeRow::Link(index) => {
                    let link = &self.home_status.as_ref().unwrap().links[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", link.name),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            if link.managed {
                                "linked by Home Manager".to_string()
                            } else if link.target.is_some() {
                                "a link of its own".to_string()
                            } else {
                                "a file of its own".to_string()
                            },
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Home Manager"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Restic -----

    /// The repository, when one is set up and usable.
    fn restic_config(&self) -> Option<&restic::Config> {
        match &self.restic_reading.as_ref()?.found {
            restic::Found::Ready(config) => Some(config),
            _ => None,
        }
    }

    /// Read the repository and its snapshots in the background. Tests set
    /// what was read themselves, so nothing of this machine's is asked.
    fn read_restic(&mut self) {
        if !self.examine_here {
            self.build_restic_rows();
            return;
        }
        let runner = self.restic.clone();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let found = restic::here();
            let snapshots = match &found {
                restic::Found::Ready(config) => restic::snapshots(runner.as_ref(), config),
                _ => Ok(Vec::new()),
            };
            let _ = sender.send(ResticReading {
                restores: matches!(found, restic::Found::Ready(_))
                    && restic::restores_safely(runner.as_ref()),
                found,
                snapshots,
                timer: restic::units_here().and_then(|units| restic::timer(&units)),
                baselines: history::state_dir()
                    .map(|state| restic::baselines(&state))
                    .unwrap_or_default(),
            });
        });
        self.reading_restic = Some(receiver);
        self.build_restic_rows();
    }

    /// List a folder of a snapshot: in the background, or at once in tests.
    fn browse_restic(&mut self, snapshot: restic::Snapshot, folder: String) {
        let Some(config) = self.restic_config().cloned() else {
            return;
        };
        let runner = self.restic.clone();
        let work = move || {
            let nodes = restic::files(runner.as_ref(), &config, &snapshot.id, &folder);
            ResticBrowse {
                snapshot,
                folder,
                nodes,
            }
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(work());
            });
            self.browsing_restic = Some(receiver);
        } else {
            self.restic_browse = Some(work());
        }
        self.build_restic_rows();
    }

    /// Look for a file's history across the snapshots.
    fn search_restic(&mut self, pattern: String) {
        let Some(config) = self.restic_config().cloned() else {
            return;
        };
        let ignore_case = self.restic_ignore_case;
        let runner = self.restic.clone();
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let work = move || {
            let found = restic::history(runner.as_ref(), &config, &home, &pattern, ignore_case);
            (pattern, found)
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(work());
            });
            self.searching_restic = Some(receiver);
        } else {
            self.restic_search = Some(work());
        }
        self.build_restic_rows();
    }

    fn restic_snapshots(&self) -> &[restic::Snapshot] {
        self.restic_reading
            .as_ref()
            .and_then(|reading| reading.snapshots.as_ref().ok())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    fn selected_restic_row(&self) -> Option<&ResticRow> {
        self.restic_rows.get(self.main_state.selected()?)
    }

    /// Enter: browse a snapshot or folder, go up, restore a file, test a
    /// baseline.
    fn open_restic_row(&mut self) {
        match self.selected_restic_row().cloned() {
            Some(ResticRow::Snapshot(index)) => {
                let snapshot = self.restic_snapshots()[index].clone();
                let folder = snapshot
                    .paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "/".into());
                self.browse_restic(snapshot, folder);
            }
            Some(ResticRow::Up) => {
                if let Some(browse) = &self.restic_browse {
                    let parent = Path::new(&browse.folder)
                        .parent()
                        .map(|parent| parent.display().to_string())
                        .unwrap_or_else(|| "/".into());
                    let snapshot = browse.snapshot.clone();
                    self.browse_restic(snapshot, parent);
                }
            }
            Some(ResticRow::Node(index)) => {
                let Some(browse) = &self.restic_browse else {
                    return;
                };
                let Some(node) = browse
                    .nodes
                    .as_ref()
                    .ok()
                    .and_then(|nodes| nodes.get(index))
                else {
                    return;
                };
                if node.is_dir() {
                    let (snapshot, folder) = (browse.snapshot.clone(), node.path.clone());
                    self.browse_restic(snapshot, folder);
                } else {
                    self.ask_restore();
                }
            }
            Some(ResticRow::Version(..)) => self.ask_restore(),
            Some(ResticRow::Baseline(_)) => self.ask_test(),
            _ => {}
        }
    }

    /// The snapshot and path the selected row would restore.
    fn restic_restore_target(&self) -> Option<(String, Option<String>)> {
        match self.selected_restic_row()? {
            ResticRow::Snapshot(index) => {
                Some((self.restic_snapshots().get(*index)?.id.clone(), None))
            }
            ResticRow::Node(index) => {
                let browse = self.restic_browse.as_ref()?;
                let node = browse.nodes.as_ref().ok()?.get(*index)?;
                Some((browse.snapshot.id.clone(), Some(node.path.clone())))
            }
            ResticRow::Version(file, version) => {
                let found = self.restic_search.as_ref()?.1.as_ref().ok()?.get(*file)?;
                Some((
                    found.versions.get(*version)?.snapshot.clone(),
                    Some(found.path.clone()),
                ))
            }
            _ => None,
        }
    }

    /// Restore what the selected row names, or, on another row, what is typed.
    fn ask_restore(&mut self) {
        match self.restic_restore_target() {
            Some((snapshot, include)) => self.ask_restore_into(snapshot, include),
            None => self.ask_restore_from(),
        }
    }

    /// Whether this restic restores without overwriting; says why not when not.
    fn restic_restores(&mut self) -> bool {
        if self
            .restic_reading
            .as_ref()
            .is_some_and(|reading| reading.restores)
        {
            return true;
        }
        self.popup = Some(Popup::Text(TextPopup::new(
            "Cannot restore here",
            "This restic cannot restore without overwriting and check what it wrote; restic 0.17 or later can.",
            true,
        )));
        false
    }

    /// A restore typed in full: the snapshot (latest unless another is
    /// named) and, after it, only this path.
    fn ask_restore_from(&mut self) {
        if self.restic_config().is_none() || !self.restic_restores() {
            return;
        }
        self.popup = Some(Popup::Ask(
            InputPopup::new(
                "Restore into a new folder",
                "The snapshot (latest, or its ID), and after it, if only one path of it, that path, such as latest /home/me/Documents:",
            )
            .with_value("latest".to_string())
            .answering("Next"),
            Asked::ResticRestoreFrom,
        ));
    }

    fn ask_restore_into(&mut self, snapshot: String, include: Option<String>) {
        if !self.restic_restores() {
            return;
        }
        let what = include
            .as_deref()
            .map(helix_toolbox_core::printable)
            .unwrap_or_else(|| "the whole snapshot".to_string());
        self.popup = Some(Popup::Ask(
            InputPopup::new(
                "Restore into a new folder",
                format!(
                    "Where to restore {what} from {}: a new folder of your home folder; nothing is overwritten.",
                    &snapshot[..snapshot.len().min(8)]
                ),
            )
            .with_value("~/restored".to_string())
            .path()
            .answering("Restore"),
            Asked::ResticRestore(snapshot, include),
        ));
    }

    fn restore_restic(&mut self, snapshot: &str, include: Option<&str>, text: &str) {
        let planned = (|| -> Result<Plan, String> {
            let config = self
                .restic_config()
                .ok_or("No restic repository is set up.")?;
            let home = std::env::var_os("HOME")
                .map(PathBuf::from)
                .ok_or("there is no home folder")?;
            let target = restic::check_target(&home, &typed_path(text)?)?;
            restic::plan_restore(config, snapshot, &target, include)
        })();
        self.confirm_restic(planned);
    }

    fn ask_test(&mut self) {
        let Some(ResticRow::Baseline(index)) = self.selected_restic_row() else {
            return;
        };
        let Some(baseline) = self
            .restic_reading
            .as_ref()
            .and_then(|reading| reading.baselines.get(*index))
        else {
            return;
        };
        self.popup = Some(Popup::Ask(
            InputPopup::new(
                "Test a restore",
                format!(
                    "The snapshot to restore {} from and compare (latest, or a snapshot's id):",
                    helix_toolbox_core::printable(&baseline.path)
                ),
            )
            .with_value("latest".to_string())
            .answering("Test"),
            Asked::ResticTest(baseline.id.clone()),
        ));
    }

    fn test_restic(&mut self, baseline: &str, snapshot: &str) {
        let planned = (|| -> Result<Plan, String> {
            let config = self
                .restic_config()
                .ok_or("No restic repository is set up.")?;
            let reading = self.restic_reading.as_ref().ok_or("Nothing read yet.")?;
            let chosen = restic::find_baseline(&reading.baselines, baseline)
                .ok_or("That baseline is not there.")?;
            restic::plan_test(
                config,
                chosen,
                if snapshot.is_empty() {
                    "latest"
                } else {
                    snapshot
                },
            )
        })();
        self.confirm_restic(planned);
    }

    /// A backup or check of the repository, to review.
    fn plan_restic(&mut self, make: fn(&restic::Config) -> Plan) {
        match self.restic_config() {
            Some(config) => {
                let planned = make(config);
                self.confirm_plan(Ok(planned));
            }
            None => self.no_restic(),
        }
    }

    fn confirm_restic(&mut self, planned: Result<Plan, String>) {
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)));
            }
        }
    }

    fn no_restic(&mut self) {
        self.popup = Some(Popup::Text(TextPopup::new(
            "No repository",
            "Name the repository and its password under [restic] in settings.toml (helix-toolbox restic says how), or with restic's RESTIC_REPOSITORY.",
            true,
        )));
    }

    fn record_baseline(&mut self, text: &str) -> Result<String, String> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or("there is no home folder")?;
        let state = history::state_dir().ok_or("cannot find the state folder")?;
        let recorded = restic::record(&state, &home, &typed_path(text)?, history::now())?;
        if let Some(reading) = &mut self.restic_reading {
            reading.baselines = restic::baselines(&state);
        }
        self.build_restic_rows();
        Ok(format!(
            "{}: SHA-256 {}.\n\nBack it up, then v on it restores it from a snapshot and compares.",
            recorded.path, recorded.sha256
        ))
    }

    /// The timed backup on, every day or every week, or off when it is on.
    fn toggle_restic_timer(&mut self, every: restic::Every) {
        if !self.examine_here {
            return;
        }
        let on = restic::units_here().and_then(|units| restic::timer(&units));
        self.set_restic_timer(if on.is_some() { None } else { Some(every) });
    }

    /// The timed backup's state, for Settings.
    fn restic_timer_label(&self) -> String {
        if !self.examine_here {
            return "not available here".to_string();
        }
        match restic::units_here().and_then(|units| restic::timer(&units)) {
            Some(every) => format!("yes, {}", every.label()),
            None if self.restic_config().is_some() || restic::repository_here().is_some() => {
                "no".to_string()
            }
            None => "no (name a repository in Restic first)".to_string(),
        }
    }

    /// Turn the timed backup off, or on to run that often.
    fn set_restic_timer(&mut self, every: Option<restic::Every>) {
        let config = self
            .restic_config()
            .cloned()
            .or_else(|| match restic::here() {
                restic::Found::Ready(config) => Some(config),
                _ => None,
            });
        let Some(config) = config else {
            self.no_restic();
            return;
        };
        let said = (|| -> Result<String, String> {
            let units =
                restic::units_here().ok_or("there is no home folder to keep the timer in")?;
            let Some(every) = every else {
                restic::timer_off(&units, &notifications::systemctl)?;
                return Ok("The timed backup is off.".to_string());
            };
            let program = std::env::current_exe().map_err(|error| error.to_string())?;
            let started =
                restic::timer_on(&units, &program, &config, every, &notifications::systemctl)?;
            Ok(format!(
                "The backup runs {}{}. Settings, or w in Restic, change it.",
                every.label(),
                match started {
                    notifications::Started::Now => "",
                    notifications::Started::AtLogin => ", from the next login",
                }
            ))
        })();
        if let Some(reading) = &mut self.restic_reading {
            reading.timer = restic::units_here().and_then(|units| restic::timer(&units));
        }
        self.build_restic_rows();
        self.popup = Some(Popup::Text(match said {
            Ok(text) => TextPopup::new("Timed backup", text, true),
            Err(why) => TextPopup::new("The timer did not change", why, true),
        }));
    }

    fn describe_restic(&mut self) {
        let text = match self.selected_restic_row() {
            Some(ResticRow::Snapshot(index)) => {
                let Some(snapshot) = self.restic_snapshots().get(*index) else {
                    return;
                };
                format!(
                    "Snapshot {} of {}, taken {}.\nFolders: {}\n\nEnter browses it; R restores it into a new folder.",
                    snapshot.id,
                    helix_toolbox_core::printable(&snapshot.hostname),
                    snapshot.when(),
                    helix_toolbox_core::printable(&snapshot.paths.join(", "))
                )
            }
            Some(ResticRow::Baseline(index)) => {
                let Some(baseline) = self
                    .restic_reading
                    .as_ref()
                    .and_then(|reading| reading.baselines.get(*index))
                else {
                    return;
                };
                format!(
                    "{}\nSHA-256 {}, recorded {}.\n\nv restores it from a snapshot into a temporary folder and compares.",
                    helix_toolbox_core::printable(&baseline.path),
                    baseline.sha256,
                    timestamp(baseline.recorded)
                )
            }
            _ => return,
        };
        self.popup = Some(Popup::Text(TextPopup::new("Restic", text, true)));
    }

    fn build_restic_rows(&mut self) {
        let mut rows = vec![ResticRow::Heading("The repository".into())];
        match self.restic_reading.as_ref().map(|reading| &reading.found) {
            None if self.reading_restic.is_some() => {
                rows.push(ResticRow::Text("Asking restic…".into()))
            }
            None => rows.push(ResticRow::Text("Press r to read the repository.".into())),
            Some(restic::Found::Missing) => {
                rows.push(ResticRow::Text("No restic repository is set up.".into()));
                rows.push(ResticRow::Text(
                    "Name it under [restic] in settings.toml: repository = \"…\", password_file = \"…\"; helix-toolbox restic shows how.".into(),
                ));
            }
            Some(restic::Found::Problem(why)) => rows.push(ResticRow::Text(why.clone())),
            Some(restic::Found::Ready(config)) => {
                rows.push(ResticRow::Text(format!(
                    "{} (from {})",
                    helix_toolbox_core::printable(&config.repository),
                    config.origin.label()
                )));
                rows.push(ResticRow::Text(format!(
                    "Password from {}",
                    config.password.describe()
                )));
                rows.push(ResticRow::Text(format!(
                    "Backs up {}; the timer is {}",
                    config
                        .paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    self.restic_reading
                        .as_ref()
                        .and_then(|reading| reading.timer)
                        .map(restic::Every::label)
                        .unwrap_or("off")
                )));
                if !config.excludes.is_empty() {
                    rows.push(ResticRow::Text(format!(
                        "Leaves out {}",
                        helix_toolbox_core::printable(&config.excludes.join(", "))
                    )));
                }
                rows.push(ResticRow::Text(
                    "b backs up, c checks, w backs up every day and W every week (either stops the timer), s looks for a file's history, F restores a snapshot you name.".into(),
                ));
            }
        }
        if let Some(reading) = &self.restic_reading
            && matches!(reading.found, restic::Found::Ready(_))
        {
            match &reading.snapshots {
                Ok(snapshots) => {
                    rows.push(ResticRow::Heading(format!(
                        "Snapshots ({})",
                        snapshots.len()
                    )));
                    rows.extend((0..snapshots.len()).map(ResticRow::Snapshot));
                }
                Err(why) => rows.push(ResticRow::Text(format!(
                    "restic could not read the repository: {why}"
                ))),
            }
        }
        if self.browsing_restic.is_some() {
            rows.push(ResticRow::Heading("Listing the folder…".into()));
        } else if let Some(browse) = &self.restic_browse {
            rows.push(ResticRow::Heading(format!(
                "In {} at {}",
                browse.snapshot.short_id,
                helix_toolbox_core::printable(&browse.folder)
            )));
            if browse.folder != "/" {
                rows.push(ResticRow::Up);
            }
            match &browse.nodes {
                Ok(nodes) if nodes.is_empty() => rows.push(ResticRow::Text("Nothing here.".into())),
                Ok(nodes) => rows.extend((0..nodes.len()).map(ResticRow::Node)),
                Err(why) => rows.push(ResticRow::Text(why.clone())),
            }
        }
        if self.searching_restic.is_some() {
            rows.push(ResticRow::Heading("Looking through the snapshots…".into()));
        } else if let Some((pattern, found)) = &self.restic_search {
            rows.push(ResticRow::Heading(format!(
                "History of {}{}",
                helix_toolbox_core::printable(pattern),
                if self.restic_ignore_case {
                    ", ignoring upper and lower case (i)"
                } else {
                    ""
                }
            )));
            match found {
                Ok(files) if files.is_empty() => rows.push(ResticRow::Text(
                    "No snapshot has a file that matches.".into(),
                )),
                Ok(files) => {
                    for (index, file) in files.iter().enumerate() {
                        rows.push(ResticRow::Text(helix_toolbox_core::printable(&file.path)));
                        rows.extend(
                            (0..file.versions.len())
                                .map(|version| ResticRow::Version(index, version)),
                        );
                        if file.missing_from > 0 {
                            rows.push(ResticRow::Text(format!(
                                "Not in the {} newer snapshot(s) of the same folder: deleted or moved since.",
                                file.missing_from
                            )));
                        }
                    }
                }
                Err(why) => rows.push(ResticRow::Text(why.clone())),
            }
        }
        if let Some(reading) = &self.restic_reading {
            rows.push(ResticRow::Heading(format!(
                "Recovery tests ({} baselines)",
                reading.baselines.len()
            )));
            rows.push(ResticRow::Text(
                "n records a file's SHA-256; v on one restores it from a snapshot and compares."
                    .into(),
            ));
            rows.extend((0..reading.baselines.len()).map(ResticRow::Baseline));
        }
        self.restic_rows = rows;
        self.clamp_main();
    }

    fn draw_restic(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Restic",
            "Backups of your own files: snapshots, a file's history, restores into a new folder and recovery tests.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let printable = helix_toolbox_core::printable;
        let size = |bytes: Option<u64>| bytes.map(storage::shown).unwrap_or_default();
        let snapshots = self.restic_snapshots();
        let mut items: Vec<Line> = Vec::new();
        for row in &self.restic_rows {
            items.push(match row {
                ResticRow::Heading(text) => {
                    Line::styled(text.clone(), Style::default().fg(self.theme.group()).bold())
                }
                ResticRow::Text(text) => Line::styled(format!("  {text}"), muted),
                ResticRow::Snapshot(index) => {
                    let snapshot = &snapshots[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  {}  ", snapshot.short_id, snapshot.when()),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<16} {}",
                                printable(&snapshot.hostname),
                                printable(&snapshot.paths.join(", "))
                            ),
                            muted,
                        ),
                    ])
                }
                ResticRow::Up => Line::styled("  ..", Style::default().fg(self.theme.entry())),
                ResticRow::Node(index) => {
                    let node =
                        &self.restic_browse.as_ref().unwrap().nodes.as_ref().unwrap()[*index];
                    if node.is_dir() {
                        Line::styled(
                            format!("  {}/", printable(&node.name)),
                            Style::default().fg(self.theme.group()),
                        )
                    } else {
                        Line::from(vec![
                            Span::styled(
                                format!("  {:<40} ", printable(&node.name)),
                                Style::default().fg(self.theme.entry()),
                            ),
                            Span::styled(size(node.size), muted),
                        ])
                    }
                }
                ResticRow::Version(file, version) => {
                    let found = &self.restic_search.as_ref().unwrap().1.as_ref().unwrap()[*file];
                    let version = &found.versions[*version];
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "    {}  {}  ",
                                version.short_id,
                                version
                                    .time
                                    .replace('T', " ")
                                    .get(..16)
                                    .unwrap_or(&version.time)
                            ),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<10} {}",
                                size(version.size),
                                match version.change {
                                    restic::Change::First => "first",
                                    restic::Change::Changed => "changed",
                                    restic::Change::Same => "same",
                                }
                            ),
                            muted,
                        ),
                    ])
                }
                ResticRow::Baseline(index) => {
                    let baseline = &self.restic_reading.as_ref().unwrap().baselines[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {}  ", &baseline.sha256[..baseline.sha256.len().min(12)]),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{}  {}",
                                timestamp(baseline.recorded),
                                printable(&baseline.path)
                            ),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Restic"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Repositories -----

    /// Where repositories are looked for: the tests' places, else those the
    /// settings name; none in a test that sets none.
    fn places_here(&self) -> Option<repos::Places> {
        if let Some(places) = &self.repo_places {
            return Some(places.clone());
        }
        let settings = self.settings_path.as_ref()?;
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        Some(repos::places(settings, &home))
    }

    fn workspaces_path(&self) -> Option<PathBuf> {
        self.workspaces_file.clone().or_else(|| {
            self.settings_path
                .as_ref()
                .map(|settings| settings.with_file_name("workspaces.toml"))
        })
    }

    fn kept_workspaces(&self) -> Workspaces {
        self.workspaces_path()
            .and_then(|path| workspaces::load(&path).ok())
            .unwrap_or_default()
    }

    /// Find the repositories and read each, in the background on this
    /// machine; favorites first.
    fn read_repos(&mut self) {
        let Some(places) = self.places_here() else {
            self.repos_found = Some((
                repos::Places {
                    roots: Vec::new(),
                    depth: repos::DEFAULT_DEPTH,
                    origin: "default".to_string(),
                },
                Vec::new(),
            ));
            self.build_repo_rows();
            return;
        };
        let kept = self.kept_workspaces();
        let read = move || {
            let found = repos::discover(&places);
            let mut summaries = repos::summaries(&repos::Here, &found);
            summaries.sort_by_key(|summary| !kept.favorite(&summary.path));
            (places, summaries)
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(read());
            });
            self.reading_repos = Some(receiver);
        } else {
            self.repos_found = Some(read());
        }
        self.build_repo_rows();
    }

    /// Read the list again, and the open repository with it.
    fn reload_repos(&mut self) {
        self.read_repos();
        if let Some((found, _)) = &self.repo_open {
            let path = found.summary.path.clone();
            self.open_repo(path);
        }
    }

    /// Read one repository and its tasks, in the background on this machine.
    fn open_repo(&mut self, path: PathBuf) {
        let kept = self.kept_workspaces();
        let read = move || -> RepoOpened {
            let found = repos::details(&repos::Here, &path)?;
            let tasks = workspaces::tasks(&path, &kept);
            Ok((found, tasks))
        };
        if self.examine_here {
            let (sender, receiver) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = sender.send(read());
            });
            self.opening_repo = Some(receiver);
            self.build_repo_rows();
        } else {
            self.opened_repo(read());
        }
    }

    fn opened_repo(&mut self, read: RepoOpened) {
        match read {
            Ok(open) => {
                let same = self
                    .repo_open
                    .as_ref()
                    .map(|(found, _)| &found.summary.path)
                    == Some(&open.0.summary.path);
                self.repo_open = Some(open);
                if !same {
                    self.main_state.select(Some(0));
                }
                self.build_repo_rows();
            }
            Err(why) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot read the repository",
                    why,
                    true,
                )));
            }
        }
    }

    /// Back from a repository to the list; false when the list is shown.
    fn close_repo(&mut self) -> bool {
        if self.repo_open.take().is_none() {
            return false;
        }
        self.main_state.select(Some(0));
        self.build_repo_rows();
        true
    }

    fn selected_repo_row(&self) -> Option<RepoRow> {
        self.repo_rows.get(self.main_state.selected()?).cloned()
    }

    fn repo_key(&mut self, code: KeyCode) {
        let open = self.repo_open.is_some();
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.repo_enter(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                if !self.close_repo() {
                    self.focus = Focus::Sidebar;
                }
            }
            KeyCode::Char('r') => self.reload_repos(),
            KeyCode::Char('f') => self.toggle_repo_favorite(),
            _ if !open => {}
            KeyCode::Char(' ') => self.stage_selected(),
            KeyCode::Char('A') => self.plan_repo_change(Change::StageAll),
            KeyCode::Char('c') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Commit", "The commit message:").answering("Review"),
                    Asked::CommitMessage,
                ))
            }
            KeyCode::Char('b') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("New branch", "The branch to create and switch to:")
                        .answering("Review"),
                    Asked::NewBranch,
                ))
            }
            KeyCode::Char('s') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Stash", "A message for the stash (new files go in it too):")
                        .answering("Review"),
                    Asked::StashMessage,
                ))
            }
            KeyCode::Char('F') => self.plan_repo_change(Change::Fetch),
            KeyCode::Char('p') => self.plan_repo_change(Change::Pull),
            KeyCode::Char('P') => self.push_repo(),
            KeyCode::Char('n') => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "A task of yours",
                        "The command, run in the repository's folder, such as npm run serve; begin with fish: or bash: to run it with that shell:",
                    )
                    .answering("Keep"),
                    Asked::AddTask,
                ))
            }
            KeyCode::Char('x') => self.remove_repo_task(),
            _ => {}
        }
    }

    /// Enter: open a repository; in one, a file's diff, a branch to switch
    /// to, a stash to apply or a task to run.
    fn repo_enter(&mut self) {
        let Some(row) = self.selected_repo_row() else {
            return;
        };
        let Some((found, tasks)) = &self.repo_open else {
            if let (RepoRow::Repository(index), Some((_, summaries))) = (row, &self.repos_found)
                && let Some(summary) = summaries.get(index)
            {
                let path = summary.path.clone();
                self.open_repo(path);
            }
            return;
        };
        match row {
            RepoRow::File(index) => {
                let Some(file) = found.files.get(index) else {
                    return;
                };
                let path = found.summary.path.clone();
                let mut text = String::new();
                for (staged, heading) in [(true, "Staged"), (false, "Not staged")] {
                    if (staged && !file.is_staged()) || (!staged && !file.has_unstaged()) {
                        continue;
                    }
                    match repos::diff(&repos::Here, &path, found, Some(&file.path), staged) {
                        Ok(shown) => {
                            text.push_str(&format!("{heading}:\n{}", shown.text));
                            if shown.truncated {
                                text.push_str("(cut short)\n");
                            }
                        }
                        Err(why) => text.push_str(&format!("{heading}: {why}\n")),
                    }
                }
                let text: String = text
                    .lines()
                    .map(|line| format!("{}\n", helix_toolbox_core::printable(line)))
                    .collect();
                self.popup = Some(Popup::Text(TextPopup::new(
                    helix_toolbox_core::printable(&file.path),
                    text,
                    false,
                )));
            }
            RepoRow::Branch(index) => {
                if let Some(branch) = found.branches.get(index).cloned()
                    && Some(&branch) != found.summary.branch.as_ref()
                {
                    self.plan_repo_change(Change::SwitchBranch(branch));
                }
            }
            RepoRow::Stash(index) => {
                if let Some(stash) = found.stashes.get(index) {
                    let id = stash.id.clone();
                    self.plan_repo_change(Change::ApplyStash(id));
                }
            }
            RepoRow::Task(index) => {
                if let Some(task) = tasks.get(index) {
                    match workspaces::plan_task(&found.summary.path, task) {
                        Ok(planned) => self.confirm_plan(Ok(planned)),
                        Err(why) => {
                            self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true)))
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Review a change to the open repository.
    fn plan_repo_change(&mut self, change: Change) {
        let Some((found, _)) = &self.repo_open else {
            return;
        };
        match repos::plan(&repos::Here, found, &change) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => self.popup = Some(Popup::Text(TextPopup::new("Cannot plan", why, true))),
        }
    }

    /// Space: stage the file's changes, or unstage it when all are staged.
    fn stage_selected(&mut self) {
        let (Some(RepoRow::File(index)), Some((found, _))) =
            (self.selected_repo_row(), &self.repo_open)
        else {
            return;
        };
        let Some(file) = found.files.get(index) else {
            return;
        };
        let paths = vec![file.path.clone()];
        let change = if file.has_unstaged() || file.conflict {
            Change::Stage(paths)
        } else {
            Change::Unstage(paths)
        };
        self.plan_repo_change(change);
    }

    /// P: push to the upstream, or publish the branch to origin (or the
    /// only remote) when it has none.
    fn push_repo(&mut self) {
        let Some((found, _)) = &self.repo_open else {
            return;
        };
        let change = if found.push_to.is_some() {
            Change::Push
        } else {
            match found
                .remotes
                .iter()
                .find(|remote| *remote == "origin")
                .or(found.remotes.first())
            {
                Some(remote) => Change::Publish(remote.clone()),
                None => Change::Push,
            }
        };
        self.plan_repo_change(change);
    }

    fn toggle_repo_favorite(&mut self) {
        let path = match (&self.repo_open, self.selected_repo_row(), &self.repos_found) {
            (Some((found, _)), ..) => found.summary.path.clone(),
            (None, Some(RepoRow::Repository(index)), Some((_, summaries))) => {
                match summaries.get(index) {
                    Some(summary) => summary.path.clone(),
                    None => return,
                }
            }
            _ => return,
        };
        let Some(file) = self.workspaces_path() else {
            return;
        };
        let mut kept = self.kept_workspaces();
        let favorite = !kept.favorite(&path);
        kept.set_favorite(&path, favorite);
        if let Err(why) = workspaces::save(&file, &kept) {
            self.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.reload_repos();
    }

    /// Keep a command as a task of the open repository.
    fn add_repo_task(&mut self, text: &str) -> Result<String, String> {
        let Some((found, _)) = &self.repo_open else {
            return Err("No repository is open.".to_string());
        };
        // "fish: …" or "bash: …" runs it with that shell, as the window's
        // Run with choice; otherwise it runs as it is.
        let (shell, command) = task_shell(text);
        let mut id: String = command
            .to_lowercase()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() {
                    character
                } else {
                    '-'
                }
            })
            .collect();
        while id.contains("--") {
            id = id.replace("--", "-");
        }
        let id: String = id.trim_matches('-').chars().take(40).collect();
        let path = found.summary.path.clone();
        let file = self
            .workspaces_path()
            .ok_or("There is no configuration folder to keep it in.")?;
        let mut kept = self.kept_workspaces();
        kept.add_task(
            &path,
            Task {
                id: if id.is_empty() {
                    "task".to_string()
                } else {
                    id
                },
                name: command.chars().take(80).collect(),
                command: command.clone(),
                shell,
                detected: false,
            },
        )?;
        workspaces::save(&file, &kept)?;
        self.open_repo(path);
        Ok(format!(
            "{command} is kept as a task in {}. Enter on it runs it, after you review it.",
            file.display()
        ))
    }

    /// x: remove the selected task, when it is one of the user's.
    fn remove_repo_task(&mut self) {
        let (Some(RepoRow::Task(index)), Some((found, tasks))) =
            (self.selected_repo_row(), &self.repo_open)
        else {
            return;
        };
        let Some(task) = tasks.get(index) else {
            return;
        };
        if task.detected {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Not yours to remove",
                "The repository's manifest names this task; only tasks you added are removed.",
                true,
            )));
            return;
        }
        let (path, id) = (found.summary.path.clone(), task.id.clone());
        let Some(file) = self.workspaces_path() else {
            return;
        };
        let mut kept = self.kept_workspaces();
        let removed = kept
            .remove_task(&path, &id)
            .and_then(|_| workspaces::save(&file, &kept));
        if let Err(why) = removed {
            self.popup = Some(Popup::Text(TextPopup::new("It did not work", why, true)));
            return;
        }
        self.open_repo(path);
    }

    fn repo_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        let Some((found, _)) = &self.repo_open else {
            list.push(Hint::new(&["Enter"], "Open"));
            list.push(Hint::new(&["f"], "Favorite"));
            list.push(Hint::new(&["r"], "Read again"));
            return;
        };
        match self.selected_repo_row() {
            Some(RepoRow::File(_)) => {
                list.push(Hint::new(&["Enter"], "Diff"));
                list.push(Hint::new(&["Space"], "Stage"));
            }
            Some(RepoRow::Branch(_)) => list.push(Hint::new(&["Enter"], "Switch")),
            Some(RepoRow::Stash(_)) => list.push(Hint::new(&["Enter"], "Apply")),
            Some(RepoRow::Task(_)) => {
                list.push(Hint::new(&["Enter"], "Run"));
                list.push(Hint::new(&["x"], "Remove"));
            }
            _ => {}
        }
        if found.staged() > 0 {
            list.push(Hint::new(&["c"], "Commit"));
        }
        list.push(Hint::new(&["b"], "Branch"));
        if !found.clean() {
            list.push(Hint::new(&["s"], "Stash"));
        }
        if !found.remotes.is_empty() {
            list.push(Hint::new(&["F", "p", "P"], "Fetch, pull, push"));
        }
        list.push(Hint::new(&["n"], "Task"));
        list.push(Hint::new(&["h"], "Back"));
    }

    fn build_repo_rows(&mut self) {
        let mut rows: Vec<RepoRow> = Vec::new();
        if let Some((found, tasks)) = &self.repo_open {
            let summary = &found.summary;
            rows.push(RepoRow::Heading(format!(
                "{} on {}: {}",
                summary.name,
                summary.branch.as_deref().unwrap_or("a detached HEAD"),
                crate::repos::sync_words(summary)
            )));
            rows.push(RepoRow::Text(summary.path.display().to_string()));
            if let Some(origin) = &summary.origin {
                rows.push(RepoRow::Text(format!("origin {origin}")));
            }
            if let Some(busy) = &found.busy {
                rows.push(RepoRow::Text(format!(
                    "In the middle of {busy}: finish it with git."
                )));
            }
            if found.files.is_empty() {
                rows.push(RepoRow::Heading("No changes".into()));
            } else {
                rows.push(RepoRow::Heading(format!(
                    "Changes ({}, {} staged)",
                    found.files.len(),
                    found.staged()
                )));
                rows.extend((0..found.files.len()).map(RepoRow::File));
            }
            if !found.branches.is_empty() {
                rows.push(RepoRow::Heading("Branches".into()));
                rows.extend((0..found.branches.len()).map(RepoRow::Branch));
            }
            if !found.stashes.is_empty() {
                rows.push(RepoRow::Heading("Stashes".into()));
                rows.extend((0..found.stashes.len()).map(RepoRow::Stash));
            }
            rows.push(RepoRow::Heading("Tasks".into()));
            if tasks.is_empty() {
                rows.push(RepoRow::Text(
                    "None found in its manifests; n keeps a command of yours.".into(),
                ));
            }
            rows.extend((0..tasks.len()).map(RepoRow::Task));
            if !found.log.is_empty() {
                rows.push(RepoRow::Heading("Recent commits".into()));
                rows.extend((0..found.log.len()).map(RepoRow::Commit));
            }
        } else if self.opening_repo.is_some() {
            rows.push(RepoRow::Heading("Reading the repository…".into()));
        } else {
            match &self.repos_found {
                None => rows.push(RepoRow::Heading("Looking for repositories…".into())),
                Some((places, summaries)) => {
                    let roots = if places.roots.is_empty() {
                        "no folders".to_string()
                    } else {
                        places
                            .roots
                            .iter()
                            .map(|root| root.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    };
                    if summaries.is_empty() {
                        rows.push(RepoRow::Heading("No repositories found".into()));
                    } else {
                        rows.push(RepoRow::Heading(format!(
                            "{} repositories",
                            summaries.len()
                        )));
                        rows.extend((0..summaries.len()).map(RepoRow::Repository));
                    }
                    rows.push(RepoRow::Text(format!(
                        "Under {roots}, {} folders deep (from the {}); [repositories] roots in settings.toml changes it.",
                        places.depth, places.origin
                    )));
                }
            }
            if self.reading_repos.is_some() && self.repos_found.is_some() {
                rows.push(RepoRow::Text("Reading them again…".into()));
            }
        }
        self.repo_rows = rows;
        self.clamp_main();
    }

    fn draw_repos(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Repositories",
            "The Git repositories under your project folders. Reading changes nothing; every change is reviewed first.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let printable = helix_toolbox_core::printable;
        let muted = Style::default().fg(self.theme.muted());
        let kept = self.kept_workspaces();
        let mut items: Vec<Line> = Vec::new();
        for row in &self.repo_rows {
            items.push(match row {
                RepoRow::Heading(text) => Line::styled(
                    printable(text),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                RepoRow::Text(text) => Line::styled(format!("  {}", printable(text)), muted),
                RepoRow::Repository(index) => {
                    let Some(summary) = self
                        .repos_found
                        .as_ref()
                        .and_then(|(_, summaries)| summaries.get(*index))
                    else {
                        continue;
                    };
                    let changes = match (&summary.problem, summary.changed) {
                        (Some(problem), _) => problem.clone(),
                        (None, 0) => "clean".to_string(),
                        (None, count) => format!("{count} changed"),
                    };
                    let color = if summary.problem.is_some() {
                        self.theme.failure()
                    } else if summary.changed > 0 {
                        self.theme.warning()
                    } else {
                        self.theme.success()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{}{:<26}",
                                if kept.favorite(&summary.path) {
                                    "★ "
                                } else {
                                    "  "
                                },
                                printable(&summary.name)
                            ),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{:<20}",
                                printable(summary.branch.as_deref().unwrap_or("(detached)"))
                            ),
                            muted,
                        ),
                        Span::styled(format!("{changes:<14}"), Style::default().fg(color)),
                        Span::styled(format!("{:<18}", crate::repos::sync_words(summary)), muted),
                        Span::styled(
                            printable(summary.last.as_deref().unwrap_or_default()),
                            muted,
                        ),
                    ])
                }
                RepoRow::File(index) => {
                    let Some(file) = self
                        .repo_open
                        .as_ref()
                        .and_then(|(found, _)| found.files.get(*index))
                    else {
                        continue;
                    };
                    let color = if file.conflict {
                        self.theme.failure()
                    } else if file.is_staged() && !file.has_unstaged() {
                        self.theme.success()
                    } else {
                        self.theme.warning()
                    };
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "  {} {:<13}",
                                if file.is_staged() { "●" } else { "○" },
                                file.state()
                            ),
                            Style::default().fg(color),
                        ),
                        Span::styled(
                            format!(
                                "{}{}",
                                printable(&file.path),
                                file.original
                                    .as_deref()
                                    .map(|original| format!(" (was {})", printable(original)))
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(self.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Branch(index) => {
                    let Some((found, _)) = &self.repo_open else {
                        continue;
                    };
                    let branch = &found.branches[*index];
                    let current = found.summary.branch.as_ref() == Some(branch);
                    Line::styled(
                        format!(
                            "  {} {}",
                            if current { "●" } else { " " },
                            printable(branch)
                        ),
                        Style::default().fg(if current {
                            self.theme.success()
                        } else {
                            self.theme.entry()
                        }),
                    )
                }
                RepoRow::Stash(index) => {
                    let Some((found, _)) = &self.repo_open else {
                        continue;
                    };
                    let stash = &found.stashes[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<12}", stash.name),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(printable(&stash.subject), muted),
                    ])
                }
                RepoRow::Commit(index) => {
                    let Some((found, _)) = &self.repo_open else {
                        continue;
                    };
                    let commit = &found.log[*index];
                    Line::from(vec![
                        Span::styled(format!("  {:<10}", commit.hash), muted),
                        Span::styled(
                            format!(
                                "{:<12}",
                                commit
                                    .at
                                    .map(helix_toolbox_engine::date)
                                    .unwrap_or_default()
                            ),
                            muted,
                        ),
                        Span::styled(
                            printable(&commit.subject),
                            Style::default().fg(self.theme.entry()),
                        ),
                    ])
                }
                RepoRow::Task(index) => {
                    let Some((_, tasks)) = &self.repo_open else {
                        continue;
                    };
                    let task = &tasks[*index];
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<28}", printable(&task.name)),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            format!(
                                "{}{}{}",
                                if task.detected { "" } else { "yours: " },
                                match task.shell {
                                    workspaces::Shell::Direct => "",
                                    workspaces::Shell::Fish => "fish: ",
                                    workspaces::Shell::Bash => "bash: ",
                                },
                                printable(&task.command)
                            ),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Repositories"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Configs -----

    /// Where a configuration is: in the tests' folders, else this machine's.
    fn locate_config(&self, kind: configs::Kind) -> Result<configs::Located, String> {
        match &self.config_places {
            Some((home, config)) => configs::locate(
                kind,
                &configs::Facts {
                    home,
                    config_home: config,
                    settings: None,
                    command_center: None,
                    store: Path::new("/nix/store"),
                },
            ),
            None if self.examine_here => configs::here(kind),
            None => Err("configurations are not read here".to_string()),
        }
    }

    /// The private folder for copies: the tests' own, else beside the
    /// history; never a shared one.
    fn copies_folder(&self) -> Option<PathBuf> {
        self.config_folder.clone().or_else(configs::private_folder)
    }

    fn no_copies_folder(&mut self) {
        self.cannot_config(
            "There is no state folder for Helix Toolbox's private copies; nothing is saved.".into(),
        );
    }

    /// Ghostty's own check on this machine; none in tests.
    fn config_check(&self) -> fn(&Path) -> Option<(bool, String)> {
        if self.examine_here {
            configs::ghostty_here
        } else {
            |_| None
        }
    }

    /// Read both configurations, their checks and their backups.
    fn read_configs(&mut self) {
        let check = self.config_check();
        self.config_parts = configs::Kind::ALL
            .into_iter()
            .map(|kind| {
                let found = self.locate_config(kind);
                let (loaded, checked, history) = match &found {
                    Ok(located) => {
                        let loaded = configs::read(located);
                        let checked = loaded.as_ref().ok().map(|loaded| {
                            configs::validate(
                                kind,
                                &loaded.content,
                                &check,
                                self.copies_folder().as_deref(),
                            )
                        });
                        (Some(loaded), checked, configs::history(located))
                    }
                    Err(_) => (None, None, Vec::new()),
                };
                ConfigPart {
                    kind,
                    found,
                    loaded,
                    checked,
                    history,
                    draft: None,
                    draft_checked: None,
                }
            })
            .collect();
        self.build_config_rows();
    }

    fn config_part(&self, kind: configs::Kind) -> Option<&ConfigPart> {
        self.config_parts.iter().find(|part| part.kind == kind)
    }

    /// The configuration ready to change: where it is and its text.
    fn config_ready(
        &self,
        kind: configs::Kind,
    ) -> Result<(configs::Located, configs::Loaded), String> {
        let part = self.config_part(kind).ok_or("it has not been read")?;
        let located = part.found.clone()?;
        let loaded = part.loaded.clone().ok_or("it has not been read")??;
        Ok((located, loaded))
    }

    fn selected_config_row(&self) -> Option<ConfigRow> {
        self.config_rows.get(self.main_state.selected()?).cloned()
    }

    /// The configuration the selected row belongs to.
    fn selected_config_kind(&self) -> Option<configs::Kind> {
        let at = self.main_state.selected()?;
        self.config_rows[..=at.min(self.config_rows.len().saturating_sub(1))]
            .iter()
            .rev()
            .find_map(|row| match row {
                ConfigRow::Field(kind, _) | ConfigRow::Edit(kind) | ConfigRow::Backup(kind, _) => {
                    Some(*kind)
                }
                ConfigRow::Heading(text) => configs::Kind::ALL
                    .into_iter()
                    .find(|kind| text.starts_with(kind.label())),
                ConfigRow::Text(_) => None,
            })
    }

    fn config_key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                match self.selected_config_row() {
                    Some(ConfigRow::Field(kind, index)) => self.ask_config_field(kind, index),
                    Some(ConfigRow::Edit(kind)) => self.start_edit(kind),
                    Some(ConfigRow::Backup(kind, index)) => self.restore_config(kind, index),
                    _ => self.describe_config(),
                }
            }
            KeyCode::Char('E') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.start_edit(kind);
                }
            }
            KeyCode::Char('d') => self.describe_config(),
            KeyCode::Char('c') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.check_config(kind);
                }
            }
            KeyCode::Char('s') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.save_draft(kind);
                }
            }
            KeyCode::Char('x') => {
                if let Some(kind) = self.selected_config_kind() {
                    self.discard_draft(kind);
                }
            }
            KeyCode::Char('v') | KeyCode::Char('r') => self.read_configs(),
            KeyCode::Char('h') | KeyCode::Left => self.focus = Focus::Sidebar,
            _ => {}
        }
    }

    fn config_hints(&self, list: &mut Vec<Hint>) {
        list.push(Hint::new(&["↑↓"], "Navigate"));
        match self.selected_config_row() {
            Some(ConfigRow::Field(..)) => list.push(Hint::new(&["Enter"], "Change")),
            Some(ConfigRow::Backup(..)) => list.push(Hint::new(&["Enter"], "Put back")),
            _ => {}
        }
        if let Some(kind) = self.selected_config_kind() {
            list.push(Hint::new(&["c"], "Check"));
            if self
                .config_part(kind)
                .is_some_and(|part| part.draft.is_some())
            {
                list.push(Hint::new(&["s"], "Save"));
                list.push(Hint::new(&["x"], "Discard changes"));
            }
            list.push(Hint::new(&["E"], "Edit in your editor"));
        }
        list.push(Hint::new(&["d"], "Details"));
        list.push(Hint::new(&["r"], "Read again"));
    }

    fn cannot_config(&mut self, why: String) {
        self.popup = Some(Popup::Text(TextPopup::new("Cannot change it", why, true)));
    }

    fn ask_config_field(&mut self, kind: configs::Kind, index: usize) {
        let Some(field) = kind.fields().get(index) else {
            return;
        };
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or(loaded.content);
        let now = configs::values(kind, &text)
            .into_iter()
            .find(|(key, _)| *key == field.key)
            .and_then(|(_, value)| value)
            .unwrap_or_default();
        self.popup = Some(Popup::Ask(
            InputPopup::new(
                format!("{}: {}", kind.label(), field.label),
                format!("{} ({}):", field.key, field.hint),
            )
            .with_value(now)
            .answering("Change"),
            Asked::ConfigField(kind, field.key),
        ));
    }

    /// Change a field in the draft; s saves the draft, after review.
    fn set_config(&mut self, kind: configs::Kind, key: &str, value: &str) {
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        match configs::set(kind, &text, key, value) {
            Ok(after) => self.set_draft(kind, &loaded, after),
            Err(why) => self.cannot_config(why),
        }
    }

    /// Keep text as the draft, or none when it is the file as it is.
    fn set_draft(&mut self, kind: configs::Kind, loaded: &configs::Loaded, after: String) {
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = (after != loaded.content).then_some(after);
            part.draft_checked = None;
        }
        self.build_config_rows();
    }

    /// Check the draft, or the file when there is none, as the window's Check.
    fn check_config(&mut self, kind: configs::Kind) {
        let (_, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let check = self.config_check();
        let folder = self.copies_folder();
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            let checked = configs::validate(
                kind,
                part.draft.as_deref().unwrap_or(&loaded.content),
                &check,
                folder.as_deref(),
            );
            if part.draft.is_some() {
                part.draft_checked = Some(checked);
            } else {
                part.checked = Some(checked);
            }
        }
        self.build_config_rows();
    }

    /// Review saving the draft.
    fn save_draft(&mut self, kind: configs::Kind) {
        let (located, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let draft = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        self.save_config(&located, &loaded, &draft);
    }

    /// Go back to the file as it was read.
    fn discard_draft(&mut self, kind: configs::Kind) {
        if let Some(part) = self.config_parts.iter_mut().find(|part| part.kind == kind) {
            part.draft = None;
            part.draft_checked = None;
        }
        self.build_config_rows();
    }

    /// Check new text and review the plan that saves it.
    fn save_config(&mut self, located: &configs::Located, loaded: &configs::Loaded, after: &str) {
        let Some(folder) = self.copies_folder() else {
            return self.no_copies_folder();
        };
        let checked = configs::validate(located.kind, after, &self.config_check(), Some(&folder));
        if let Checked::Invalid(why) = &checked {
            return self.cannot_config(format!("Not saved: {why}."));
        }
        if loaded.content == after {
            self.popup = Some(Popup::Text(TextPopup::new(
                "Nothing changed",
                "The file is as it was; nothing is saved.",
                true,
            )));
            return;
        }
        match configs::plan_save(located, &loaded.revision, after, &checked, &folder) {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(why) => self.cannot_config(why),
        }
    }

    /// A private copy of the file for the user's editor, opened when the
    /// key has been handled.
    fn start_edit(&mut self, kind: configs::Kind) {
        let (located, loaded) = match self.config_ready(kind) {
            Ok(ready) => ready,
            Err(why) => return self.cannot_config(why),
        };
        let Some(folder) = self.copies_folder() else {
            return self.no_copies_folder();
        };
        // The draft, when there is one, is what the editor starts from.
        let text = self
            .config_part(kind)
            .and_then(|part| part.draft.clone())
            .unwrap_or_else(|| loaded.content.clone());
        let copy = match configs::private_copy(kind, "edit", &text, &folder) {
            Ok(copy) => copy,
            Err(why) => return self.cannot_config(why),
        };
        let editor = ["VISUAL", "EDITOR"]
            .iter()
            .filter_map(|name| std::env::var(name).ok())
            .find(|editor| !editor.trim().is_empty())
            .unwrap_or_else(|| "nano".to_string());
        self.edit_request = Some(EditRequest {
            copy,
            editor,
            located,
            loaded,
        });
    }

    /// The copy to open in the user's editor, once.
    pub fn take_edit(&mut self) -> Option<EditRequest> {
        self.edit_request.take()
    }

    /// The editor closed: check the copy, and review saving it.
    pub fn edited(&mut self, request: EditRequest, closed: Result<(), String>) {
        let after = std::fs::read_to_string(&request.copy);
        let _ = std::fs::remove_file(&request.copy);
        if let Err(why) = closed {
            return self.cannot_config(format!("{why}; nothing is saved."));
        }
        match after {
            Ok(after) => {
                // What came back is the draft, and its save is reviewed.
                self.set_draft(request.located.kind, &request.loaded, after.clone());
                self.save_config(&request.located, &request.loaded, &after)
            }
            Err(error) => self.cannot_config(format!(
                "The edited copy cannot be read: {error}; nothing is saved."
            )),
        }
    }

    /// Put a backup back, after review: the file there now is kept too.
    fn restore_config(&mut self, kind: configs::Kind, index: usize) {
        let Some(backup) = self
            .config_part(kind)
            .and_then(|part| part.history.get(index))
            .cloned()
        else {
            return;
        };
        self.confirm_plan(Ok(backups::plan_restore(&backup)));
    }

    fn describe_config(&mut self) {
        let Some(kind) = self.selected_config_kind() else {
            return;
        };
        let Some(part) = self.config_part(kind) else {
            return;
        };
        let text = match (&part.found, &part.loaded) {
            (Err(why), _) => format!("{why}."),
            (Ok(located), loaded) => {
                let mut text = format!(
                    "{}, named by {}.\n\n{} reads {}{}.",
                    helix_toolbox_core::printable(&located.path.display().to_string()),
                    located.origin.label(),
                    kind.label(),
                    helix_toolbox_core::printable(&located.live.display().to_string()),
                    if located.managed {
                        ", a Home Manager link into the Nix store"
                    } else {
                        ""
                    }
                );
                if let Some(Ok(loaded)) = loaded {
                    text.push_str(&format!(
                        "\n\n{} bytes; SHA-256 {}.",
                        loaded.content.len(),
                        loaded.revision
                    ));
                }
                if let Some(flake) = &located.flake {
                    text.push_str(&format!(
                        "\n\nIt is part of the Home Manager flake in {}: run Home Manager after a change to make it live.",
                        helix_toolbox_core::printable(&flake.display().to_string())
                    ));
                }
                text.push_str("\n\nA save keeps the file there now as a backup beside it, which Backups and helix-toolbox restore can put back.");
                text
            }
        };
        self.popup = Some(Popup::Text(TextPopup::new(
            format!("{}'s configuration", kind.label()),
            text,
            true,
        )));
    }

    fn build_config_rows(&mut self) {
        let mut rows = Vec::new();
        for part in &self.config_parts {
            let kind = part.kind;
            let located = match &part.found {
                Ok(located) => located,
                Err(why) => {
                    rows.push(ConfigRow::Heading(kind.label().to_string()));
                    rows.push(ConfigRow::Text(format!("{why}.")));
                    continue;
                }
            };
            rows.push(ConfigRow::Heading(format!(
                "{}  {}",
                kind.label(),
                located.path.display()
            )));
            let mut about = format!("named by {}", located.origin.label());
            if located.managed {
                about.push_str("; the live file is a Home Manager link");
            }
            if located.flake.is_some() {
                about.push_str("; run Home Manager after a change");
            }
            rows.push(ConfigRow::Text(about));
            match &part.loaded {
                Some(Err(why)) => {
                    rows.push(ConfigRow::Text(format!("{why}.")));
                    continue;
                }
                Some(Ok(loaded)) if !loaded.exists => {
                    rows.push(ConfigRow::Text(
                        "not there yet; a change makes it".to_string(),
                    ));
                }
                _ => {}
            }
            if let Some(checked) = &part.checked {
                rows.push(ConfigRow::Text(match checked {
                    Checked::Valid => "Checks: valid".to_string(),
                    Checked::Unchecked(why) | Checked::Invalid(why) => {
                        format!("Checks: {}: {why}", checked.label())
                    }
                }));
            }
            if part.draft.is_some() {
                rows.push(ConfigRow::Text(
                    "Draft: changed, not saved yet. c checks it, s saves it after review, x discards it."
                        .to_string(),
                ));
                rows.push(ConfigRow::Text(match &part.draft_checked {
                    None => "Draft checks: not checked yet".to_string(),
                    Some(Checked::Valid) => "Draft checks: valid".to_string(),
                    Some(checked @ (Checked::Unchecked(why) | Checked::Invalid(why))) => {
                        format!("Draft checks: {}: {why}", checked.label())
                    }
                }));
            }
            rows.extend((0..kind.fields().len()).map(|index| ConfigRow::Field(kind, index)));
            rows.push(ConfigRow::Edit(kind));
            if !part.history.is_empty() {
                rows.push(ConfigRow::Text(format!(
                    "backups kept ({}), newest first:",
                    part.history.len()
                )));
                rows.extend((0..part.history.len()).map(|index| ConfigRow::Backup(kind, index)));
            }
        }
        self.config_rows = rows;
        self.clamp_main();
    }

    fn draw_configs(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Configs",
            "Ghostty's and Fastfetch's configuration. Each change is checked and shown before it is saved, and the file there now is kept as a backup.",
        );
        self.fill(frame, inner);
        let focused = self.focus == Focus::Main;
        let muted = Style::default().fg(self.theme.muted());
        let mut items: Vec<Line> = Vec::new();
        for row in &self.config_rows {
            items.push(match row {
                ConfigRow::Heading(text) => Line::styled(
                    helix_toolbox_core::printable(text),
                    Style::default().fg(self.theme.group()).bold(),
                ),
                ConfigRow::Text(text) => {
                    let color = if text.starts_with("Checks: Invalid")
                        || text.starts_with("Draft checks: Invalid")
                    {
                        self.theme.failure()
                    } else {
                        self.theme.muted()
                    };
                    Line::styled(
                        format!("  {}", helix_toolbox_core::printable(text)),
                        Style::default().fg(color),
                    )
                }
                ConfigRow::Field(kind, index) => {
                    let field = &kind.fields()[*index];
                    // The draft's value, when there is a draft.
                    let value = self.config_part(*kind).and_then(|part| {
                        let text = match &part.draft {
                            Some(draft) => draft.as_str(),
                            None => part.loaded.as_ref()?.as_ref().ok()?.content.as_str(),
                        };
                        configs::values(*kind, text)
                            .into_iter()
                            .find(|(key, _)| *key == field.key)?
                            .1
                    });
                    Line::from(vec![
                        Span::styled(
                            format!("  {:<26}", field.label),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            value
                                .as_deref()
                                .map(helix_toolbox_core::printable)
                                .unwrap_or_else(|| "(not set)".to_string()),
                            muted,
                        ),
                    ])
                }
                ConfigRow::Edit(_) => Line::styled(
                    "  Edit the whole file in your editor…".to_string(),
                    Style::default().fg(self.theme.entry()),
                ),
                ConfigRow::Backup(kind, index) => {
                    let backup = self
                        .config_part(*kind)
                        .and_then(|part| part.history.get(*index));
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "    {}  ",
                                backup
                                    .and_then(|backup| backup.taken.clone())
                                    .unwrap_or_default()
                            ),
                            Style::default().fg(self.theme.entry()),
                        ),
                        Span::styled(
                            backup
                                .map(|backup| {
                                    helix_toolbox_core::printable(
                                        &backup.backup.display().to_string(),
                                    )
                                })
                                .unwrap_or_default(),
                            muted,
                        ),
                    ])
                }
            });
        }
        let list = List::new(self.measure(items))
            .block(self.card("Configs"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, inner, &mut self.main_state, &self.hits);
    }

    // ----- Offline bundles -----

    fn selected_bundle_row(&self) -> Option<BundleRow> {
        BUNDLE_ROWS.get(self.main_state.selected()?).copied()
    }

    /// A row's label and value; headings have no value.
    fn bundle_row(&self, row: BundleRow) -> (String, String) {
        let yes_no = |on: bool| if on { "yes" } else { "no" }.to_string();
        match row {
            BundleRow::MakeHeading => ("Make a bundle".to_string(), String::new()),
            BundleRow::Entries => (
                "Entries".to_string(),
                match self.queue.len() {
                    0 => "none: the Install Plan is empty".to_string(),
                    1 => "1 from the Install Plan".to_string(),
                    count => format!("{count} from the Install Plan"),
                },
            ),
            BundleRow::Folder => (
                "Folder, new or empty".to_string(),
                self.bundle_folder.clone(),
            ),
            BundleRow::Complete => ("Every dependency".to_string(), yes_no(self.bundle_complete)),
            BundleRow::Contents => ("Check what it needs".to_string(), String::new()),
            BundleRow::Make => ("Make the bundle".to_string(), String::new()),
            BundleRow::InstallHeading => ("Install from a bundle".to_string(), String::new()),
            BundleRow::Source => (
                "The bundle's folder".to_string(),
                if self.bundle_source.is_empty() {
                    "not chosen".to_string()
                } else {
                    self.bundle_source.clone()
                },
            ),
            BundleRow::Trust => (
                "Take the bundle's word for what it holds".to_string(),
                yes_no(self.bundle_trust),
            ),
            BundleRow::Check => ("Check it".to_string(), String::new()),
            BundleRow::Install => ("Install from it".to_string(), String::new()),
        }
    }

    /// What a row is for.
    fn bundle_help(&self, row: BundleRow) -> String {
        match row {
            BundleRow::MakeHeading => format!(
                "The Install Plan's entries and the packages they need for {}, downloaded with Helix Toolbox into a folder. Nothing is installed here.",
                self.platform.pretty_name
            ),
            BundleRow::Entries if self.queue.is_empty() => {
                "Queue the entries to bundle first: Space on an entry puts it in the Install Plan. Enter opens the Install Plan.".to_string()
            }
            BundleRow::Entries => format!(
                "{}. Enter opens the Install Plan.",
                self.queue
                    .iter()
                    .map(|entry| entry.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            BundleRow::Folder => "Where to download to: a new or empty folder. Enter changes it.".to_string(),
            BundleRow::Complete => "Every dependency makes a larger bundle, for a machine set up less than this one. Off, only what this machine lacks.".to_string(),
            BundleRow::Contents => "Asks the package manager what the bundle would download, and downloads nothing.".to_string(),
            BundleRow::Make => "Downloads into the folder once you confirm, with Helix Toolbox itself. Copy the folder over; on the other machine, inside it: ./helix-toolbox bundle install .".to_string(),
            BundleRow::InstallHeading => "A bundle is checked first: made for this distribution and architecture, and every file matching its checksum. Then its packages install from its files, without downloading, all together.".to_string(),
            BundleRow::Source => "The bundle's folder, such as /run/media/you/USB/helix-toolbox-bundle. Enter changes it.".to_string(),
            BundleRow::Trust => "Install Debian packages this machine's repository index does not know, and add a Flatpak remote this machine lacks with the bundle's copy of its key. Only for a bundle you made yourself.".to_string(),
            BundleRow::Check => "Checks what the bundle was made for and every file's checksum, and installs nothing.".to_string(),
            BundleRow::Install => "Checks the bundle, then shows the plan to confirm.".to_string(),
        }
    }

    fn bundle_enter(&mut self) {
        let Some(row) = self.selected_bundle_row() else {
            return;
        };
        match row {
            BundleRow::Entries => {
                self.open_screen(Screen::Plan);
                self.focus = Focus::Main;
            }
            BundleRow::Folder => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new(
                        "Make a bundle",
                        "The folder to download into, new or empty:",
                    )
                    .with_value(self.bundle_folder.clone())
                    .path()
                    .answering("Keep"),
                    Asked::MakeFolder,
                ));
            }
            BundleRow::Complete => self.bundle_complete = !self.bundle_complete,
            BundleRow::Contents | BundleRow::Make if self.queue.is_empty() => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "The Install Plan is empty",
                    "Queue the entries to bundle in the Install Plan first: Space on an entry puts it there.",
                    true,
                )));
            }
            BundleRow::Contents => self.start_bundle_task(
                BundleTask::Contents,
                "Asking the package manager what the bundle needs…",
            ),
            BundleRow::Make => self.make_bundle(),
            BundleRow::Source => {
                self.popup = Some(Popup::Ask(
                    InputPopup::new("Install from a bundle", "The bundle's folder:")
                        .with_value(self.bundle_source.clone())
                        .path()
                        .answering("Keep"),
                    Asked::InstallFolder,
                ));
            }
            BundleRow::Trust => self.bundle_trust = !self.bundle_trust,
            BundleRow::Check | BundleRow::Install if self.bundle_source.trim().is_empty() => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "No bundle chosen",
                    "Choose the bundle's folder first, on the row above.",
                    true,
                )));
            }
            BundleRow::Check => self.start_bundle_task(
                BundleTask::Check,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::Install => self.start_bundle_task(
                BundleTask::Install,
                "Checking every file in the bundle against its checksum…",
            ),
            BundleRow::MakeHeading | BundleRow::InstallHeading => {}
        }
    }

    /// Show what is being done; it is done on the next tick.
    fn start_bundle_task(&mut self, task: BundleTask, notice: &str) {
        self.popup = Some(Popup::Text(TextPopup::new("Offline bundles", notice, true)));
        self.bundle_due = Some(task);
    }

    fn do_bundle_task(&mut self, task: BundleTask) {
        let said = |title: &str, text: String| Some(Popup::Text(TextPopup::new(title, text, true)));
        match task {
            BundleTask::Contents => {
                let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
                let found = bundle::scratch()
                    .map_err(|error| format!("cannot create a scratch folder: {error}"))
                    .and_then(|scratch| {
                        let found = bundle::contents(
                            &ids,
                            &self.catalog,
                            &self.platform,
                            self.bundle_complete,
                            &doctor::Here,
                            &scratch,
                        );
                        let _ = std::fs::remove_dir_all(&scratch);
                        found.map_err(|error| error.to_string())
                    });
                self.popup = match found {
                    Ok(found) => {
                        let mut lines = crate::bundle::contents_lines(&found, self.bundle_complete);
                        if !found.entries.iter().any(|entry| entry.offline) {
                            lines.push(String::new());
                            lines.push(
                                "None of these entries can be bundled; they all need the network."
                                    .to_string(),
                            );
                        }
                        said("What the bundle needs", lines.join("\n"))
                    }
                    Err(error) => said("Cannot tell what it needs", error),
                };
            }
            BundleTask::Check => {
                let checked = typed_path(&self.bundle_source).and_then(|directory| {
                    bundle::verify(&directory, &self.platform)
                        .map(|manifest| crate::bundle::manifest_lines(&directory, &manifest))
                        .map_err(|error| error.to_string())
                });
                self.popup = match checked {
                    Ok(lines) => said("The bundle checks out", lines.join("\n")),
                    Err(error) => said("The bundle does not check out", error),
                };
            }
            BundleTask::Install => {
                let planned = typed_path(&self.bundle_source).and_then(|directory| {
                    let manifest = bundle::verify(&directory, &self.platform)
                        .map_err(|error| error.to_string())?;
                    bundle::plan_install(
                        &directory,
                        &manifest,
                        &[],
                        &self.platform,
                        &doctor::Here,
                        self.bundle_trust,
                    )
                    .map_err(|error| error.to_string())
                });
                self.popup = None;
                match planned {
                    Ok(planned) => self.confirm_plan(Ok(planned)),
                    Err(error) => self.popup = said("Cannot install from it", error),
                }
            }
        }
    }

    /// Plan making a bundle of the Install Plan, which the command line does
    /// in the running window.
    fn make_bundle(&mut self) {
        let ids: Vec<String> = self.queue.iter().map(|entry| entry.id.clone()).collect();
        let planned = typed_path(&self.bundle_folder).and_then(|directory| {
            if std::fs::read_dir(&directory).is_ok_and(|mut listing| listing.next().is_some()) {
                return Err(bundle::BundleError::NotEmpty(directory).to_string());
            }
            let this = std::env::current_exe()
                .map_err(|error| format!("cannot tell where this program is: {error}"))?;
            Ok(bundle::plan_create(
                &this,
                &directory,
                &ids,
                self.bundle_complete,
            ))
        });
        match planned {
            Ok(planned) => self.confirm_plan(Ok(planned)),
            Err(error) => {
                self.popup = Some(Popup::Text(TextPopup::new(
                    "Cannot make the bundle",
                    error,
                    true,
                )))
            }
        }
    }

    fn draw_bundles(&mut self, frame: &mut Frame, area: Rect) {
        let inner = self.page(
            frame,
            area,
            "Offline bundles",
            "Packages downloaded here, for a machine of this distribution without internet.",
        );
        let help = self
            .selected_bundle_row()
            .map(|row| self.bundle_help(row))
            .unwrap_or_default();
        let width = (inner.width.saturating_sub(4) as usize).max(1);
        let height = help.chars().count().div_ceil(width).max(1) as u16 + 2;
        let parts = Layout::vertical([
            Constraint::Min(6),
            Constraint::Length(1),
            Constraint::Length(height),
        ])
        .split(inner);
        let focused = self.focus == Focus::Main;
        let room = parts[0].width.saturating_sub(6) as usize;
        let items: Vec<Line> = BUNDLE_ROWS
            .iter()
            .map(|&row| {
                let (label, value) = self.bundle_row(row);
                if matches!(row, BundleRow::MakeHeading | BundleRow::InstallHeading) {
                    return Line::styled(label, Style::default().fg(self.theme.group()).bold());
                }
                let label = format!("  {label}");
                let value = if value.chars().count() + label.chars().count() + 2 > room {
                    // A long path shows its end.
                    let keep = room.saturating_sub(label.chars().count() + 3);
                    let skip = value.chars().count().saturating_sub(keep);
                    format!("…{}", value.chars().skip(skip).collect::<String>())
                } else {
                    value
                };
                let gap = room.saturating_sub(label.chars().count() + value.chars().count());
                Line::from(vec![
                    Span::styled(label, Style::default().fg(self.theme.entry())),
                    Span::raw(" ".repeat(gap)),
                    Span::styled(value, Style::default().fg(self.theme.focused()).bold()),
                ])
            })
            .collect();
        self.fill(frame, parts[0]);
        let list = List::new(self.measure(items))
            .block(self.card("Offline bundles"))
            .highlight_style(self.highlight(focused))
            .highlight_symbol(self.marker(focused))
            .scroll_padding(1);
        bordered_list(frame, list, parts[0], &mut self.main_state, &self.hits);
        self.fill(frame, parts[2]);
        frame.render_widget(
            Paragraph::new(help)
                .block(self.card("About this"))
                .style(Style::default().fg(self.theme.muted()))
                .wrap(Wrap { trim: true }),
            parts[2],
        );
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
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn clipped_text_ends_at_a_word() {
        assert_eq!(
            clip("Search and install entries", 40),
            "Search and install entries"
        );
        assert_eq!(clip("Search and install entries", 16), "Search and…");
        assert_eq!(clip("Supercalifragilistic", 8), "Superca…");
        assert_eq!(clip("anything", 0), "");
    }

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
        assert!(text.contains("HELIX TOOLBOX"));
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
        // The name and tagline beside the emblem, in the middle rows.
        assert!(
            lines[1].contains("H E L I X   T O O L B O X"),
            "{}",
            lines[1]
        );
        assert!(lines[2].contains("BUILD • CONFIGURE • OPTIMIZE"));
        assert!(!text.contains("HELIX TOOLBOX"));
        // Four rows of picture, then the rule and the sidebar below it.
        assert!(lines[4].contains('─'));
        assert!(lines[6].contains("MAIN MENU"));
    }

    #[test]
    fn the_text_header_returns_without_icons_in_sixteen_colors_or_when_narrow() {
        let mut app = with_banner(app());
        app.icons = false;
        assert!(screen(&mut app, 120, 40).contains("HELIX TOOLBOX"));
        app.icons = true;
        app.theme = Theme::Compatible;
        assert!(screen(&mut app, 120, 40).contains("HELIX TOOLBOX"));
        app.theme = Theme::Solid;
        app.size_bypass = true;
        let narrow = screen(&mut app, 40, 30);
        assert!(!narrow.contains('\u{10EEEE}'));
        assert!(narrow.contains("HELIX TOOLBOX"));
    }

    #[test]
    fn a_small_terminal_gets_a_warning_unless_bypassed() {
        let mut app = app();
        assert!(screen(&mut app, 60, 20).contains("needs 100 by 25"));
        app.size_bypass = true;
        assert!(screen(&mut app, 60, 20).contains("HELIX TOOLBOX"));
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

    /// Where `text` is drawn, as the column and row of its first cell.
    fn find(app: &mut App, text: &str) -> (u16, u16) {
        let drawn = screen(app, 120, 40);
        for (row, line) in drawn.lines().enumerate() {
            if let Some(at) = line.find(text) {
                return (line[..at].chars().count() as u16, row as u16);
            }
        }
        panic!("{text:?} is not on the screen:\n{drawn}");
    }

    fn mouse_at(app: &mut App, kind: MouseEventKind, (column, row): (u16, u16)) -> bool {
        app.handle_mouse(&MouseEvent {
            kind,
            column,
            row,
            modifiers: KeyModifiers::NONE,
        })
    }

    fn click(app: &mut App, text: &str) -> bool {
        let at = find(app, text);
        mouse_at(app, MouseEventKind::Down(MouseButton::Left), at)
    }

    /// A click on the lowest place `text` is drawn, such as a result below
    /// the search that found it.
    fn click_last(app: &mut App, text: &str) -> bool {
        let drawn = screen(app, 120, 40);
        let (column, row) = drawn
            .lines()
            .enumerate()
            .filter_map(|(row, line)| {
                line.find(text)
                    .map(|at| (line[..at].chars().count() as u16, row as u16))
            })
            .last()
            .unwrap_or_else(|| panic!("{text:?} is not on the screen:\n{drawn}"));
        mouse_at(app, MouseEventKind::Down(MouseButton::Left), (column, row))
    }

    #[test]
    fn clicks_open_screens_and_select_rows_and_a_second_click_is_enter() {
        let mut app = app();
        let area = app.menu[1].label.clone();
        click(&mut app, &area);
        assert_eq!(app.screen, Screen::Area(0));
        assert_eq!(app.focus, Focus::Sidebar);
        // A row of the list: selected, by the keys, and the list focused.
        click(&mut app, "Developer Tools");
        assert_eq!(app.focus, Focus::Main);
        let row = app.main_state.selected().unwrap();
        assert!(
            matches!(app.selected_row(), Some(Row::Group { .. })),
            "row {row}"
        );
        assert!(app.at_root());
        click(&mut app, "Developer Tools");
        assert_eq!(
            app.areas[0].path.len(),
            1,
            "the second click opens the group"
        );
        // On an entry, the second click asks first, as Enter does; a click
        // beside the confirmation answers nothing.
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "System Report (script)");
        let name = "System Report (script)";
        click_last(&mut app, name);
        assert_eq!(app.focus, Focus::Main);
        assert!(app.popup.is_none());
        click_last(&mut app, name);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        click(&mut app, &area);
        mouse_at(&mut app, MouseEventKind::Down(MouseButton::Left), (60, 20));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert_eq!(app.screen, Screen::Area(0));
        // Its hints are its keys: n cancels.
        click(&mut app, "n/q/Esc");
        assert!(app.popup.is_none());
    }

    #[test]
    fn the_wheel_scrolls_the_list_and_popups_and_hints_are_keys() {
        let mut app = app();
        press(&mut app, KeyCode::Char('j'));
        let list = find(&mut app, "Developer Tools");
        mouse_at(&mut app, MouseEventKind::ScrollDown, list);
        assert_eq!(app.focus, Focus::Main);
        let before = app.main_state.selected().unwrap();
        mouse_at(&mut app, MouseEventKind::ScrollDown, list);
        assert_eq!(app.main_state.selected(), Some(before + 1));
        mouse_at(&mut app, MouseEventKind::ScrollUp, list);
        assert_eq!(app.main_state.selected(), Some(before));
        // The wheel over the sidebar changes no screen.
        let sidebar = find(&mut app, "MAIN MENU");
        mouse_at(&mut app, MouseEventKind::ScrollDown, sidebar);
        assert_eq!(app.screen, Screen::Area(0));
        // The ? hint is the ? key; the wheel scrolls the help it opens.
        click(&mut app, "? Help");
        let Some(Popup::Text(_)) = &app.popup else {
            panic!("the help did not open")
        };
        mouse_at(&mut app, MouseEventKind::ScrollDown, (60, 20));
        let Some(Popup::Text(help)) = &app.popup else {
            panic!("the wheel closed the help")
        };
        assert_eq!(help.scroll(), 1);
        click(&mut app, "Enter/q/Esc");
        assert!(app.popup.is_none());
        // And q quits.
        assert!(!click(&mut app, "q Quit"));
    }

    #[test]
    fn the_mouse_can_be_turned_off_in_settings() {
        let mut app = app();
        assert!(app.mouse());
        app.open_screen(Screen::Settings);
        press(&mut app, KeyCode::Enter);
        click(&mut app, "Mouse");
        click(&mut app, "Mouse");
        assert!(!app.mouse(), "{}", screen(&mut app, 120, 40));
        assert!(screen(&mut app, 120, 40).contains("off"));
        // Off, clicks do nothing at all.
        let area = app.menu[1].label.clone();
        click(&mut app, &area);
        assert_eq!(app.screen, Screen::Settings);
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

    /// Replace what an `Ask` popup holds with `text`.
    fn answer(app: &mut App, text: &str) {
        for _ in 0..200 {
            press(app, KeyCode::Backspace);
        }
        type_text(app, text);
        press(app, KeyCode::Enter);
    }

    #[test]
    fn your_profiles_are_imported_exported_and_deleted_in_the_profiles_area() {
        let mut app = app();
        let directory = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        app.profiles_dir = Some(directory.path().to_path_buf());
        app.queue = vec![app.catalog.find("git").unwrap().clone()];
        app.save_profile("Dev Box");
        app.popup = None;
        let area = area_of(&app, "profile-dev-box");
        app.open_screen(Screen::Area(area));
        app.focus = Focus::Main;
        assert!(screen(&mut app, 140, 40).contains("o Import"));
        // Into Your profiles, then onto the profile.
        press(&mut app, KeyCode::Enter);
        app.main_state.select(Some(1));
        assert_eq!(app.selected_entry().unwrap().id, "profile-dev-box");
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("x Export"), "{shown}");

        press(&mut app, KeyCode::Char('x'));
        assert!(screen(&mut app, 140, 40).contains("~/dev-box.toml"));
        let exported = elsewhere.path().join("shared.toml");
        answer(&mut app, &exported.display().to_string());
        assert!(screen(&mut app, 140, 40).contains("Profile exported"));
        assert!(std::fs::read_to_string(&exported).unwrap().contains("git"));
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('x'));
        answer(&mut app, "shared.toml");
        assert!(screen(&mut app, 140, 40).contains("Give the full path"));
        press(&mut app, KeyCode::Esc);

        // Deleting needs the word.
        press(&mut app, KeyCode::Delete);
        answer(&mut app, "no");
        assert!(screen(&mut app, 140, 40).contains("Not deleted"));
        assert!(directory.path().join("dev-box.toml").exists());
        press(&mut app, KeyCode::Esc);
        app.queue = vec![app.catalog.find("profile-dev-box").unwrap().clone()];
        press(&mut app, KeyCode::Delete);
        answer(&mut app, "yes");
        assert!(screen(&mut app, 140, 40).contains("Profile deleted"));
        assert!(!directory.path().join("dev-box.toml").exists());
        assert!(app.catalog.find("profile-dev-box").is_none());
        assert!(app.queue.is_empty(), "it leaves the Install Plan");
        assert!(app.areas[area].path.is_empty(), "its group is gone");
        press(&mut app, KeyCode::Esc);

        // And it comes back from the exported file.
        press(&mut app, KeyCode::Char('o'));
        answer(&mut app, &exported.display().to_string());
        assert!(screen(&mut app, 140, 40).contains("Profile imported"));
        assert!(app.catalog.find("profile-dev-box").is_some());
        assert!(directory.path().join("dev-box.toml").exists());
    }

    #[test]
    fn offline_bundles_are_made_from_the_install_plan_and_checked_before_installing() {
        let mut app = app();
        let folder = tempfile::tempdir().unwrap();
        app.open_screen(Screen::Bundles);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "Offline bundles",
            "Make a bundle",
            "Install from a bundle",
            "none: the Install Plan is empty",
            "~/helix-toolbox-bundle",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        let row = |wanted: BundleRow| BUNDLE_ROWS.iter().position(|&row| row == wanted);
        app.main_state.select(row(BundleRow::Make));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("The Install Plan is empty"));
        press(&mut app, KeyCode::Esc);

        app.queue = vec![app.catalog.find("git").unwrap().clone()];
        app.main_state.select(row(BundleRow::Folder));
        press(&mut app, KeyCode::Enter);
        let target = folder.path().join("bundle");
        answer(&mut app, &target.display().to_string());
        assert_eq!(app.bundle_folder, target.display().to_string());
        app.main_state.select(row(BundleRow::Complete));
        press(&mut app, KeyCode::Enter);
        assert!(app.bundle_complete);
        app.main_state.select(row(BundleRow::Make));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let planned = app.pending.as_ref().unwrap();
        let command = planned.steps[0].display();
        assert!(
            command.contains("bundle create") && command.contains("--complete"),
            "{command}"
        );
        press(&mut app, KeyCode::Char('n'));
        // A folder with something in it is not used.
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("keep.txt"), "mine").unwrap();
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("Cannot make the bundle"));
        press(&mut app, KeyCode::Esc);

        // Installing checks the bundle first.
        app.main_state.select(row(BundleRow::Install));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("No bundle chosen"));
        press(&mut app, KeyCode::Esc);
        app.main_state.select(row(BundleRow::Source));
        press(&mut app, KeyCode::Enter);
        answer(&mut app, &target.display().to_string());
        app.main_state.select(row(BundleRow::Check));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("Checking every file"));
        assert!(app.needs_redraw());
        assert!(screen(&mut app, 140, 40).contains("The bundle does not check out"));
        press(&mut app, KeyCode::Esc);
        app.main_state.select(row(BundleRow::Install));
        press(&mut app, KeyCode::Enter);
        assert!(app.needs_redraw());
        assert!(screen(&mut app, 140, 40).contains("Cannot install from it"));
        assert!(app.pending.is_none());
    }

    #[test]
    fn settings_make_a_bug_report_and_keep_a_copy() {
        let mut app = app();
        let directory = tempfile::tempdir().unwrap();
        app.reports_dir = Some(directory.path().to_path_buf());
        app.open_screen(Screen::Settings);
        app.focus = Focus::Main;
        app.main_state.select(Some(11));
        assert!(screen(&mut app, 120, 40).contains("Bug report, to paste into an issue"));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("Gathering the report"));
        let start = std::time::Instant::now();
        while app.reporting.is_some() && start.elapsed() < std::time::Duration::from_secs(20) {
            app.needs_redraw();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let shown = screen(&mut app, 120, 40);
        assert!(shown.contains("Paste it into a new issue"), "{shown}");
        let saved = std::fs::read_to_string(directory.path().join("bug-report.md")).unwrap();
        assert!(saved.contains("### Helix Toolbox report"), "{saved}");
        // c copies the report itself, not the words around it.
        assert!(shown.contains("c Copy"), "{shown}");
        app.copier = |text| {
            if text.starts_with("### Helix Toolbox report") {
                Ok("Copied with wl-copy.".into())
            } else {
                Err(format!("not the report: {text}"))
            }
        };
        press(&mut app, KeyCode::Char('c'));
        assert!(screen(&mut app, 120, 40).contains("Copied with wl-copy."));
        assert!(matches!(app.popup, Some(Popup::Text(_))));
        let (choices, _) = app.palette_choices();
        assert!(
            choices
                .iter()
                .any(|choice| choice.label == "Make a bug report")
        );
    }

    #[test]
    fn quick_actions_open_screens() {
        let mut app = app();
        press(&mut app, KeyCode::Enter);
        for _ in 0..4 {
            press(&mut app, KeyCode::Char('j'));
        }
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Doctor);
        assert!(screen(&mut app, 120, 40).contains("Machine Summary"));
    }

    #[test]
    fn quick_actions_set_up_the_machine_and_find_anything() {
        let mut app = app();
        press(&mut app, KeyCode::Enter);
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("Set up this machine"), "{text}");
        assert!(text.contains("Find anything"), "{text}");
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Wizard(_))));
        app.popup = None;
        press(&mut app, KeyCode::Char('j'));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Palette(_))));
    }

    #[test]
    fn the_dashboard_counts_what_is_installed_queued_and_pinned() {
        let mut app = app();
        app.statuses.insert(
            "ripgrep".to_string(),
            Status::Installed {
                source: "pacman".to_string(),
                version: Some("15.2.0-1".to_string()),
                shadowed: None,
            },
        );
        app.favorites.push("marker".to_string());
        let text = screen(&mut app, 120, 40);
        assert!(text.contains("1 installed"), "{text}");
        assert!(text.contains("0 updates"), "{text}");
        assert!(text.contains("0 queued"), "{text}");
        assert!(text.contains("1 pinned"), "{text}");
        // Taller, the counts are tiles.
        let tall = screen(&mut app, 120, 60);
        assert!(tall.contains("╭"), "{tall}");
        assert!(tall.contains("1 installed"), "{tall}");
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
            screen(&mut app, 140, 40).contains("Installed from: Home Manager 0.74.4"),
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
            Some(helix_toolbox_core::PackageManager::Arch),
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
        assert!(text.contains("helix-toolbox native system-report"));
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
        let path = directory.path().join("favorites.toml");
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
        let mut launcher = channel("helixforge", "Helix Toolbox", Some(Vec::new()));
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
    fn a_run_that_worked_becomes_a_hardware_report_to_send() {
        let state = tempfile::tempdir().unwrap();
        let reports = tempfile::tempdir().unwrap();
        let mut app = app();
        app.reports_dir = Some(reports.path().to_path_buf());
        app.machine = Some(hardware_report::Machine {
            distribution: "Garuda Linux".into(),
            model: Some("Lenovo ThinkPad T15p Gen 2i".into()),
            graphics: vec!["NVIDIA GeForce GTX 1650 Mobile / Max-Q (nvidia)".into()],
            architecture: "x86_64".into(),
            ..Default::default()
        });
        history::record_in(
            state.path(),
            &[("git".into(), "Git".into())],
            false,
            "exit 1",
            None,
        )
        .unwrap();
        history::record_in(
            state.path(),
            &[("git".into(), "Git".into()), ("meld".into(), "Meld".into())],
            true,
            "exit status 0",
            None,
        )
        .unwrap();
        app.records = history::read_in(state.path(), 10).unwrap();
        app.open_screen(Screen::History);
        app.records = history::read_in(state.path(), 10).unwrap();
        app.focus = Focus::Main;
        let successful = app
            .records
            .iter()
            .position(|record| record.success)
            .unwrap();
        app.main_state.select(Some(1 - successful));
        assert!(!screen(&mut app, 140, 40).contains("Report it worked"));
        press(&mut app, KeyCode::Char('w'));
        assert!(screen(&mut app, 140, 40).contains("No hardware report"));
        press(&mut app, KeyCode::Esc);

        app.main_state.select(Some(successful));
        assert!(screen(&mut app, 140, 40).contains("w Report it worked"));
        press(&mut app, KeyCode::Char('w'));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("[[hardware]]"), "{shown}");
        assert!(shown.contains("o Open the issue"), "{shown}");
        let Some(Popup::HardwareReport(_, url)) = &app.popup else {
            panic!("no hardware report");
        };
        assert!(url.starts_with(hardware_report::ISSUES));
        assert!(
            url.contains("entry%20%3D%20%22meld%22"),
            "both entries: {url}"
        );
        let saved = std::fs::read_to_string(reports.path().join("hardware-report.md")).unwrap();
        assert!(saved.contains(url.as_str()));
        // c copies the report with its issue link, as the window's Copy does.
        app.copier = |text| {
            if text.contains("[[hardware]]") && text.contains("The issue, filled in:") {
                Ok("Copied with xclip.".into())
            } else {
                Err(format!("not the report: {text}"))
            }
        };
        press(&mut app, KeyCode::Char('c'));
        assert!(screen(&mut app, 140, 40).contains("Copied with xclip."));
        assert!(matches!(app.popup, Some(Popup::HardwareReport(..))));
        app.opener = |url| {
            if url.starts_with(hardware_report::ISSUES) {
                Ok(())
            } else {
                Err("not the issue".into())
            }
        };
        press(&mut app, KeyCode::Char('o'));
        assert!(screen(&mut app, 140, 40).contains("open in your browser"));
        press(&mut app, KeyCode::Esc);

        // From Reliability, one entry's.
        app.open_screen(Screen::Reliability);
        app.focus = Focus::Main;
        let row = app
            .reliability_rows
            .iter()
            .position(|(id, _, _)| id == "meld")
            .unwrap();
        app.main_state.select(Some(row));
        assert!(screen(&mut app, 140, 40).contains("w Report it worked"));
        press(&mut app, KeyCode::Char('w'));
        let Some(Popup::HardwareReport(_, url)) = &app.popup else {
            panic!("no hardware report");
        };
        assert!(url.contains("entry%20%3D%20%22meld%22") && !url.contains("%22git%22"));
    }

    #[test]
    fn c_checks_an_entry_now_and_says_when_there_is_nothing_to_check() {
        let mut app = app();
        let area = area_of(&app, "git");
        app.open_screen(Screen::Area(area));
        app.focus = Focus::Main;
        app.search = "git".chars().collect();
        app.update_rows();
        let row = app
            .rows
            .iter()
            .position(|row| matches!(row, Row::Entry(entry) if entry.id == "git"))
            .unwrap();
        app.main_state.select(Some(row));
        assert!(screen(&mut app, 140, 40).contains("c Check"));
        press(&mut app, KeyCode::Char('c'));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("Checks: Git"), "{shown}");
        assert!(
            shown.contains("verified: Git") || shown.contains("Verified: Git"),
            "a package entry is checked for its package: {shown}"
        );
        press(&mut app, KeyCode::Esc);
        // An entry that installs nothing it can find and declares no checks.
        let nothing = app
            .catalog
            .entries()
            .into_iter()
            .find(|entry| {
                matches!(entry.action, Action::Script { .. })
                    && entry.detect.is_none()
                    && entry.effects.checks.is_empty()
            })
            .unwrap()
            .clone();
        app.rows = vec![Row::Entry(Box::new(nothing))];
        app.main_state.select(Some(0));
        press(&mut app, KeyCode::Char('c'));
        assert!(screen(&mut app, 140, 40).contains("nothing to check"));
    }

    fn stand_in_services() -> services::Services {
        let unit = |name: &str, enablement: &str, active: &str, user: bool| services::Unit {
            name: name.into(),
            description: format!("About {name}"),
            enablement: Some(enablement.into()),
            active: Some(active.into()),
            sub: Some(
                if active == "active" {
                    "running"
                } else {
                    "dead"
                }
                .into(),
            ),
            user,
        };
        services::Services {
            systemd: true,
            session: true,
            units: vec![
                unit("broken.service", "enabled", "failed", false),
                unit("fstrim.timer", "disabled", "inactive", false),
                unit("sddm.service", "enabled", "active", false),
                unit("pipewire.service", "indirect", "active", true),
            ],
            autostart: vec![services::Autostart {
                file: "discover.desktop".into(),
                name: "Discover".into(),
                exec: "discover --notifier".into(),
                enabled: true,
                here: true,
                system: true,
                ..Default::default()
            }],
        }
    }

    #[test]
    fn the_services_screen_lists_units_and_asks_before_stopping_what_is_needed() {
        let mut app = app();
        app.services_found = Some(stand_in_services());
        app.open_screen(Screen::Services);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("The machine's services (2)"), "{shown}");
        assert!(shown.contains("sddm.service") && !shown.contains("pipewire.service"));
        let row = |app: &App, name: &str| {
            app.service_rows
                .iter()
                .position(|row| matches!(row, ServiceRow::Unit(index) if app.unit(*index).unwrap().name == name))
                .unwrap()
        };
        app.main_state.select(Some(row(&app, "sddm.service")));
        assert!(screen(&mut app, 140, 40).contains("s Stop"));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("login screen"));
        press(&mut app, KeyCode::Esc);
        // Disabling the login screen is high risk, and says why first.
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("Risk: high") && shown.contains("Warning: It runs the login screen"),
            "{shown}"
        );
        assert!(shown.contains("sudo systemctl disable sddm.service"));
        press(&mut app, KeyCode::Char('n'));
        // The views: timers, your session's, at login, failed.
        press(&mut app, KeyCode::Char('i'));
        assert!(screen(&mut app, 140, 40).contains("fstrim.timer"));
        press(&mut app, KeyCode::Char('i'));
        assert!(screen(&mut app, 140, 40).contains("pipewire.service"));
        press(&mut app, KeyCode::Char('i'));
        assert!(screen(&mut app, 140, 40).contains("Discover"));
        press(&mut app, KeyCode::Char('i'));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("broken.service") && !shown.contains("sddm.service"));
        // The filter narrows any view.
        press(&mut app, KeyCode::Char('i'));
        press(&mut app, KeyCode::Char('f'));
        answer(&mut app, "sddm");
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("The machine's services (1)") && shown.contains("with \"sddm\""),
            "{shown}"
        );
    }

    #[test]
    fn the_storage_screen_shows_what_each_cleanup_frees_and_what_stays() {
        let mut app = app();
        app.storage_found = Some(storage::Storage {
            filesystems: vec![storage::Filesystem {
                path: "/".into(),
                free: 40_000_000_000,
                total: 500_000_000_000,
            }],
            items: vec![
                storage::Item {
                    id: "package-cache".into(),
                    title: "Package cache".into(),
                    detail: "Downloaded packages.".into(),
                    size: Some(2_000_000_000),
                    frees: Some(1_500_000_000),
                    ..Default::default()
                },
                storage::Item {
                    id: "trash".into(),
                    title: "The trash".into(),
                    detail: "Deleted files.".into(),
                    size: Some(0),
                    not_offered: Some("the trash is empty".into()),
                    ..Default::default()
                },
            ],
            kept: vec![storage::Kept {
                title: "Kernels".into(),
                detail: "2 installed: never removed here.".into(),
                size: None,
            }],
        });
        app.open_screen(Screen::Storage);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "40.0 GB free of 500 GB",
            "frees 1.5 GB",
            "the trash is empty",
            "Kept, never cleaned here",
            "Kernels",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        let row = |app: &App, wanted: usize| {
            app.storage_rows
                .iter()
                .position(|row| matches!(row, StorageRow::Item(index) if *index == wanted))
                .unwrap()
        };
        app.main_state.select(Some(row(&app, 1)));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("not offered"));
        press(&mut app, KeyCode::Esc);
        app.main_state.select(Some(row(&app, 0)));
        assert!(screen(&mut app, 140, 40).contains("Enter Clean"));
        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 140, 40).contains("frees about 1.5 GB"));
    }

    #[test]
    fn the_performance_screen_sets_this_boot_beside_an_earlier_one() {
        let mut app = app();
        let boot = |id: &str, booted: u64, total: u64, waited: u64| performance::Boot {
            id: id.into(),
            booted,
            kernel: format!("7.2.{booted}"),
            phases: performance::Phases {
                kernel: 1000,
                userspace: total - 1000,
                total,
                ..Default::default()
            },
            slowest: vec![performance::Timed {
                name: "plocate-updatedb.service".into(),
                took: 4183,
            }],
            chain: vec![performance::Link {
                name: "NetworkManager-wait-online.service".into(),
                at: Some(1000),
                took: Some(waited),
            }],
        };
        let now = boot("now", 3000, 20_000, 6000);
        let before = boot("before", 2000, 15_000, 1000);
        let oldest = boot("oldest", 1000, 30_000, 1000);
        let found = performance::Performance {
            previous: Some(performance::compare(&before, &now, &[])),
            hints: performance::hints(Some(&now), &performance::Now::default()),
            boot: Some(now.clone()),
            recorded: 3,
            ..Default::default()
        };
        app.performance_found = Some((found, vec![now, before, oldest]));
        app.open_screen(Screen::Performance);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 50);
        for text in [
            "Kernel 1.0 s · Services 19.0 s",
            "Against the boot of 1970-01-01 00:33 UTC: +5.0 s",
            "NetworkManager-wait-online.service",
            "(+5.0 s)",
            "The login screen waited for",
            "The boot waited 6.0 s for NetworkManager-wait-online.service",
            "Enter turns it off",
            "b An earlier boot",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // b sets it beside the boot before that one, and back.
        press(&mut app, KeyCode::Char('b'));
        let shown = screen(&mut app, 140, 50);
        assert!(
            shown.contains("Against the boot of 1970-01-01 00:16 UTC: -10.0 s"),
            "{shown}"
        );
        press(&mut app, KeyCode::Char('b'));
        assert!(screen(&mut app, 140, 50).contains("+5.0 s"));

        let hint = app
            .performance_rows
            .iter()
            .position(|row| matches!(row, PerformanceRow::Hint(_)))
            .unwrap();
        app.main_state.select(Some(hint));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 50);
        assert!(
            shown.contains("sudo systemctl disable NetworkManager-wait-online.service"),
            "{shown}"
        );
        press(&mut app, KeyCode::Char('n'));
        // A unit opens in Services, filtered to it.
        let unit = app
            .performance_rows
            .iter()
            .position(|row| matches!(row, PerformanceRow::Unit(name, _) if name == "plocate-updatedb.service"))
            .unwrap();
        app.main_state.select(Some(unit));
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Services);
        assert_eq!(app.service_filter, "plocate-updatedb.service");
    }

    #[test]
    fn the_recovery_screen_says_what_is_ready_and_offers_the_snapshot_check() {
        let mut app = app();
        let check =
            |id: &str, title: &str, health, summary: &str, entry: Option<&str>| doctor::Diagnosis {
                id: id.into(),
                title: title.into(),
                health,
                summary: summary.into(),
                evidence: vec!["The root file system is btrfs".into()],
                advice: None,
                repair: None,
                entry: entry.map(str::to_string),
            };
        app.recovery_found = vec![
            check(
                "snapshots",
                "System snapshots",
                doctor::Health::Good,
                "Snapper has 2 snapshots",
                None,
            ),
            check(
                "own-files",
                "Your own files",
                doctor::Health::Notice,
                "Nothing Helix Toolbox knows of backs up your home folder",
                None,
            ),
            check(
                "second-kernel",
                "A second kernel",
                doctor::Health::Notice,
                "Only one kernel",
                Some("zram-swap"),
            ),
        ];
        app.snapshot_tool = Some(backups::Snapshots::Snapper);
        app.open_screen(Screen::Recovery);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "Recovery",
            "Snapper has 2 snapshots",
            "c checks them",
            "Notice  Your own files",
            "f sets it up",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("looks inside the newest"), "{shown}");
        press(&mut app, KeyCode::Esc);
        press(&mut app, KeyCode::Char('c'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("sudo snapper -c root list"), "{shown}");
        press(&mut app, KeyCode::Char('n'));
        app.main_state.select(Some(2));
        press(&mut app, KeyCode::Char('f'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    }

    /// A migration made on Fedora, written where a test can open it.
    fn fedora_migration(directory: &Path) -> PathBuf {
        let path = directory.join("from-fedora.toml");
        std::fs::write(
            &path,
            "version = 1\nfrom = \"Fedora Linux 43\"\nfamily = \"fedora\"\narchitecture = \"x86_64\"\nmade = 0\nentries = [\"git\", \"no-such-entry\"]\nflatpaks = [\"com.spotify.Client\"]\npackages = [\"python3-requests\"]\n",
        )
        .unwrap();
        path
    }

    /// restic answering from a fixed repository, for the Restic screen's
    /// tests.
    struct FakeRestic;

    impl Restic for FakeRestic {
        fn run(&self, args: &[String]) -> Result<String, String> {
            let line = args.join(" ");
            if line.contains(" ls ") {
                Ok([
                    r#"{"struct_type":"snapshot","id":"b6f77f6d00"}"#,
                    r#"{"name":"me","type":"dir","path":"/home/me","struct_type":"node"}"#,
                    r#"{"name":"Documents","type":"dir","path":"/home/me/Documents","struct_type":"node"}"#,
                    r#"{"name":"notes.txt","type":"file","path":"/home/me/notes.txt","size":6,"struct_type":"node"}"#,
                ]
                .join("\n"))
            } else if line.contains(" snapshots ") {
                Ok(r#"[{"id":"b6f77f6d00","short_id":"b6f77f6d","time":"2026-10-21T22:00:04+02:00","hostname":"t15p","paths":["/home/me"]},{"id":"aa0d9cfe00","short_id":"aa0d9cfe","time":"2026-10-20T22:00:04+02:00","hostname":"t15p","paths":["/home/me"]}]"#.to_string())
            } else if line.contains(" find ") {
                Ok(r#"[{"matches":[{"type":"file","path":"/home/me/notes.txt","size":6}],"snapshot":"b6f77f6d00"},{"matches":[{"type":"file","path":"/home/me/notes.txt","size":5}],"snapshot":"aa0d9cfe00"}]"#.to_string())
            } else {
                Err(format!("not answered: {line}"))
            }
        }
    }

    fn restic_app(baselines: Vec<restic::Baseline>) -> App {
        let mut app = app();
        app.restic = std::sync::Arc::new(FakeRestic);
        let config = restic::Config {
            repository: "/run/media/me/Backup/restic".into(),
            origin: restic::Origin::Settings,
            password: restic::Password::File("/home/me/.restic-password".into()),
            paths: vec!["/home/me".into()],
            excludes: Vec::new(),
        };
        app.restic_reading = Some(ResticReading {
            snapshots: restic::snapshots(&FakeRestic, &config),
            found: restic::Found::Ready(config),
            restores: true,
            timer: None,
            baselines,
        });
        app
    }

    #[test]
    fn the_restic_screen_lists_snapshots_browses_them_and_finds_a_files_history() {
        let mut app = app();
        app.restic_reading = Some(ResticReading {
            found: restic::Found::Missing,
            snapshots: Ok(Vec::new()),
            restores: false,
            timer: None,
            baselines: Vec::new(),
        });
        app.open_screen(Screen::Restic);
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("No restic repository is set up"), "{shown}");
        let mut app = restic_app(Vec::new());
        app.open_screen(Screen::Restic);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "/run/media/me/Backup/restic (from Helix Toolbox's settings)",
            "Snapshots (2)",
            "b6f77f6d  2026-10-21 22:00",
            "Recovery tests (0 baselines)",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        let first = app
            .restic_rows
            .iter()
            .position(|row| matches!(row, ResticRow::Snapshot(0)))
            .unwrap();
        app.main_state.select(Some(first));
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 140, 40);
        for text in ["In b6f77f6d at /home/me", "Documents/", "notes.txt", ".."] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        press(&mut app, KeyCode::Char('s'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::ResticHistory))
        ));
        type_text(&mut app, "notes.txt");
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 140, 40);
        for text in [
            "History of notes.txt",
            "/home/me/notes.txt",
            "changed",
            "first",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
    }

    #[test]
    fn the_restic_screen_reviews_restores_backups_and_recovery_tests_before_they_run() {
        let baseline = restic::Baseline {
            id: "1792600000000".into(),
            path: "/home/me/notes.txt".into(),
            sha256: "5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03".into(),
            recorded: 1_792_600_000,
        };
        let mut app = restic_app(vec![baseline]);
        app.open_screen(Screen::Restic);
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('b'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 140, 40).contains("'backup' '--exclude-caches'"));
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
        // A restore asks for a new folder, which must not be there yet.
        let first = app
            .restic_rows
            .iter()
            .position(|row| matches!(row, ResticRow::Snapshot(0)))
            .unwrap();
        app.main_state.select(Some(first));
        press(&mut app, KeyCode::Char('R'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::ResticRestore(..)))
        ));
        let home = std::env::var("HOME").unwrap();
        for _ in 0.."~/restored".len() {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(&mut app, &home);
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 140, 40).contains("is there already"));
        press(&mut app, KeyCode::Esc);
        // v on a baseline tests a restore of it, after review.
        let baseline = app
            .restic_rows
            .iter()
            .position(|row| matches!(row, ResticRow::Baseline(0)))
            .unwrap();
        app.main_state.select(Some(baseline));
        press(&mut app, KeyCode::Char('v'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::ResticTest(_)))
        ));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        assert!(
            shown.contains("Test a restore against its baseline"),
            "{shown}"
        );
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
    }

    /// An app reading configurations in a home of its own: a Ghostty
    /// configuration with a backup beside it, and a Fastfetch one that does
    /// not check.
    fn configs_app(home: &Path) -> App {
        let config = home.join(".config");
        std::fs::create_dir_all(config.join("ghostty")).unwrap();
        std::fs::create_dir_all(config.join("fastfetch")).unwrap();
        std::fs::write(config.join("ghostty/config"), "font-size = 11\n").unwrap();
        std::fs::write(
            config.join("ghostty/config.helix-toolbox-backup-20261001000000-1"),
            "font-size = 10\n",
        )
        .unwrap();
        std::fs::write(
            config.join("fastfetch/config.jsonc"),
            "{ \"modules\": \"os\" }\n",
        )
        .unwrap();
        let mut app = app();
        app.config_places = Some((home.to_path_buf(), config));
        app.config_folder = Some(home.join("copies"));
        app
    }

    #[test]
    fn the_configs_screen_shows_both_files_their_checks_fields_and_backups() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().canonicalize().unwrap();
        let mut app = configs_app(&home);
        app.open_screen(Screen::Configs);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 160, 50);
        for text in [
            "Ghostty",
            "named by the live file",
            "Checks: Not checked: Ghostty is not installed here",
            "Font size",
            "11",
            "Edit the whole file in your editor",
            "backups kept (1), newest first:",
            "2026-10-01 00:00:00",
            "Fastfetch",
            "Checks: Invalid: \"modules\" must be a list",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // d tells where it is and what a save keeps.
        let field = app
            .config_rows
            .iter()
            .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
            .unwrap();
        app.main_state.select(Some(field));
        press(&mut app, KeyCode::Char('d'));
        let told = screen(&mut app, 160, 50);
        assert!(told.contains("SHA-256"), "{told}");
        assert!(told.contains("as a backup beside it"), "{told}");
    }

    #[test]
    fn the_configs_screen_reviews_a_change_an_edit_and_a_backup_before_anything_is_saved() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().canonicalize().unwrap();
        let file = home.join(".config/ghostty/config");
        let mut app = configs_app(&home);
        app.open_screen(Screen::Configs);
        app.focus = Focus::Main;
        // Enter on a field asks for its value, then reviews the save.
        let field = app
            .config_rows
            .iter()
            .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
            .unwrap();
        app.main_state.select(Some(field));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(
                _,
                Asked::ConfigField(configs::Kind::Ghostty, "font-size")
            ))
        ));
        press(&mut app, KeyCode::Backspace);
        press(&mut app, KeyCode::Backspace);
        type_text(&mut app, "14");
        press(&mut app, KeyCode::Enter);
        // It goes into the draft, and s reviews saving it.
        assert!(app.popup.is_none());
        let shown = screen(&mut app, 160, 50);
        assert!(shown.contains("Draft: changed, not saved yet"), "{shown}");
        assert!(shown.contains("Font size                 14"), "{shown}");
        press(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 160, 50);
        assert!(shown.contains("Save Ghostty's configuration"), "{shown}");
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "font-size = 11\n");
        // E hands a private copy to the editor; what comes back is reviewed.
        press(&mut app, KeyCode::Char('E'));
        let request = app.take_edit().expect("a copy for the editor");
        assert!(request.copy.starts_with(home.join("copies")));
        std::fs::write(&request.copy, "font-size = 16\n").unwrap();
        let copy = request.copy.clone();
        app.edited(request, Ok(()));
        assert!(!copy.exists(), "the copy is removed");
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        press(&mut app, KeyCode::Char('n'));
        // An edit of Fastfetch that does not check is refused.
        let fastfetch = app
            .config_rows
            .iter()
            .position(|row| matches!(row, ConfigRow::Edit(configs::Kind::Fastfetch)))
            .unwrap();
        app.main_state.select(Some(fastfetch));
        press(&mut app, KeyCode::Enter);
        let request = app.take_edit().unwrap();
        std::fs::write(&request.copy, "{ \"modules\": [ { } ] }").unwrap();
        app.edited(request, Ok(()));
        let shown = screen(&mut app, 160, 50);
        assert!(shown.contains("Not saved: module 1"), "{shown}");
        press(&mut app, KeyCode::Esc);
        // A backup is put back after review.
        let backup = app
            .config_rows
            .iter()
            .position(|row| matches!(row, ConfigRow::Backup(configs::Kind::Ghostty, 0)))
            .unwrap();
        app.main_state.select(Some(backup));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        press(&mut app, KeyCode::Char('n'));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "font-size = 11\n");
    }

    #[test]
    fn the_migration_screen_says_what_a_migration_means_here() {
        let directory = tempfile::tempdir().unwrap();
        let path = fedora_migration(directory.path());
        let mut app = app();
        app.migration_here = Some(migration::Migration {
            version: migration::VERSION,
            from: "Garuda Linux".into(),
            entries: vec!["git".into()],
            flatpaks: vec!["org.kde.haruna".into()],
            packages: vec!["htop".into(), "nano".into()],
            ..Default::default()
        });
        app.open_screen(Screen::Migration);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "Migration",
            "This machine",
            "1 entries, 1 Flatpaks and 2 packages of yours, from Garuda Linux",
            "e Export",
            "o Open a file",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        press(&mut app, KeyCode::Char('o'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::OpenMigration))
        ));
        type_text(&mut app, &path.display().to_string());
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none());
        let shown = screen(&mut app, 140, 40);
        for text in [
            "From Fedora Linux 43 (x86_64), made 1970-01-01 00:00 UTC",
            "To install (2)",
            "Git",
            "com.spotify.Client",
            "Flathub",
            "Cannot be installed here (1)",
            "no-such-entry",
            "not in the catalog here",
            "Left to you (1)",
            "python3-requests",
            "named as on Fedora Linux 43; look for what Garuda Linux calls it",
            "a Install",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // d tells of the selected one.
        let left = app
            .migration_rows
            .iter()
            .position(|row| {
                matches!(row, MigrationRow::Item(index)
                    if app.migration_opened.as_ref().unwrap().1.items[*index].id == "python3-requests")
            })
            .unwrap();
        app.main_state.select(Some(left));
        press(&mut app, KeyCode::Char('d'));
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("It is left to you"), "{shown}");
        press(&mut app, KeyCode::Esc);
        // A file that is not a migration says why.
        std::fs::write(directory.path().join("bad.toml"), "packages = 1\n").unwrap();
        press(&mut app, KeyCode::Char('o'));
        for _ in 0..path.display().to_string().len() {
            press(&mut app, KeyCode::Backspace);
        }
        type_text(
            &mut app,
            &directory.path().join("bad.toml").display().to_string(),
        );
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("Cannot open the migration"), "{shown}");
        // The one opened before stays.
        assert!(app.migration_opened.is_some());
    }

    #[test]
    fn the_migration_screen_installs_what_it_can_after_review() {
        let directory = tempfile::tempdir().unwrap();
        let path = fedora_migration(directory.path());
        let mut app = app();
        app.open_screen(Screen::Migration);
        app.focus = Focus::Main;
        // Nothing open yet: a says how to open one.
        press(&mut app, KeyCode::Char('a'));
        assert!(screen(&mut app, 140, 40).contains("No migration open"));
        press(&mut app, KeyCode::Esc);
        app.open_migration(path);
        press(&mut app, KeyCode::Char('a'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 140, 40);
        for text in ["sudo pacman -S --needed git", "2. com.spotify.Client"] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        assert!(!shown.contains("python3-requests"), "{shown}");
        // Declined, nothing runs.
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
        // Enter on a row to install plans the same.
        let install = app
            .migration_rows
            .iter()
            .position(|row| matches!(row, MigrationRow::Item(0)))
            .unwrap();
        app.main_state.select(Some(install));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        press(&mut app, KeyCode::Char('n'));
        // e writes this machine's migration where it is told.
        app.migration_here = Some(migration::Migration {
            version: migration::VERSION,
            from: "Garuda Linux".into(),
            packages: vec!["htop".into()],
            ..Default::default()
        });
        press(&mut app, KeyCode::Char('e'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::ExportMigration))
        ));
        let target = directory.path().join("out.toml");
        app.answered(Asked::ExportMigration, &target.display().to_string());
        let written = migration::read(&target).unwrap();
        assert_eq!(written.packages, ["htop"]);
        assert!(screen(&mut app, 140, 40).contains("Migration exported"));
    }

    /// A repository of the test's own under `folder/projects`: one commit
    /// and a new file.
    fn sample_repository(folder: &Path) -> PathBuf {
        let repo = folder.join("projects/sample");
        std::fs::create_dir_all(&repo).unwrap();
        let git = |args: &[&str]| {
            let output = std::process::Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "Test")
                .env("GIT_AUTHOR_EMAIL", "test@example.com")
                .env("GIT_COMMITTER_NAME", "Test")
                .env("GIT_COMMITTER_EMAIL", "test@example.com")
                .output()
                .unwrap();
            assert!(output.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        std::fs::write(repo.join("one.txt"), "one\n").unwrap();
        git(&["add", "one.txt"]);
        git(&["commit", "-q", "-m", "First"]);
        std::fs::write(repo.join("new.txt"), "new\n").unwrap();
        repo.canonicalize().unwrap()
    }

    fn repository_app(folder: &Path) -> App {
        let mut app = app();
        app.repo_places = Some(repos::Places {
            roots: vec![folder.join("projects")],
            depth: 3,
            origin: "settings".into(),
        });
        app.workspaces_file = Some(folder.join("workspaces.toml"));
        app
    }

    fn select_repo_row(app: &mut App, wanted: impl Fn(&RepoRow) -> bool) {
        let index = app.repo_rows.iter().position(wanted).unwrap();
        app.main_state.select(Some(index));
    }

    #[test]
    fn the_repositories_screen_lists_them_and_opens_one() {
        let folder = tempfile::tempdir().unwrap();
        sample_repository(folder.path());
        let mut app = repository_app(folder.path());
        app.open_screen(Screen::Repositories);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 160, 40);
        for text in [
            "1 repositories",
            "sample",
            "main",
            "1 changed",
            "no upstream",
            "First",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        select_repo_row(&mut app, |row| matches!(row, RepoRow::Repository(0)));
        press(&mut app, KeyCode::Enter);
        let shown = screen(&mut app, 160, 40);
        for text in [
            "sample on main: no upstream",
            "Changes (1, 0 staged)",
            "new.txt",
            "Branches",
            "Tasks",
            "n keeps a command of yours",
            "Recent commits",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // Enter on a file shows its diff.
        select_repo_row(&mut app, |row| matches!(row, RepoRow::File(0)));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 160, 40).contains("+new"));
        press(&mut app, KeyCode::Esc);
        // f keeps it as a favorite, marked in the list.
        press(&mut app, KeyCode::Char('f'));
        let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
        assert!(kept.contains("favorite = true"), "{kept}");
        press(&mut app, KeyCode::Char('h'));
        assert!(app.repo_open.is_none());
        assert!(screen(&mut app, 160, 40).contains("★ sample"));
    }

    #[test]
    fn changes_to_a_repository_are_reviewed_before_anything_runs() {
        let folder = tempfile::tempdir().unwrap();
        let repo = sample_repository(folder.path());
        let mut app = repository_app(folder.path());
        app.open_screen(Screen::Repositories);
        app.focus = Focus::Main;
        app.open_repo(repo.clone());
        select_repo_row(&mut app, |row| matches!(row, RepoRow::File(0)));
        press(&mut app, KeyCode::Char(' '));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 160, 40);
        for text in ["Stage 1 file in sample", "add --all -- new.txt"] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // Declined: nothing is staged.
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
        let staged = std::process::Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["diff", "--cached", "--name-only"])
            .output()
            .unwrap();
        assert!(staged.stdout.is_empty());
        // A commit with nothing staged says why.
        press(&mut app, KeyCode::Char('c'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::CommitMessage))
        ));
        app.answered(Asked::CommitMessage, "Second");
        assert!(screen(&mut app, 160, 40).contains("nothing is staged"));
        press(&mut app, KeyCode::Esc);
        // A command of the user's becomes a task, reviewed before it runs.
        press(&mut app, KeyCode::Char('n'));
        assert!(matches!(app.popup, Some(Popup::Ask(_, Asked::AddTask))));
        app.answered(Asked::AddTask, "sh -c 'echo hi'");
        let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
        assert!(kept.contains("id = \"sh-c-echo-hi\""), "{kept}");
        press(&mut app, KeyCode::Esc);
        select_repo_row(&mut app, |row| matches!(row, RepoRow::Task(0)));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 160, 40).contains("sh -c 'echo hi'"));
        press(&mut app, KeyCode::Char('n'));
        // x takes the task away again.
        press(&mut app, KeyCode::Char('x'));
        let kept = std::fs::read_to_string(folder.path().join("workspaces.toml")).unwrap();
        assert!(!kept.contains("sh-c-echo-hi"), "{kept}");
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
    fn settings_turn_the_update_notifications_on_and_off() {
        let directory = tempfile::tempdir().unwrap();
        let mut app = app();
        app.open_screen(Screen::Settings);
        app.focus = Focus::Main;
        app.main_state.select(Some(6));
        assert!(screen(&mut app, 120, 40).contains("not available here"));
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none(), "nothing to turn on without a home");
        let places = notifications::Places {
            units: directory.path().join("systemd/user"),
            told: directory.path().join("notified.txt"),
        };
        app.notices = Some(places.clone());
        app.settings_path = Some(directory.path().join("settings.toml"));
        app.systemctl = |_| Some(true);
        press(&mut app, KeyCode::Enter);
        assert!(notifications::is_on(&places));
        assert!(screen(&mut app, 120, 40).contains("Helix Toolbox now checks"));
        press(&mut app, KeyCode::Esc);
        assert!(screen(&mut app, 120, 40).contains("yes, every 6 hours"));
        // Health problems too, then updates off: the timer stays for them.
        app.main_state.select(Some(7));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("Helix Doctor finds a new problem"));
        press(&mut app, KeyCode::Esc);
        app.main_state.select(Some(6));
        press(&mut app, KeyCode::Enter);
        assert!(app.popup.is_none());
        assert!(notifications::is_on(&places));
        let watched = watch::read(&places, app.settings_path.as_deref());
        assert!(watched.health && !watched.updates);
        // Drift: no baseline saved yet, so nothing to choose.
        let baselines = tempfile::tempdir().unwrap();
        app.baselines_dir = Some(baselines.path().to_path_buf());
        assert!(screen(&mut app, 120, 40).contains("no (save one in Drift)"));
        std::fs::write(
            baselines.path().join("fresh.toml"),
            "name = \"Fresh\"\nsystem = \"Garuda Linux\"\ncreated = 1\n",
        )
        .unwrap();
        app.main_state.select(Some(8));
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Esc);
        assert!(screen(&mut app, 120, 40).contains("from fresh"));
        press(&mut app, KeyCode::Enter);
        assert!(
            watch::read(&places, app.settings_path.as_deref())
                .drift
                .is_none()
        );
        // Everything off removes the timer.
        app.main_state.select(Some(7));
        press(&mut app, KeyCode::Enter);
        assert!(!notifications::is_on(&places));
        assert!(app.popup.is_none());
        // Without systemd it says so, and nothing is written.
        app.systemctl = |_| None;
        app.main_state.select(Some(6));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 120, 40).contains("could not be turned on"));
        assert!(!places.units.join(notifications::TIMER).exists());
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
            Some(helix_toolbox_core::PackageManager::Arch),
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

    fn queue_ids(app: &App) -> Vec<String> {
        app.queue.iter().map(|entry| entry.id.clone()).collect()
    }

    #[test]
    fn the_sidebar_counts_what_is_pinned_queued_and_waiting() {
        let mut app = app();
        for id in ["git", "ripgrep"] {
            let entry = app.catalog.find(id).unwrap().clone();
            app.toggle_queued(entry);
        }
        app.favorites.push("fzf".to_string());
        app.statuses.insert(
            "fzf".to_string(),
            Status::Update {
                source: "pacman".to_string(),
                version: Some("0.60".to_string()),
                latest: Some("0.61".to_string()),
                shadowed: None,
            },
        );
        let text = screen(&mut app, 120, 40);
        let line = |label: &str| {
            text.lines()
                .find(|line| line.contains(label))
                .unwrap_or_default()
                .to_string()
        };
        let sidebar = |line: String| {
            line.chars()
                .take(SIDEBAR_WIDTH as usize)
                .collect::<String>()
        };
        assert!(sidebar(line("Install Plan")).contains(" 2 │"), "{text}");
        assert!(sidebar(line("Favorites")).contains(" 1"), "{text}");
        assert!(sidebar(line("Updates")).contains(" 1"), "{text}");
        // Nothing to count, nothing shown.
        assert!(
            !sidebar(line("History")).chars().any(|c| c.is_ascii_digit()),
            "{text}"
        );
    }

    #[test]
    fn the_header_shows_the_version_where_it_fits() {
        let mut app = app();
        let wide = screen(&mut app, 120, 40);
        assert!(
            wide.contains(concat!("v", env!("CARGO_PKG_VERSION"))),
            "{wide}"
        );
        app.size_bypass = true;
        let narrow = screen(&mut app, 40, 30);
        assert!(
            narrow.contains("HELIX TOOLBOX") && narrow.contains("SYSTEM READY"),
            "{narrow}"
        );
    }

    #[test]
    fn the_footer_says_how_the_check_stands_and_ctrl_r_checks_again() {
        let mut app = app();
        assert!(!screen(&mut app, 120, 40).contains("package entries checked"));
        app.statuses.insert(
            "fzf".to_string(),
            Status::Absent {
                replaces: Vec::new(),
            },
        );
        let text = screen(&mut app, 120, 40);
        assert!(
            text.contains("1 package entries checked · Ctrl-R check again"),
            "{text}"
        );
        let (_sender, receiver) = mpsc::channel();
        app.checking = Some(receiver);
        assert!(screen(&mut app, 120, 40).contains("Checking what is installed…"));
        // Off in tests, Ctrl-R starts nothing, and the palette offers it too.
        app.checking = None;
        app.handle_key(&KeyEvent::new(KeyCode::Char('r'), KeyModifiers::CONTROL));
        assert!(app.checking.is_none() && app.popup.is_none());
        let (choices, _) = app.palette_choices();
        assert!(
            choices
                .iter()
                .any(|choice| choice.label == "Check again what is installed")
        );
    }

    #[test]
    fn area_rows_say_what_entries_and_groups_hold_and_results_where_they_are() {
        let mut app = app();
        app.open_screen(Screen::Area(app.first_area()));
        app.focus = Focus::Main;
        let text = screen(&mut app, 160, 40);
        let group = app
            .rows
            .iter()
            .find_map(|row| match row {
                Row::Group { name, count, .. } => Some((name.clone(), *count)),
                _ => None,
            })
            .unwrap();
        let line = text.lines().find(|line| line.contains(&group.0)).unwrap();
        assert!(
            line.trim_end()
                .trim_end_matches('│')
                .trim_end()
                .ends_with(&group.1.to_string()),
            "{line}"
        );
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        let text = screen(&mut app, 160, 40);
        let row = text.lines().find(|line| line.contains("Ripgrep")).unwrap();
        assert!(row.contains("Applications › "), "{row}");
        // A row that cannot be queued is marked as running on its own.
        let alone = app
            .catalog
            .entries()
            .into_iter()
            .find(|entry| !entry.multi_select)
            .unwrap()
            .clone();
        press(&mut app, KeyCode::Esc);
        app.rows = vec![Row::Entry(Box::new(alone.clone()))];
        let text = screen(&mut app, 160, 40);
        assert!(text.contains(&format!("[>]  {}", alone.name)), "{text}");
    }

    #[test]
    fn details_say_where_an_entry_is_its_kind_and_its_flags() {
        let mut app = app();
        app.open_screen(Screen::Area(app.first_area()));
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('/'));
        type_text(&mut app, "ripgrep");
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('d'));
        let text = screen(&mut app, 140, 45);
        assert!(text.contains("Applications › "), "{text}");
        assert!(text.contains("Package"), "{text}");
        assert!(text.contains("What it does:"), "{text}");
        assert!(text.contains("installation (privileged)"), "{text}");
        assert_eq!(
            flag_meanings("PI FM X"),
            vec![
                (
                    "PI".to_string(),
                    "installation (privileged), privileged".to_string()
                ),
                ("FM".to_string(), "file changes".to_string()),
                ("X".to_string(), "unknown flag".to_string()),
            ]
        );
    }

    #[test]
    fn a_profile_shows_what_it_runs_and_e_queues_its_entries() {
        let mut app = app();
        let profile = app.catalog.find("developer-basics").unwrap().clone();
        let Action::Profile { entries } = &profile.action else {
            panic!("not a profile");
        };
        let entries = entries.clone();
        app.rows = vec![Row::Entry(Box::new(profile.clone()))];
        app.screen = Screen::Area(app.first_area());
        app.focus = Focus::Main;
        app.main_state.select(Some(0));
        press(&mut app, KeyCode::Char('d'));
        let text = screen(&mut app, 140, 45);
        assert!(text.contains("Runs, in order:"), "{text}");
        assert!(text.contains("1. Git") || text.contains("1. git"), "{text}");
        app.popup = None;
        // One entry already queued stays once.
        let git = app.catalog.find("git").unwrap().clone();
        app.queue.push(git);
        app.rows = vec![Row::Entry(Box::new(profile))];
        app.main_state.select(Some(0));
        press(&mut app, KeyCode::Char('e'));
        assert_eq!(app.screen, Screen::Plan);
        assert_eq!(queue_ids(&app), entries);
        assert!(screen(&mut app, 140, 40).contains("3 of its entries were added"));
        app.popup = None;
        app.open_screen(Screen::Area(app.first_area()));
        app.rows = vec![Row::Entry(Box::new(
            app.catalog.find("developer-basics").unwrap().clone(),
        ))];
        app.main_state.select(Some(0));
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('e'));
        assert!(screen(&mut app, 140, 40).contains("Its entries are already in the Install Plan"));
    }

    #[test]
    fn the_install_plan_moves_entries_and_says_what_each_is() {
        let mut app = app();
        for id in ["git", "ripgrep", "fzf"] {
            let entry = app.catalog.find(id).unwrap().clone();
            app.toggle_queued(entry);
        }
        app.open_screen(Screen::Plan);
        app.focus = Focus::Main;
        app.main_state.select(Some(0));
        press(&mut app, KeyCode::Char('J'));
        assert_eq!(queue_ids(&app), ["ripgrep", "git", "fzf"]);
        assert_eq!(app.main_state.selected(), Some(1));
        app.handle_key(&KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
        assert_eq!(queue_ids(&app), ["ripgrep", "fzf", "git"]);
        press(&mut app, KeyCode::Char('K'));
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(queue_ids(&app), ["git", "ripgrep", "fzf"]);
        // At the top, it stays.
        press(&mut app, KeyCode::Char('K'));
        assert_eq!(queue_ids(&app), ["git", "ripgrep", "fzf"]);
        app.statuses.insert(
            "fzf".to_string(),
            Status::Absent {
                replaces: Vec::new(),
            },
        );
        let text = screen(&mut app, 140, 50);
        assert!(text.contains("Queued entries · 3"), "{text}");
        assert!(text.contains("1. Git") || text.contains("1. git"), "{text}");
        assert!(text.contains("Applications › "), "{text}");
        assert!(text.contains(" · Package"), "{text}");
        assert!(text.contains("Not installed"), "{text}");
        assert!(text.contains("K/J Move"), "{text}");
    }

    #[test]
    fn screens_are_found_by_name_for_the_command_line() {
        let app = app();
        assert_eq!(app.screen_named("updates"), Some(Screen::Updates));
        assert_eq!(app.screen_named("Home-Manager"), Some(Screen::HomeManager));
        assert_eq!(
            app.screen_named("applications"),
            Some(Screen::Area(app.first_area()))
        );
        assert_eq!(app.screen_named("nowhere"), None);
        assert!(app.screen_names().contains("doctor, updates, drift"));
        assert!(
            app.screen_names().ends_with("profiles") || app.screen_names().contains("applications")
        );
        let mut app = app;
        app.open_on(Screen::Doctor);
        assert_eq!((app.screen, app.focus), (Screen::Doctor, Focus::Main));
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
            .join("app.conf.helix-toolbox-backup-20261016120000");
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
        use helix_toolbox_engine::hardware::{Battery, Disk, Firmware, FirmwareUpdate, Hardware};
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
        assert!(text.contains("Welcome to Helix Toolbox"), "{text}");
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
        use helix_toolbox_engine::desktop::{Desktop, Kind, Value};
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

    /// Home Manager as the tests' screens see it: a flake, two generations
    /// and a configuration file it links.
    fn home_status() -> home_manager::Status {
        home_manager::Status {
            installed: true,
            version: Some("26.11-pre".into()),
            config: Some(home_manager::Config {
                folder: "/home/me/dotfiles/home-manager".into(),
                flake: true,
                profile: Some("me".into()),
                origin: home_manager::Origin::Settings,
            }),
            problem: None,
            generations: vec![
                home_manager::Generation {
                    id: 201,
                    built: "2026-10-20 12:01".into(),
                    path: "/nix/store/aaa-home-manager-generation".into(),
                    current: true,
                },
                home_manager::Generation {
                    id: 200,
                    built: "2026-10-16 13:23".into(),
                    path: "/nix/store/no-such-home-manager-generation".into(),
                    current: false,
                },
            ],
            repository: Some(Summary {
                name: "dotfiles".into(),
                path: "/home/me/dotfiles".into(),
                branch: Some("main".into()),
                changed: 2,
                ahead: Some(1),
                behind: Some(0),
                ..Summary::default()
            }),
            links: vec![home_manager::Link {
                name: "Ghostty".into(),
                path: "/home/me/.config/ghostty/config".into(),
                target: Some("/nix/store/xyz-home-manager-files/.config/ghostty/config".into()),
                managed: true,
            }],
        }
    }

    #[test]
    fn the_home_manager_screen_shows_generations_its_repository_and_links() {
        let mut app = app();
        app.home_status = Some(home_status());
        app.open_screen(Screen::HomeManager);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 140, 40);
        for text in [
            "Home Manager 26.11-pre",
            "Configuration: /home/me/dotfiles/home-manager#me (a flake, from Helix Toolbox's settings)",
            "Repository /home/me/dotfiles: main, 2 changed, 1 ahead and 0 behind",
            "Generations (2)",
            "201  2026-10-20 12:01  active",
            "Ghostty",
            "linked by Home Manager",
            "b Build",
            "p Pull",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
        // d tells of the linked file.
        let link = app
            .home_rows
            .iter()
            .position(|row| matches!(row, HomeRow::Link(0)))
            .unwrap();
        app.main_state.select(Some(link));
        press(&mut app, KeyCode::Char('d'));
        assert!(screen(&mut app, 140, 40).contains("Home Manager manages it"));
        // Not installed: building says so instead of planning.
        press(&mut app, KeyCode::Esc);
        app.home_status = Some(home_manager::Status::default());
        app.build_home_rows();
        press(&mut app, KeyCode::Char('b'));
        assert!(screen(&mut app, 140, 40).contains("home-manager is not installed here"));
    }

    #[test]
    fn the_home_manager_screen_reviews_build_switch_and_going_back() {
        let mut app = app();
        app.home_status = Some(home_status());
        app.open_screen(Screen::HomeManager);
        app.focus = Focus::Main;
        press(&mut app, KeyCode::Char('b'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("home-manager build --flake '/home/me/dotfiles/home-manager#me'"),
            "{shown}"
        );
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
        press(&mut app, KeyCode::Char('s'));
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 160, 40).contains("home-manager switch --flake"));
        press(&mut app, KeyCode::Char('n'));
        // The earlier generation was collected: there is no script to run.
        let earlier = app
            .home_rows
            .iter()
            .position(|row| matches!(row, HomeRow::Generation(1)))
            .unwrap();
        app.main_state.select(Some(earlier));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 160, 40).contains("has no activate script"));
        // Enter on the active one only tells of it.
        press(&mut app, KeyCode::Esc);
        app.main_state.select(Some(earlier - 1));
        press(&mut app, KeyCode::Enter);
        assert!(screen(&mut app, 160, 40).contains("It is the active one."));
    }

    #[test]
    fn the_doctor_counts_its_checks_shows_repair_commands_and_its_reports_are_entries() {
        let mut app = app();
        let check = |id: &str, health: doctor::Health| doctor::Diagnosis {
            id: id.into(),
            title: id.into(),
            health,
            summary: "found".into(),
            evidence: Vec::new(),
            advice: None,
            repair: None,
            entry: None,
        };
        app.diagnoses = vec![
            check("one", doctor::Health::Problem),
            check("two", doctor::Health::Notice),
            check("three", doctor::Health::Good),
            check("four", doctor::Health::Good),
        ];
        app.diagnoses[0].repair = Some(doctor::Repair {
            label: "fix it".into(),
            commands: vec![vec!["sudo".into(), "true".into()]],
        });
        app.open_screen(Screen::Doctor);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("Checks · 1 problem · 1 notice · 2 OK"),
            "{shown}"
        );
        // The repair's commands are shown before f runs them.
        press(&mut app, KeyCode::Char('d'));
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("Repair: fix it.") && shown.contains("sudo true"),
            "{shown}"
        );
        app.popup = None;
        // A report row queues, pins and describes like any entry.
        app.main_state.select(Some(app.diagnoses.len()));
        let report = app.selected_entry().cloned().expect("a report entry");
        press(&mut app, KeyCode::Char('d'));
        assert!(matches!(app.popup, Some(Popup::Text(_))));
        app.popup = None;
        press(&mut app, KeyCode::Char('*'));
        assert!(app.favorites.contains(&report.id));
        if report.multi_select {
            press(&mut app, KeyCode::Char(' '));
            assert!(app.is_queued(&report));
            assert!(screen(&mut app, 160, 40).contains(&format!("{} *", report.name)));
        }
    }

    #[test]
    fn the_updates_screen_lists_entries_with_an_update_below_the_channels() {
        let mut app = app();
        app.channels = vec![channel("pacman", "Arch packages", Some(Vec::new()))];
        let inventory = Inventory::from_parts(
            Some(helix_toolbox_core::PackageManager::Arch),
            Some([("ripgrep".to_string(), "15.2.0-1".to_string())].into()),
            [("ripgrep".to_string(), "15.3.0-1".to_string())].into(),
            HashMap::new(),
            Default::default(),
        );
        let ripgrep = app.catalog.find("ripgrep").unwrap().clone();
        let status = inventory
            .status(&ripgrep, &app.platform, &|_| None)
            .unwrap();
        app.statuses.insert("ripgrep".into(), status);
        app.inventory = Some(inventory);
        app.open_screen(Screen::Updates);
        app.focus = Focus::Main;
        assert_eq!(app.main_len(), 3, "the channel, the entry and update all");
        let shown = screen(&mut app, 140, 40);
        assert!(shown.contains("Entry   Ripgrep"), "{shown}");
        assert!(shown.contains("Update every entry (1)"), "{shown}");
        app.main_state.select(Some(1));
        assert!(screen(&mut app, 140, 40).contains("Update it"));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
        assert!(screen(&mut app, 140, 40).contains("sudo pacman -Syu"));
        press(&mut app, KeyCode::Char('n'));
        app.popup = None;
        app.main_state.select(Some(2));
        press(&mut app, KeyCode::Enter);
        assert!(matches!(app.popup, Some(Popup::Confirm(_))));
    }

    #[test]
    fn drift_shows_was_and_now_what_is_not_compared_and_runs_the_missing_after_review() {
        let mut app = app();
        // Opening the screen reads the baselines; the comparison is set after.
        app.open_screen(Screen::Drift);
        app.focus = Focus::Main;
        app.drift_targets = vec![DriftTarget::Profile {
            id: "profile:work".into(),
            name: "Work".into(),
            entries: vec!["ripgrep".into(), "gone-entry".into()],
        }];
        app.drift_target = 0;
        app.drift_report = Some((
            "profile:work".into(),
            drift::Report {
                against: "Work".into(),
                differences: vec![drift::Difference {
                    kind: drift::Kind::Missing,
                    subject: "ripgrep".into(),
                    was: Some("15.2.0".into()),
                    now: None,
                    entry: Some("ripgrep".into()),
                    note: "not installed".into(),
                }],
                matched: 3,
                unknown: vec!["gone-entry".into()],
            },
        ));
        app.main_state.select(Some(0));
        let shown = screen(&mut app, 160, 40);
        assert!(shown.contains("15.2.0 → –"), "{shown}");
        assert!(shown.contains("Not compared: gone-entry"), "{shown}");
        assert!(shown.contains("R Run them now"), "{shown}");
        // Reviewed like any run; where it is installed already, there is
        // nothing to run.
        press(&mut app, KeyCode::Char('R'));
        assert!(
            matches!(app.popup, Some(Popup::Confirm(_)))
                || screen(&mut app, 160, 40).contains("Nothing to run"),
            "{}",
            screen(&mut app, 160, 40)
        );
        press(&mut app, KeyCode::Char('n'));
        assert!(!matches!(app.popup, Some(Popup::Running(_))));
    }

    #[test]
    fn reliability_says_what_of_the_record_could_not_be_read() {
        let mut app = app();
        let mut evidence = reliability::Evidence::default();
        evidence
            .problems
            .push("validation.toml cannot be read: broken".into());
        app.evidence = Some(evidence);
        app.open_screen(Screen::Reliability);
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("⚠ validation.toml cannot be read: broken"),
            "{shown}"
        );
    }

    #[test]
    fn a_login_application_is_described_before_it_is_switched() {
        let mut app = app();
        let mut found = stand_in_services();
        found.autostart[0].comment = "Updates in the background".into();
        found.autostart[0].own = Some("/home/me/.config/autostart/discover.desktop".into());
        app.services_found = Some(found);
        app.open_screen(Screen::Services);
        app.focus = Focus::Main;
        // The view of the applications started at login.
        for _ in 0..3 {
            press(&mut app, KeyCode::Char('i'));
        }
        let row = app
            .service_rows
            .iter()
            .position(|row| matches!(row, ServiceRow::Autostart(_)))
            .unwrap();
        app.main_state.select(Some(row));
        press(&mut app, KeyCode::Char('d'));
        let shown = screen(&mut app, 160, 40);
        for text in [
            "Updates in the background",
            "Runs: discover --notifier",
            "From: the system · your file overrides it",
            "At login: started",
            "For you only",
        ] {
            assert!(shown.contains(text), "{text}: {shown}");
        }
    }

    #[test]
    fn migration_says_when_packages_cannot_be_told_apart() {
        let mut app = app();
        app.migration_here = Some(migration::Migration {
            version: migration::VERSION,
            from: "Solus".into(),
            ..Default::default()
        });
        app.migration_packages_known = false;
        app.open_screen(Screen::Migration);
        let shown = screen(&mut app, 200, 40);
        assert!(
            shown.contains("The packages you installed yourself cannot be told apart here"),
            "{shown}"
        );
    }

    #[test]
    fn restic_restores_what_is_typed_shows_what_it_leaves_out_and_ignores_case() {
        let mut app = restic_app(Vec::new());
        if let Some(ResticReading {
            found: restic::Found::Ready(config),
            ..
        }) = &mut app.restic_reading
        {
            config.excludes = vec!["/home/me/.cache".into()];
        }
        app.open_screen(Screen::Restic);
        app.focus = Focus::Main;
        let shown = screen(&mut app, 160, 40);
        assert!(shown.contains("Leaves out /home/me/.cache"), "{shown}");
        press(&mut app, KeyCode::Char('F'));
        assert!(matches!(
            app.popup,
            Some(Popup::Ask(_, Asked::ResticRestoreFrom))
        ));
        type_text(&mut app, " /home/me/Documents");
        press(&mut app, KeyCode::Enter);
        assert!(
            matches!(
                &app.popup,
                Some(Popup::Ask(_, Asked::ResticRestore(snapshot, Some(include))))
                    if snapshot == "latest" && include == "/home/me/Documents"
            ),
            "the snapshot, then the path"
        );
        app.popup = None;
        press(&mut app, KeyCode::Char('s'));
        answer(&mut app, "NOTES.txt");
        press(&mut app, KeyCode::Char('i'));
        assert!(app.restic_ignore_case);
        assert!(screen(&mut app, 160, 40).contains("ignoring upper and lower case"));
    }

    #[test]
    fn a_task_of_yours_can_name_its_shell() {
        assert_eq!(
            task_shell(" fish: npm run dev "),
            (workspaces::Shell::Fish, "npm run dev".to_string())
        );
        assert_eq!(
            task_shell("bash:make"),
            (workspaces::Shell::Bash, "make".to_string())
        );
        assert_eq!(
            task_shell("sh -c 'echo hi'"),
            (workspaces::Shell::Direct, "sh -c 'echo hi'".to_string())
        );
        assert_eq!(
            task_shell("fish:"),
            (workspaces::Shell::Direct, "fish:".to_string()),
            "a shell with no command is the command"
        );
    }

    #[test]
    fn a_configuration_draft_is_checked_and_discarded() {
        let directory = tempfile::tempdir().unwrap();
        let home = directory.path().canonicalize().unwrap();
        let mut app = configs_app(&home);
        app.open_screen(Screen::Configs);
        app.focus = Focus::Main;
        app.set_config(configs::Kind::Ghostty, "font-size", "13");
        let shown = screen(&mut app, 160, 50);
        assert!(shown.contains("Draft checks: not checked yet"), "{shown}");
        let field = app
            .config_rows
            .iter()
            .position(|row| matches!(row, ConfigRow::Field(configs::Kind::Ghostty, 1)))
            .unwrap();
        app.main_state.select(Some(field));
        press(&mut app, KeyCode::Char('c'));
        assert!(
            !screen(&mut app, 160, 50).contains("Draft checks: not checked yet"),
            "it was checked"
        );
        press(&mut app, KeyCode::Char('x'));
        let shown = screen(&mut app, 160, 50);
        assert!(!shown.contains("Draft:"), "{shown}");
        assert_eq!(
            std::fs::read_to_string(home.join(".config/ghostty/config")).unwrap(),
            "font-size = 11\n"
        );
    }

    #[test]
    fn settings_say_where_they_are_kept() {
        let mut app = app();
        app.settings_path = Some("/home/me/.config/helix-toolbox/settings.toml".into());
        app.open_screen(Screen::Settings);
        let shown = screen(&mut app, 160, 40);
        assert!(
            shown.contains("Kept in /home/me/.config/helix-toolbox/settings.toml"),
            "{shown}"
        );
    }
}
