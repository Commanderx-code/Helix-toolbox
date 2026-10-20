// The window's state: the catalog, how entries stand, the pins and the
// Install Plan's queue, and the dialogs every screen uses. The run itself
// lives in RunPanel.

import * as api from './api';
import type { About, AreaView, EntryView, ItemView, Prepared, StatusView } from './types';

export type Screen =
  | { kind: 'dashboard' }
  | { kind: 'area'; index: number }
  | { kind: 'favorites' }
  | { kind: 'plan' }
  | { kind: 'history' }
  | { kind: 'doctor' }
  | { kind: 'updates' }
  | { kind: 'backups' }
  | { kind: 'drift' }
  | { kind: 'reliability' }
  | { kind: 'hardware' }
  | { kind: 'desktop' }
  | { kind: 'settings' };

/** An entry where it is listed: its area and group. */
export type Placed = { entry: EntryView; area: string; group: string | null };

/** A question with one text answer, such as a name. */
export type Prompt = {
  title: string;
  text: string;
  placeholder?: string;
  /** The exact word the answer must be, for a dangerous action. */
  expect?: string;
  resolve: (answer: string | null) => void;
};

function placed(areas: AreaView[]): Placed[] {
  const out: Placed[] = [];
  const walk = (items: ItemView[], area: string, group: string | null) => {
    for (const item of items) {
      if (item.type === 'entry') out.push({ entry: item, area, group });
      else walk(item.items, area, item.name);
    }
  };
  for (const area of areas) walk(area.items, area.name, null);
  return out;
}

class Toolbox {
  screen = $state<Screen>({ kind: 'dashboard' });
  about = $state<About | null>(null);
  areas = $state<AreaView[]>([]);
  statuses = $state<Record<string, StatusView>>({});
  checking = $state(false);
  favorites = $state<string[]>([]);
  /** Entry ids queued in the Install Plan, in order. */
  queue = $state<string[]>([]);
  /** The entry shown in the details pane. */
  selected = $state<string | null>(null);
  search = $state('');
  /** A plan waiting for the user's yes, and the ids it came from. */
  confirming = $state<{ prepared: Prepared; ids: string[]; fromPlan: boolean } | null>(null);
  preparing = $state(false);
  /** The run window is open. */
  running = $state<{ ids: string[]; fromPlan: boolean } | null>(null);
  /** Whether the run in the window is still going. */
  busy = $state(false);
  error = $state<string | null>(null);
  /** Something to read, such as a result or an inspection. */
  notice = $state<{ title: string; text: string; mono?: boolean } | null>(null);
  prompt = $state<Prompt | null>(null);
  /** The setup wizard is open; on the first start it opens by itself. */
  wizard = $state(false);
  /** The command palette (Ctrl-K) is open. */
  palette = $state(false);
  /** The Updates screen checks online when it next opens, once. */
  wantOnline = $state(false);

  everything = $derived(placed(this.areas));
  byId = $derived(new Map(this.everything.map((place) => [place.entry.id, place])));

  async load() {
    try {
      const [about, areas, favorites] = await Promise.all([api.about(), api.areas(), api.favorites()]);
      this.about = about;
      this.areas = areas;
      this.favorites = favorites;
      if (about.first_run) this.wizard = true;
    } catch (error) {
      this.error = String(error);
    }
    await this.refresh();
  }

  /** Show a screen, from the sidebar, the palette or a quick action. */
  open(screen: Screen) {
    this.screen = screen;
    this.search = '';
    this.palette = false;
  }

  /** Show an entry where it is listed, selected. */
  show(id: string) {
    const place = this.byId.get(id);
    if (!place) return;
    const index = this.areas.findIndex((area) => area.name === place.area);
    if (index >= 0) this.open({ kind: 'area', index });
    this.selected = id;
  }

  /** Read the catalog again, after a profile was saved. */
  async reloadAreas() {
    try {
      this.areas = await api.areas();
    } catch (error) {
      this.error = String(error);
    }
  }

  /** Check again how package entries stand; the old answers stay meanwhile. */
  async refresh() {
    this.checking = true;
    try {
      this.statuses = await api.statuses();
    } catch (error) {
      this.error = `How entries stand could not be read: ${error}`;
    } finally {
      this.checking = false;
    }
  }

  entry(id: string): EntryView | undefined {
    return this.byId.get(id)?.entry;
  }

  isQueued(id: string) {
    return this.queue.includes(id);
  }

  toggleQueued(id: string) {
    if (this.isQueued(id)) {
      this.queue = this.queue.filter((other) => other !== id);
    } else if (this.entry(id)?.queueable) {
      this.queue = [...this.queue, id];
    }
  }

  /** Queue these entries, those not queued yet; how many were added. */
  enqueue(ids: string[]): number {
    const added = ids.filter((id) => !this.isQueued(id) && this.entry(id)?.queueable);
    this.queue = [...this.queue, ...added];
    return added.length;
  }

  async togglePin(id: string) {
    try {
      this.favorites = await api.pin(id, !this.favorites.includes(id));
    } catch (error) {
      this.error = String(error);
    }
  }

  /** Have the engine plan and check these entries, then ask. */
  async review(ids: string[], fromPlan: boolean) {
    if (ids.length === 0) return;
    await this.reviewWith(() => api.prepare(ids), ids, fromPlan);
  }

  /** Ask about whatever `prepared` plans: a repair, an update, a restore. */
  async reviewWith(prepared: () => Promise<Prepared>, ids: string[] = [], fromPlan = false) {
    if (this.preparing) return;
    this.preparing = true;
    try {
      this.confirming = { prepared: await prepared(), ids, fromPlan };
    } catch (error) {
      this.error = String(error);
    } finally {
      this.preparing = false;
    }
  }

  async decline() {
    this.confirming = null;
    await api.cancel();
  }

  confirm() {
    if (!this.confirming || this.confirming.prepared.kind !== 'ready') return;
    this.running = { ids: this.confirming.ids, fromPlan: this.confirming.fromPlan };
    this.confirming = null;
  }

  /** The run window closed: what finished leaves the queue. */
  async closeRun(finished: string[]) {
    const fromPlan = this.running?.fromPlan ?? false;
    this.running = null;
    this.busy = false;
    try {
      await api.closeRun();
    } catch (error) {
      this.error = String(error);
    }
    if (fromPlan) {
      this.queue = this.queue.filter((id) => !finished.includes(id));
    }
    await this.refresh();
  }

  /** Ask for a name or a word; null when the user cancels. */
  ask(title: string, text: string, placeholder?: string, expect?: string): Promise<string | null> {
    return new Promise((resolve) => {
      this.prompt = {
        title,
        text,
        placeholder,
        expect,
        resolve: (answer) => {
          this.prompt = null;
          resolve(answer);
        },
      };
    });
  }

  /** Show what a command said, or the problem it had. */
  async tell(title: string, work: () => Promise<string>, mono = false) {
    try {
      this.notice = { title, text: await work(), mono };
      return true;
    } catch (error) {
      this.notice = { title, text: String(error) };
      return false;
    }
  }

  /** Entries matching the search, across every area. */
  matches(text: string): Placed[] {
    const words = text.toLowerCase().split(/\s+/).filter(Boolean);
    if (words.length === 0) return [];
    return this.everything.filter(({ entry, group }) => {
      const haystack = `${entry.name} ${entry.description} ${entry.id} ${group ?? ''}`.toLowerCase();
      return words.every((word) => haystack.includes(word));
    });
  }
}

export const toolbox = new Toolbox();
