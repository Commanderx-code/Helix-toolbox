// The commands of the Tauri side. Outside Tauri (`npm run dev` in a
// browser) the same calls answer with sample data from mock.ts, so the
// interface can be worked on without touching the machine.

import { Channel, invoke } from '@tauri-apps/api/core';
import type { About, AreaView, Prepared, RecordView, RunEvent, StatusView } from './types';

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
export const cancel = () => call<void>('cancel');
export const write = (data: string) => call<void>('write', { data });
export const resize = (rows: number, cols: number) => call<void>('resize', { rows, cols });
export const stop = () => call<void>('stop');
export const closeRun = () => call<void>('close_run');
export const runs = () => call<RecordView[]>('runs');

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
