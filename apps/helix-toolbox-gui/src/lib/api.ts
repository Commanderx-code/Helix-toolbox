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
  DesktopView,
  Diagnosis,
  DriftView,
  Hardware,
  PickView,
  Prepared,
  RecordView,
  SetupView,
  ReliabilityView,
  RunEvent,
  SettingsView,
  StatusView,
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

export const doctor = () => call<Diagnosis[]>('doctor');
export const prepareRepair = (id: string) => call<Prepared>('prepare_repair', { id });

export const updates = (online: boolean) => call<UpdatesView>('updates', { online });
export const prepareChannels = (ids: string[]) => call<Prepared>('prepare_channels', { ids });
export const prepareUpdate = (ids: string[]) => call<Prepared>('prepare_update', { ids });

export const backups = () => call<BackupsView>('list_backups');
export const prepareRestore = (index: number) => call<Prepared>('prepare_restore', { index });

export const driftTargets = () => call<TargetView[]>('drift_targets');
export const driftCheck = (key: string) => call<DriftView>('drift_check', { key });
export const saveBaseline = (name: string) => call<string>('save_baseline', { name });

export const reliability = () => call<ReliabilityView>('reliability');
export const trust = (id: string) => call<string>('trust', { id });

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
