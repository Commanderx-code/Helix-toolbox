// What the commands in src-tauri/src/main.rs return; views.rs builds them,
// and the engine's own serializable types come through as they are.

export type Overview = { rows: [string, string][] };

export type About = {
  version: string;
  overview: Overview;
  command_line: boolean;
  /** The first start on this machine: the setup wizard opens by itself. */
  first_run: boolean;
  theme: WindowTheme;
  /** Started from a notification: the screen to open on. */
  opens: 'updates' | 'doctor' | 'drift' | null;
  /** What to call the person signed in: their full name, else their login. */
  person: string | null;
};

/** The window's colors: the desktop's preference, or always dark or light. */
export type WindowTheme = 'system' | 'dark' | 'light';

// The setup wizard

export type SetupView = {
  machine: [string, string][];
  workstations: { id: string; name: string; description: string }[];
};

export type PickView = {
  id: string;
  name: string;
  available: boolean;
  note: string | null;
  ticked: boolean;
};

export type EntryView = {
  id: string;
  name: string;
  description: string;
  flags: string;
  kind: 'package' | 'script' | 'command' | 'nix' | 'native' | 'profile';
  queueable: boolean;
  interactive: boolean;
  /** A package entry that offers removal. */
  removable: boolean;
  /** One of the user's own profiles, which can be exported and deleted. */
  yours: boolean;
  runs: string[];
};

export type ItemView =
  | ({ type: 'entry' } & EntryView)
  | { type: 'group'; id: string; name: string; description: string; items: ItemView[] };

export type AreaView = { id: string; name: string; items: ItemView[] };

export type StatusView = {
  kind: 'installed' | 'update' | 'managed' | 'absent';
  label: string;
  detail: string;
};

export type Line = { name: string; detail: string };

export type Prepared =
  | { kind: 'nothing' | 'blocked' | 'error'; lines: string[] }
  | { kind: 'ready'; header: string[]; lines: Line[]; risk: string; privileged: boolean };

/** What a run can leave out of date, for the window to read again. */
export type Reading = 'statuses' | 'updates' | 'tools' | 'history';

/** A closed run, as the engine's run session tells it. */
export type ClosedView = {
  /** The queued entries that stay queued: what did not finish. */
  remaining: string[];
  read_again: Reading[];
  /** Why the run was not recorded in the history, if it was not. */
  not_recorded: string | null;
};

/** What a plan is expected to change, as the terminal interface's change preview shows it. */
export type ChangesView = { steps: StepChanges[]; download: boolean };

export type StepChanges = { name: string; changes: ChangeView[] };

export type Certainty = 'known' | 'expected' | 'unknown';

export type ChangeView = { certainty: Certainty; text: string; diff: DiffView | null };

/** A file now and after; a null row marks unchanged rows left out. */
export type DiffView = { path: string; changed: boolean; rows: (DiffRow | null)[] };

export type DiffRow = { before: string | null; after: string | null; tag: 'same' | 'changed' | 'removed' | 'added' };

export type StepInfo = { index: number; id: string; name: string };

export type StepState =
  | { state: 'done' | 'interrupted' | 'skipped' | 'not_run' }
  | { state: 'failed'; exit_code: number };

export type Finished = {
  outcome:
    | { kind: 'success' }
    | { kind: 'failed'; exit_code: number }
    | { kind: 'interrupted' }
    | { kind: 'could_not_start'; reason: string };
  description: string;
  succeeded: boolean;
  verified: boolean;
  steps: (StepInfo & StepState)[];
  checks: { name: string; passed: boolean; detail: string; unsure: boolean; later: boolean }[];
};

/** An event of a run: ExecutionEvent's JSON form, output as base64. */
export type RunEvent =
  | { event: 'run_started'; entries: { id: string; name: string }[]; steps: StepInfo[] }
  | { event: 'step_started' | 'step_completed' | 'step_skipped'; step: StepInfo }
  | { event: 'step_failed'; step: StepInfo; exit_code: number; interrupted: boolean }
  | { event: 'output'; offset: number; data: string }
  | { event: 'progress'; ended: number; total: number }
  | { event: 'warning'; message: string }
  | ({ event: 'completed' | 'interrupted' } & Finished);

/** A report that a run worked on this machine, and the issue it fills in. */
export type HardwareReport = {
  text: string;
  url: string;
};

export type RecordView = {
  date: string;
  unix_timestamp: number;
  names: string[];
  success: boolean;
  result: string;
  log: string | null;
};

// Doctor

export type Diagnosis = {
  id: string;
  title: string;
  health: 'good' | 'notice' | 'problem';
  health_label: string;
  summary: string;
  evidence: string[];
  advice: string | null;
  repair: { label: string; commands: string[] } | null;
  entry: string | null;
};

// Updates

export type Pending = { name: string; from: string | null; to: string | null };

export type ChannelView = {
  id: string;
  name: string;
  count: number | null;
  summary: string;
  pending: Pending[] | null;
  note: string;
  command_line: string;
  caveat: string | null;
};

export type UpdatesView = { online: boolean; channels: ChannelView[] };

// Backups

export type BackupView = {
  index: number;
  original: string;
  backup: string;
  taken: string | null;
  system: boolean;
  directory: boolean;
};

export type BackupsView = {
  snapshots: { tool: string; restore: string } | null;
  backups: BackupView[];
};

// Drift

export type TargetView = { key: string; kind: 'baseline' | 'profile'; name: string; detail: string };

export type Difference = {
  kind: string;
  subject: string;
  was: string | null;
  now: string | null;
  entry: string | null;
  note: string;
};

export type DriftView = {
  report: { against: string; differences: Difference[]; matched: number; unknown: string[] };
  restore: string[];
  unavailable: string[];
};

// Reliability

export type Assessment = {
  level: string;
  systems: string[];
  verified: string | null;
  evidence: string[];
  here: number | null;
  critical: boolean;
};

export type ReliabilityView = {
  summary: string;
  problems: string[];
  rows: { id: string; name: string; level: string; explain: string; assessment: Assessment }[];
};

// Display rows: the engine's text for a screen, laid out by the screen.

/** A row as both interfaces show it: the label over the detail, the name beside them. */
export type DisplayRow = { name: string; label: string; detail: string; attention: boolean };

/** How full something is, for a bar. */
export type Meter = { percent: number; full: boolean };

// Hardware

export type HardwareView = {
  /** Why fwupd could not be read, and what it lists. */
  firmware: string[];
  /** By their place in the list, which prepare_firmware takes. */
  updates: DisplayRow[];
  disks: DisplayRow[];
  sensors: DisplayRow[];
  batteries: DisplayRow[];
  pci: Device[];
  usb: Device[];
};

/** Empty where it is not known. */
export type Device = { address: string; name: string; class: string; driver: string };

// Desktop

export type DesktopView = {
  desktop: {
    kind: string | null;
    name: string | null;
    session: string | null;
    display_manager: { name: string; files: string[]; theme: string | null } | null;
    appearance: { key: string; label: string; value: string | null }[];
    owners: string[];
    blocked: string | null;
  };
  looks: { slug: string; name: string; desktop: string; settings: number; other: boolean }[];
  suggestions: { id: string; name: string; available: boolean; note: string | null }[];
};

// Settings

export type SettingsView = {
  keep_going: boolean;
  snapshots: boolean;
  snapshot_tool: string | null;
  path: string | null;
  theme: WindowTheme;
  /** Whether update notifications are on; null with no home folder for their timer. */
  notifications: boolean | null;
  /** Whether new health problems are told of; null as above. */
  health: boolean | null;
  /** The baseline Drift is watched against, by its file name. */
  drift: string | null;
  /** The saved baselines: file name and name. */
  baselines: [string, string][];
  /** How often the timed restic backup runs, or null when it is off. */
  restic_timer: 'day' | 'week' | null;
  /** Whether a restic repository is set up, so the timer can be. */
  restic_ready: boolean;
};

// Services and startup

export type UnitView = {
  name: string;
  description: string;
  user: boolean;
  /** service, timer or socket. */
  kind: string;
  /** active, inactive, failed or "not loaded". */
  state: string;
  /** Its enablement and description, for the list. */
  detail: string;
  enablement: string;
  /** How it is now: "active (running)". */
  now: string;
  /** Why enable and disable do not apply, when they do not. */
  note: string | null;
  running: boolean;
  failed: boolean;
  enabled: boolean;
  switchable: boolean;
};

export type AutostartView = {
  file: string;
  name: string;
  comment: string;
  exec: string;
  enabled: boolean;
  here: boolean;
  /** on, off or "not here". */
  state: string;
  runs: string;
  from: string;
  at_login: string;
};

export type ServicesView = {
  systemd: boolean;
  units: UnitView[];
  autostart: AutostartView[];
};

export type ServiceDetails = { needed_by: string[]; consequence: string | null };

// Storage

export type StorageItem = {
  id: string;
  title: string;
  detail: string;
  packages: string[];
  /** What it takes now, "?" when not known. */
  size: string;
  /** What cleaning it frees, or why it is not offered. */
  said: string;
  offered: boolean;
};

export type StorageView = {
  filesystems: { path: string; text: string; meter: Meter }[];
  items: StorageItem[];
  kept: { title: string; detail: string; size: string | null }[];
};

// Performance

/** A recorded boot: when, and how long it took in all. */
export type BootSummary = { id: string; booted: number; total: string };

export type BootView = {
  id: string;
  /** Seconds since 1970. */
  booted: number;
  kernel: string;
  total: string;
  /** took in milliseconds, for its share of the bar. */
  parts: { name: string; took: number; shown: string; tone: string }[];
  /** What the login screen waited for. */
  waited: { name: string; took: string }[];
  slowest: { name: string; took: string }[];
};

export type Comparison = {
  before: BootSummary;
  /** The whole boot's change: "+5.0 s". */
  total: string;
  /** Positive when the later boot was slower, negative when faster. */
  sign: number;
  changes: { name: string; now: string; how: string; slower: boolean }[];
  /** The kernels before and after, when they differ. */
  kernel: [string, string] | null;
  runs: { at: number; ran: string }[];
};

export type PerformanceHint = { title: string; detail: string; command: string | null; unit: string | null; entry: string | null };

export type Fact = { term: string; text: string; note: string; meter: Meter | null };

export type PerformanceView = {
  boot: BootView | null;
  no_boot: string | null;
  previous: Comparison | null;
  /** The recorded boots before this one. */
  earlier: BootSummary[];
  now: Fact[];
  hints: PerformanceHint[];
};

// Recovery

export type RecoveryView = { checks: Diagnosis[]; snapshots: string | null };

// Migration

/** What a machine had, as a migration file holds it. */
export type Migration = {
  version: number;
  from: string;
  family?: string;
  architecture: string;
  made: number;
  entries: string[];
  flatpaks: string[];
  packages: string[];
};

export type MigrationExportView = { migration: Migration; file_name: string; packages_known: boolean };

export type Standing = 'install' | 'here' | 'unavailable' | 'yours';

export type MigrationItem = { kind: 'entry' | 'flatpak' | 'package'; id: string; name: string; standing: Standing; detail: string };

/** A migration file read on this machine. */
export type TranslationView = {
  path: string;
  migration: Migration;
  same_family: boolean;
  items: MigrationItem[];
  install: number;
  here: number;
  unavailable: number;
  yours: number;
};

// Restic

export type ResticSnapshot = { id: string; short_id: string; time: string; hostname: string; paths: string[]; tags?: string[] | null };

export type ResticNode = { name: string; type: string; path: string; size?: number | null; mtime?: string | null };

export type ResticVersion = {
  snapshot: string;
  short_id: string;
  time: string;
  hostname: string;
  size: number | null;
  mtime: string | null;
  change: 'first' | 'changed' | 'same';
};

export type ResticHistory = { path: string; versions: ResticVersion[]; missing_from: number };

export type Baseline = { id: string; path: string; sha256: string; recorded: number };

/** The repository and its snapshots, as `helix-toolbox restic` shows them. */
export type ResticView = {
  configured: boolean;
  repository: string | null;
  origin: string | null;
  password: string | null;
  paths: string[];
  excludes: string[];
  problem: string | null;
  snapshots: ResticSnapshot[];
  timer: string | null;
  restores: boolean;
  baselines: Baseline[];
  home: string;
};

// Offline bundles

export type BundledEntry = { name: string; offline: boolean; network: string | null; flatpak: string | null };

/** What a bundle of some entries would hold, before anything is downloaded. */
export type BundleContents = {
  entries: BundledEntry[];
  /** How it is downloaded, as `bundle create` says it. */
  downloads: string[];
};

/** A bundle that matched its checksums. */
export type BundleView = {
  distribution: string;
  architecture: string;
  version: string;
  release: boolean;
  created: string;
  complete: boolean;
  files: number;
  /** What its files take: "48.2 MB". */
  size: string;
  flatpaks: string[];
  entries: BundledEntry[];
};

// Repositories

/** One repository in the list, as `helix-toolbox repos --json`. */
export type RepoSummary = {
  name: string;
  path: string;
  branch: string | null;
  changed: number;
  upstream: string | null;
  ahead: number | null;
  behind: number | null;
  last: string | null;
  last_at: number | null;
  origin: string | null;
  problem: string | null;
  favorite: boolean;
  group: string;
};

export type ReposView = { roots: string[]; depth: number; origin: string; repositories: RepoSummary[] };

// Home Manager

/** One generation Home Manager built. */
export type HomeGeneration = { id: number; built: string; path: string; current: boolean };

/** A configuration file, and whether Home Manager links it into the Nix store. */
export type HomeLink = { name: string; path: string; target: string | null; managed: boolean };

/** Home Manager, as `helix-toolbox home-manager --json`. */
export type HomeManagerView = {
  installed: boolean;
  version: string | null;
  config: { folder: string; flake: boolean; profile: string | null; origin: string } | null;
  problem: string | null;
  generations: HomeGeneration[];
  repository: Omit<RepoSummary, 'favorite' | 'group'> | null;
  links: HomeLink[];
  /** The `--flake` reference built, for a flake. */
  reference: string | null;
  /** Where the configuration was named. */
  origin: string | null;
};

export type RepoFile = { path: string; original: string | null; state: string; staged: boolean; unstaged: boolean; conflict: boolean };

export type RepoTask = { id: string; name: string; command: string; shell: 'direct' | 'fish' | 'bash'; detected?: boolean };

/** One repository, as `helix-toolbox repos show --json`, with its changes in words. */
export type RepositoryView = {
  summary: Omit<RepoSummary, 'favorite' | 'group'>;
  changes: RepoFile[];
  branches: string[];
  remotes: string[];
  stashes: { id: string; name: string; subject: string }[];
  log: { hash: string; subject: string; at: number | null }[];
  head: string | null;
  branch_ref: string | null;
  busy: string | null;
  push_to: [string, string] | null;
  staged_stat: string;
  favorite: boolean;
  tasks: RepoTask[];
};

export type RepoDiff = { text: string; truncated: boolean };

/** A change to a repository, as `prepare_repository_change` takes it. */
export type RepoChange =
  | { kind: 'stage'; files: string[] }
  | { kind: 'stage_all' }
  | { kind: 'unstage'; files: string[] }
  | { kind: 'commit'; message: string }
  | { kind: 'create_branch'; name: string }
  | { kind: 'switch_branch'; name: string }
  | { kind: 'stash'; message: string; untracked: boolean }
  | { kind: 'apply_stash'; id: string }
  | { kind: 'fetch' }
  | { kind: 'pull' }
  | { kind: 'push' }
  | { kind: 'publish'; remote: string };

/** Ghostty's and Fastfetch's configuration, as `helix-toolbox config` reads them. */
export type ConfigKind = 'ghostty' | 'fastfetch';

/** What the checks say of a configuration's text. */
export type ConfigChecked = { state: 'valid' } | { state: 'unchecked' | 'invalid'; message: string };

export type ConfigField = { key: string; label: string; hint: string; value: string | null };

export type ConfigView = {
  kind: ConfigKind;
  label: string;
  /** Why it cannot be edited, when it cannot. */
  problem: string | null;
  path: string | null;
  live: string | null;
  origin: string | null;
  managed: boolean;
  flake: string | null;
  exists: boolean;
  revision: string | null;
  content: string;
  checked: ConfigChecked | null;
  fields: ConfigField[];
  backups: { backup: string; taken: string | null }[];
};

/** A field changed in a draft, not saved yet. */
export type ConfigDraft = { content: string; fields: ConfigField[] };
