// The commands of the Tauri side. Outside Tauri (`npm run dev` in a
// browser) the same calls answer with sample data from mock.ts, so the
// interface can be worked on without touching the machine.

import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  About,
  AreaView,
  BackupsView,
  BundleContents,
  BundleView,
  ChangesView,
  Comparison,
  ConfigChecked,
  ConfigDraft,
  ConfigKind,
  ConfigView,
  DesktopView,
  Diagnosis,
  DriftView,
  Hardware,
  HardwareReport,
  PerformanceView,
  PickView,
  Prepared,
  RecordView,
  RecoveryView,
  ResticHistory,
  ResticNode,
  ResticView,
  Baseline,
  MigrationExportView,
  TranslationView,
  RepoChange,
  RepoDiff,
  HomeManagerView,
  ReposView,
  RepositoryView,
  RepoTask,
  ReliabilityView,
  RunEvent,
  ServiceDetails,
  ServicesView,
  SettingsView,
  SetupView,
  StatusView,
  StorageView,
  TargetView,
  UpdatesView,
  WindowTheme,
} from './types';

const inTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (inTauri) {
    return invoke<T>(command, args);
  }
  const mock = await import('./mock');
  return mock.call(command, args) as Promise<T>;
}

export const about = () => call<About>('about');
export const areas = () => call<AreaView[]>('areas');
export const statuses = () => call<Record<string, StatusView>>('statuses');
export const favorites = () => call<string[]>('favorites');
export const pin = (id: string, pinned: boolean) => call<string[]>('pin', { id, pinned });
export const prepare = (ids: string[]) => call<Prepared>('prepare', { ids });
export const changes = () => call<ChangesView>('changes');
export const downloadSources = () => call<ChangesView>('download_sources');
export const cancel = () => call<void>('cancel');
export const write = (data: string) => call<void>('write', { data });
export const resize = (rows: number, cols: number) => call<void>('resize', { rows, cols });
export const stop = () => call<void>('stop');
export const closeRun = () => call<void>('close_run');
export const runs = () => call<RecordView[]>('runs');
export const saveProfile = (name: string, ids: string[]) => call<string>('save_profile', { name, ids });
/** Import a profile file, chosen in the file chooser; null when it was closed. */
export const importProfile = () => call<string | null>('import_profile', { path: null });
/** Export one of the user's profiles to a file the file chooser names; null when it was closed. */
export const exportProfile = (id: string) => call<string | null>('export_profile', { id, path: null });
export const deleteProfile = (id: string) => call<string>('delete_profile', { id });
/** `helix-toolbox report` and the window's own facts, as Markdown. */
export const bugReport = () => call<string>('bug_report');
export const copyText = (text: string) => call<void>('copy_text', { text });
export const hardwareReport = (at: number | null, entry: string | null) => call<HardwareReport>('hardware_report', { at, entry });
export const openIssue = (url: string) => call<void>('open_issue', { url });

export const doctor = () => call<Diagnosis[]>('doctor');
export const prepareRepair = (id: string) => call<Prepared>('prepare_repair', { id });

export const updates = (online: boolean) => call<UpdatesView>('updates', { online });
export const prepareChannels = (ids: string[]) => call<Prepared>('prepare_channels', { ids });
export const prepareUpdate = (ids: string[]) => call<Prepared>('prepare_update', { ids });
export const prepareRemove = (ids: string[]) => call<Prepared>('prepare_remove', { ids });

export const backups = () => call<BackupsView>('list_backups');
export const prepareRestore = (index: number) => call<Prepared>('prepare_restore', { index });

export const driftTargets = () => call<TargetView[]>('drift_targets');
export const driftCheck = (key: string) => call<DriftView>('drift_check', { key });
export const saveBaseline = (name: string) => call<string>('save_baseline', { name });

export const reliability = () => call<ReliabilityView>('reliability');
export const trust = (id: string) => call<string>('trust', { id });
export const checkEntry = (id: string) => call<string>('check_entry', { id });

export const services = () => call<ServicesView>('read_services');
export const serviceDetails = (name: string, user: boolean) => call<ServiceDetails>('service_details', { name, user });
export const prepareService = (name: string, user: boolean, verb: string) => call<Prepared>('prepare_service', { name, user, verb });
export const prepareAutostart = (file: string, on: boolean) => call<Prepared>('prepare_autostart', { file, on });
export const storage = () => call<StorageView>('read_storage');
export const prepareClean = (id: string) => call<Prepared>('prepare_clean', { id });
export const performance = () => call<PerformanceView>('read_performance');
export const compareBoots = (before: string, after: string) => call<Comparison>('compare_boots', { before, after });
export const recovery = () => call<RecoveryView>('read_recovery');
export const prepareSnapshotCheck = () => call<Prepared>('prepare_snapshot_check');
export const migrationExport = () => call<MigrationExportView>('read_migration_export');
/** Write this machine's migration to `path`, or where the file chooser says; null when it was closed. */
export const exportMigration = (path: string | null = null) => call<string | null>('export_migration', { path });
/** Read a migration file from `path`, or one the file chooser names; null when it was closed. */
export const openMigration = (path: string | null = null) => call<TranslationView | null>('open_migration', { path });
export const prepareMigration = (path: string) => call<Prepared>('prepare_migration', { path });
export const restic = () => call<ResticView>('read_restic');
export const resticFiles = (snapshot: string, folder: string) => call<ResticNode[]>('restic_files', { snapshot, folder });
export const resticHistory = (pattern: string, ignoreCase: boolean) => call<ResticHistory[]>('restic_history', { pattern, ignoreCase });
/** A restic plan: `check`, `backup`, `restore` (into the new folder `target`) or `test` (a baseline against a snapshot). */
export const prepareRestic = (
  action: 'check' | 'backup' | 'restore' | 'test',
  options: { snapshot?: string; target?: string; include?: string; baseline?: string } = {},
) => call<Prepared>('prepare_restic', { action, snapshot: null, target: null, include: null, baseline: null, ...options });
export const recordBaseline = (path: string) => call<Baseline>('record_baseline', { path });
export const setResticTimer = (on: boolean, weekly: boolean) => call<string>('set_restic_timer', { on, weekly });
export const repositories = () => call<ReposView>('read_repositories');
export const homeManager = () => call<HomeManagerView>('read_home_manager');
/** A Home Manager plan: `build`, `switch`, `activate` (an earlier generation) or `pull` (its repository). */
export const prepareHomeManager = (action: 'build' | 'switch' | 'activate' | 'pull', generation: number | null = null) =>
  call<Prepared>('prepare_home_manager', { action, generation });
export const repository = (path: string) => call<RepositoryView>('read_repository', { path });
export const repositoryDiff = (path: string, file: string | null, staged: boolean) =>
  call<RepoDiff>('repository_diff', { path, file, staged });
export const prepareRepositoryChange = (path: string, change: RepoChange) =>
  call<Prepared>('prepare_repository_change', { path, change });
export const prepareTask = (path: string, id: string) => call<Prepared>('prepare_task', { path, id });
/** Mark a favorite, keep a task of the user's or remove one, in workspaces.toml. */
export const saveWorkspace = (path: string, change: { favorite?: boolean; task?: RepoTask; remove?: string }) =>
  call<null>('save_workspace', { path, favorite: change.favorite ?? null, task: change.task ?? null, remove: change.remove ?? null });

export const configs = () => call<ConfigView[]>('read_configs');
export const validateConfig = (kind: ConfigKind, text: string) => call<ConfigChecked>('validate_config', { kind, text });
export const setConfigField = (kind: ConfigKind, text: string, key: string, value: string) =>
  call<ConfigDraft>('set_config_field', { kind, text, key, value });
/** The plan that saves a draft over the file read (`path`, unchanged since `revision`). */
export const prepareConfigSave = (kind: ConfigKind, path: string, revision: string, text: string) =>
  call<Prepared>('prepare_config_save', { kind, path, revision, text });
export const prepareConfigRestore = (kind: ConfigKind, backup: string) => call<Prepared>('prepare_config_restore', { kind, backup });

export const hardware = () => call<Hardware>('read_hardware');
export const prepareFirmware = (index: number) => call<Prepared>('prepare_firmware', { index });
export const prepareSmart = (disk: string) => call<Prepared>('prepare_smart', { disk });

export const desktop = () => call<DesktopView>('read_desktop');
export const saveLook = (name: string) => call<string>('save_look', { name });
export const prepareLook = (slug: string) => call<Prepared>('prepare_look', { slug });

export const bundleContents = (ids: string[], complete: boolean) =>
  call<BundleContents>('bundle_contents', { ids, complete });
export const prepareBundleCreate = (directory: string, ids: string[], complete: boolean) =>
  call<Prepared>('prepare_bundle_create', { directory, ids, complete });
export const bundleCheck = (directory: string) => call<BundleView>('bundle_check', { directory });
export const prepareBundleInstall = (directory: string, trust: boolean) =>
  call<Prepared>('prepare_bundle_install', { directory, trust });

export const setupStart = () => call<SetupView>('setup_start');
export const setupChoose = (id: string) => call<PickView[]>('setup_choose', { id });
export const setupDone = () => call<void>('setup_done');

export const settings = () => call<SettingsView>('read_settings');
export const saveSettings = (keepGoing: boolean, snapshots: boolean) =>
  call<SettingsView>('save_settings', { keepGoing, snapshots });
export const saveTheme = (theme: WindowTheme) => call<SettingsView>('save_theme', { theme });
export const saveWatch = (updates: boolean, health: boolean, drift: string | null) =>
  call<string>('save_watch', { updates, health, drift });

/** Run the confirmed plan; `onEvent` gets each event as it comes. */
export async function run(rows: number, cols: number, onEvent: (event: RunEvent) => void) {
  if (inTauri) {
    const events = new Channel<RunEvent>();
    events.onmessage = onEvent;
    return invoke<void>('run', { rows, cols, events });
  }
  const mock = await import('./mock');
  return mock.run(onEvent);
}

/** Output bytes from an event's base64. */
export function bytes(data: string): Uint8Array {
  const text = atob(data);
  const out = new Uint8Array(text.length);
  for (let index = 0; index < text.length; index++) {
    out[index] = text.charCodeAt(index);
  }
  return out;
}
