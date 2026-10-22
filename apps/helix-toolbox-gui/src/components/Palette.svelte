<script lang="ts">
  // The command palette (Ctrl-K): one searchable list of what the window
  // can do from anywhere, screens, actions and entries, best match first,
  // ranked as the terminal interface ranks them.
  import { modal } from '../lib/keys';
  import Icon from './Icon.svelte';
  import * as api from '../lib/api';
  import { score } from '../lib/search';
  import { toolbox, type Screen } from '../lib/state.svelte';

  type Choice = {
    label: string;
    /** What it is: a screen, an action, an entry's area. */
    detail: string;
    /** More words it is found by. */
    keywords: string;
    /** Shown before anything is typed. */
    listed: boolean;
    act: () => void | Promise<void>;
  };

  /** At most this many results are shown for a query. */
  const SHOWN = 50;

  let query = $state('');
  let cursor = $state(0);
  let input: HTMLInputElement | undefined = $state();
  let list: HTMLDivElement | undefined = $state();

  $effect(() => {
    setTimeout(() => input?.focus(), 0);
  });

  const screens: { screen: Screen; label: string }[] = $derived([
    { screen: { kind: 'dashboard' }, label: 'Dashboard' },
    { screen: { kind: 'favorites' }, label: 'Favorites' },
    { screen: { kind: 'plan' }, label: 'Install Plan' },
    { screen: { kind: 'history' }, label: 'History' },
    ...toolbox.areas.flatMap((area, index) => (area.id === 'doctor' ? [] : [{ screen: { kind: 'area', index } as Screen, label: area.name }])),
    { screen: { kind: 'doctor' }, label: 'Helix Doctor' },
    { screen: { kind: 'updates' }, label: 'Updates' },
    { screen: { kind: 'backups' }, label: 'Backups' },
    { screen: { kind: 'drift' }, label: 'Drift' },
    { screen: { kind: 'reliability' }, label: 'Reliability' },
    { screen: { kind: 'hardware' }, label: 'Hardware' },
    { screen: { kind: 'services' }, label: 'Services' },
    { screen: { kind: 'storage' }, label: 'Storage' },
    { screen: { kind: 'performance' }, label: 'Performance' },
    { screen: { kind: 'recovery' }, label: 'Recovery' },
    { screen: { kind: 'migration' }, label: 'Migration' },
    { screen: { kind: 'bundles' }, label: 'Offline bundles' },
    { screen: { kind: 'desktop' }, label: 'Desktop' },
    { screen: { kind: 'settings' }, label: 'Settings' },
  ]);

  const withUpdate = $derived(Object.entries(toolbox.statuses).filter(([, status]) => status.kind === 'update').map(([id]) => id));

  const choices: Choice[] = $derived.by(() => {
    const out: Choice[] = [];
    for (const { screen, label } of screens) {
      out.push({ label: `Open ${label}`, detail: 'screen', keywords: 'go to show', listed: true, act: () => toolbox.open(screen) });
    }
    const action = (label: string, keywords: string, act: Choice['act']) =>
      out.push({ label, detail: 'action', keywords, listed: true, act });
    action('Check updates online', 'updates refresh', () => {
      toolbox.wantOnline = true;
      toolbox.open({ kind: 'updates' });
    });
    action('Run the health checks', 'doctor diagnostics', () => toolbox.open({ kind: 'doctor' }));
    action('Check again what is installed', 'status refresh packages', () => {
      toolbox.palette = false;
      toolbox.refresh();
    });
    action('Run the Install Plan', 'queue', () => {
      toolbox.palette = false;
      if (toolbox.queue.length === 0) toolbox.notice = { title: 'Install Plan', text: 'The Install Plan is empty.' };
      else toolbox.review(toolbox.queue, true);
    });
    action('Clear the Install Plan', 'queue empty', () => {
      toolbox.palette = false;
      toolbox.queue = [];
    });
    action('Save the Install Plan as a profile', 'queue', async () => {
      toolbox.palette = false;
      if (toolbox.queue.length === 0) {
        toolbox.notice = { title: 'Install Plan', text: 'The Install Plan is empty.' };
        return;
      }
      const name = await toolbox.ask('Save as profile', 'A name for this plan, to run again later.', 'My workstation');
      if (!name) return;
      if (await toolbox.tell('Profile saved', () => api.saveProfile(name, toolbox.queue))) await toolbox.reloadAreas();
    });
    action('Save this machine as a baseline', 'drift snapshot record', async () => {
      toolbox.palette = false;
      const name = await toolbox.ask('Save as baseline', 'A name for how this machine is now, to compare with later.', 'Fresh install');
      if (name) await toolbox.tell('Baseline saved', () => api.saveBaseline(name));
    });
    action(`Update every entry with an update (${withUpdate.length})`, 'upgrade packages', () => {
      toolbox.palette = false;
      if (withUpdate.length === 0) toolbox.notice = { title: 'Updates', text: 'No entry has an update waiting.' };
      else toolbox.reviewWith(() => api.prepareUpdate(withUpdate), withUpdate);
    });
    action('Make a bug report', 'report issue problem copy', () => toolbox.makeReport());
    action('Set up this machine', 'wizard first run workstation', () => {
      toolbox.palette = false;
      toolbox.wizard = true;
    });
    for (const { entry, area } of toolbox.everything) {
      const status = toolbox.statuses[entry.id];
      const verb = entry.kind === 'profile' ? 'Run profile' : status?.kind === 'absent' || (entry.kind === 'package' && !status) ? 'Install' : 'Run';
      out.push({ label: `${verb} ${entry.name}`, detail: area, keywords: entry.id, listed: false, act: () => toolbox.show(entry.id) });
      out.push({
        label: `Inspect ${entry.name}`,
        detail: 'trust and testing',
        keywords: entry.id,
        listed: false,
        act: () => {
          toolbox.palette = false;
          toolbox.tell(`Trust: ${entry.name}`, () => api.trust(entry.id), true);
        },
      });
    }
    return out;
  });

  const shown: Choice[] = $derived.by(() => {
    const text = query.trim();
    if (text === '') return choices.filter((choice) => choice.listed);
    const found: [number, number][] = [];
    choices.forEach((choice, index) => {
      const points = score(text, choice.label, choice.keywords, choice.detail);
      // Actions and screens before entries of the same score.
      if (points !== null) found.push([points * 2 + (choice.listed ? 1 : 0), index]);
    });
    found.sort((a, b) => b[0] - a[0] || a[1] - b[1]);
    return found.slice(0, SHOWN).map(([, index]) => choices[index]);
  });

  $effect(() => {
    query;
    cursor = 0;
  });

  function pick(choice: Choice) {
    toolbox.palette = false;
    choice.act();
  }

  function onkey(event: KeyboardEvent) {
    if (event.key === 'ArrowDown') {
      cursor = Math.min(cursor + 1, shown.length - 1);
    } else if (event.key === 'ArrowUp') {
      cursor = Math.max(cursor - 1, 0);
    } else if (event.key === 'Enter') {
      if (shown[cursor]) pick(shown[cursor]);
    } else if (event.key === 'Escape') {
      toolbox.palette = false;
    } else {
      return;
    }
    event.preventDefault();
    list?.querySelector<HTMLElement>(`[data-index="${cursor}"]`)?.scrollIntoView({ block: 'nearest' });
  }
</script>

<!-- A click on the backdrop itself, outside the palette, closes it. -->
<div
  class="backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) toolbox.palette = false;
  }}
>
  <div class="palette card" role="dialog" aria-modal="true" aria-label="Command palette" use:modal>
    <div class="field">
      <Icon name="search" size={18} />
      <input bind:this={input} bind:value={query} placeholder="Type a screen, an action or an entry…" onkeydown={onkey} />
      <span class="hint">Esc</span>
    </div>
    <div class="list" bind:this={list}>
      {#each shown as choice, index (choice.label + choice.detail)}
        <button class="row" class:current={index === cursor} data-index={index} onmouseenter={() => (cursor = index)} onclick={() => pick(choice)}>
          <span class="label">{choice.label}</span>
          <span class="detail">{choice.detail}</span>
        </button>
      {/each}
      {#if shown.length === 0}
        <p class="faint empty">Nothing matches.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    background: var(--backdrop);
    z-index: 40;
  }

  .palette {
    display: flex;
    flex-direction: column;
    width: min(640px, calc(100vw - 80px));
    max-height: 60vh;
    padding: 0;
    overflow: hidden;
    border-color: var(--selected-line);
    box-shadow: 0 24px 60px var(--shadow);
  }

  .field {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--line);
    color: var(--muted);
  }

  .field input {
    flex: 1;
    height: 30px;
    border: 0;
    background: none;
    color: var(--text);
    font-size: 15px;
    outline: 0;
  }

  .hint {
    padding: 2px 6px;
    border: 1px solid var(--line-strong);
    border-radius: 4px;
    font-family: var(--mono);
    font-size: 11px;
  }

  .list {
    overflow-y: auto;
    padding: 6px;
  }

  .row {
    display: flex;
    width: 100%;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border: 1px solid transparent;
    border-radius: var(--radius-small);
    background: none;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .row.current {
    background: var(--selected);
    border-color: var(--selected-line);
  }

  .label {
    flex: 1;
    font-weight: 600;
  }

  .detail {
    color: var(--faint);
    font-size: 12px;
  }

  .row.current .detail {
    color: var(--muted);
  }

  .empty {
    padding: 12px;
  }
</style>
