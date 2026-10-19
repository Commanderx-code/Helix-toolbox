<script lang="ts">
  import Icon from './Icon.svelte';
  import Flags from './Flags.svelte';
  import StatusBadge from './StatusBadge.svelte';
  import { toolbox } from '../lib/state.svelte';
  import type { EntryView } from '../lib/types';

  let { entry, place = null }: { entry: EntryView; place?: string | null } = $props();

  const queued = $derived(toolbox.isQueued(entry.id));
  const pinned = $derived(toolbox.favorites.includes(entry.id));
</script>

<div
  class="row"
  class:selected={toolbox.selected === entry.id}
  role="button"
  tabindex="0"
  onclick={() => (toolbox.selected = entry.id)}
  onkeydown={(event) => {
    if (event.key === 'Enter') toolbox.selected = entry.id;
    if (event.key === ' ') {
      event.preventDefault();
      toolbox.toggleQueued(entry.id);
    }
  }}
>
  {#if entry.queueable}
    <button
      class="queue"
      class:queued
      title={queued ? 'Take out of the Install Plan' : 'Add to the Install Plan'}
      onclick={(event) => {
        event.stopPropagation();
        toolbox.toggleQueued(entry.id);
      }}
    >
      {#if queued}<Icon name="check" size={14} />{/if}
    </button>
  {:else}
    <span class="alone" title="Runs on its own"><Icon name="play" size={12} /></span>
  {/if}
  <div class="text">
    <div class="name">
      {entry.name}
      {#if pinned}<span class="pin"><Icon name="star" size={12} /></span>{/if}
    </div>
    <div class="description">
      {#if place}<span class="place">{place} ·</span>{/if}
      {entry.description}
    </div>
  </div>
  <span><StatusBadge status={toolbox.statuses[entry.id]} /></span>
  <span class="flags"><Flags field={entry.flags} /></span>
</div>

<style>
  .row {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto 92px;
    align-items: center;
    gap: 12px;
    padding: 8px 14px;
    border-radius: var(--radius-small);
    border: 1px solid transparent;
    cursor: pointer;
  }

  .row:hover {
    background: var(--panel-raised);
  }

  .row.selected {
    background: var(--selected);
    border-color: var(--selected-line);
  }

  .queue {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    padding: 0;
    border-radius: 5px;
    border: 1.5px solid var(--line-strong);
    background: transparent;
    cursor: pointer;
  }

  .queue:hover {
    border-color: var(--cyan);
  }

  .queue.queued {
    background: var(--cyan);
    border-color: var(--cyan);
    color: var(--on-accent);
  }

  .alone {
    display: grid;
    place-items: center;
    color: var(--faint);
  }

  .text {
    min-width: 0;
  }

  .name {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }

  .pin {
    display: inline-flex;
    color: var(--yellow);
  }

  .description {
    overflow: hidden;
    color: var(--muted);
    font-size: 13px;
    white-space: nowrap;
    text-overflow: ellipsis;
  }

  .place {
    color: var(--faint);
  }

  .flags {
    display: flex;
    justify-content: flex-end;
  }
</style>
