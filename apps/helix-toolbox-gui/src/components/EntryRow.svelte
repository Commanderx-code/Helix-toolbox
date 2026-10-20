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

<div class="row" class:selected={toolbox.selected === entry.id}>
  {#if entry.queueable}
    <!-- Out of the Tab order: Space on the row queues it. -->
    <button
      class="queue"
      class:queued
      tabindex="-1"
      aria-pressed={queued}
      aria-label={queued ? `Take ${entry.name} out of the Install Plan` : `Add ${entry.name} to the Install Plan`}
      title={queued ? 'Take out of the Install Plan' : 'Add to the Install Plan'}
      onclick={() => toolbox.toggleQueued(entry.id)}
    >
      {#if queued}<Icon name="check" size={14} />{/if}
    </button>
  {:else}
    <span class="alone" role="img" aria-label="Runs on its own" title="Runs on its own"><Icon name="play" size={12} /></span>
  {/if}
  <!-- The whole row selects the entry; Space queues it, as in the terminal interface. -->
  <button
    class="select"
    aria-current={toolbox.selected === entry.id ? 'true' : undefined}
    aria-describedby="status-{entry.id}"
    onclick={() => (toolbox.selected = entry.id)}
    onkeydown={(event) => {
      if (event.key === ' ' && entry.queueable) {
        event.preventDefault();
        toolbox.toggleQueued(entry.id);
      }
    }}
  >
    <span class="name">
      {entry.name}
      {#if pinned}<span class="pin" role="img" aria-label="pinned"><Icon name="star" size={12} /></span>{/if}
    </span>
    <span class="description">
      {#if place}<span class="place">{place} ·</span>{/if}
      {entry.description}
    </span>
  </button>
  <span id="status-{entry.id}"><StatusBadge status={toolbox.statuses[entry.id]} /></span>
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
    position: relative;
  }

  .row:has(.select:focus-visible) {
    outline: 2px solid var(--cyan);
    outline-offset: 1px;
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

  .queue {
    position: relative;
    z-index: 1;
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

  .select {
    display: grid;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .select:focus-visible {
    outline: none;
  }

  /* The select button covers the row, under the checkbox. */
  .select::after {
    content: '';
    position: absolute;
    inset: 0;
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
