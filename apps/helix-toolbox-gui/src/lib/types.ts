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

// Hardware

export type Hardware = {
  firmware: {
    available: boolean;
    devices: { id: string; name: string; vendor: string | null; version: string | null; updatable: boolean }[];
    updates: { device_id: string; device: string; from: string | null; to: string; summary: string | null; urgency: string | null }[];
    note: string | null;
  };
  disks: { name: string; model: string | null; bytes: number | null; kind: string; firmware: string | null; celsius: number | null }[];
  sensors: { chip: string; label: string; celsius: number; high: number | null; critical: number | null }[];
  batteries: {
    name: string;
    status: string | null;
    capacity: number | null;
    health: number | null;
    cycles: number | null;
    model: string | null;
    technology: string | null;
  }[];
  pci: Device[];
  usb: Device[];
};

export type Device = { address: string; class: string | null; name: string; driver: string | null };

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
};

// Services and startup

export type UnitView = {
  name: string;
  description: string;
  enablement: string | null;
  active: string | null;
  sub: string | null;
  user: boolean;
};

export type AutostartView = {
  file: string;
  name: string;
  comment: string;
  exec: string;
  enabled: boolean;
  here: boolean;
  system: boolean;
  own: string | null;
};

export type ServicesView = {
  systemd: boolean;
  session: boolean;
  units: UnitView[];
  autostart: AutostartView[];
};

export type ServiceDetails = { needed_by: string[]; consequence: string | null };

// Storage

export type StorageItem = {
  id: string;
  title: string;
  detail: string;
  size: number | null;
  frees: number | null;
  packages: string[];
  not_offered: string | null;
};

export type StorageView = {
  filesystems: { path: string; free: number; total: number }[];
  items: StorageItem[];
  kept: { title: string; detail: string; size: number | null }[];
};

// Offline bundles

export type BundledEntry = { name: string; offline: boolean; network: string | null; flatpak: string | null };

/** What a bundle of some entries would hold, before anything is downloaded. */
export type BundleContents = {
  entries: BundledEntry[];
  files: number;
  dnf: string[];
  zypper: string[];
  flatpaks: string[];
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
  bytes: number;
  flatpaks: string[];
  entries: BundledEntry[];
};
