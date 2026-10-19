// The window's state: the catalog, how entries stand, the pins and the
// Install Plan's queue. The run itself lives in RunPanel.

import * as api from './api';
import type { About, AreaView, EntryView, ItemView, Prepared, StatusView } from './types';

export type Screen =
  | { kind: 'dashboard' }
  | { kind: 'area'; index: number }
  | { kind: 'favorites' }
  | { kind: 'plan' }
  | { kind: 'history' };

/** An entry where it is listed: its area and group. */
export type Placed = { entry: EntryView; area: string; group: string | null };

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

  everything = $derived(placed(this.areas));
  byId = $derived(new Map(this.everything.map((place) => [place.entry.id, place])));

  async load() {
    try {
      const [about, areas, favorites] = await Promise.all([api.about(), api.areas(), api.favorites()]);
      this.about = about;
      this.areas = areas;
      this.favorites = favorites;
    } catch (error) {
      this.error = String(error);
    }
    await this.refresh();
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

  async togglePin(id: string) {
    try {
      this.favorites = await api.pin(id, !this.favorites.includes(id));
    } catch (error) {
      this.error = String(error);
    }
  }

  /** Have the engine plan and check these entries, then ask. */
  async review(ids: string[], fromPlan: boolean) {
    if (ids.length === 0 || this.preparing) return;
    this.preparing = true;
    try {
      const prepared = await api.prepare(ids);
      this.confirming = { prepared, ids, fromPlan };
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
