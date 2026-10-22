<script lang="ts">
  // The storage and cleanup center, as `helix-toolbox storage`: the space,
  // what can be cleaned with what each frees, and what is never cleaned
  // here. Each cleanup is a plan, reviewed first.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { StorageView } from '../lib/types';

  let found = $state<StorageView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);

  async function load() {
    reading = true;
    try {
      found = await api.storage();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  $effect(() => {
    void toolbox.runs;
    load();
  });

  /** A size for people, as the command line writes it. */
  function size(bytes: number | null): string {
    if (bytes === null) return '?';
    for (const [unit, scale] of [
      ['TB', 1e12],
      ['GB', 1e9],
      ['MB', 1e6],
      ['KB', 1e3],
    ] as const) {
      const value = bytes / scale;
      if (value >= 1) return `${value >= 100 ? value.toFixed(0) : value.toFixed(1)} ${unit}`;
    }
    return `${bytes} B`;
  }
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Storage</h1>
      <p class="screen-subtitle">Where the space goes and what can be cleaned; each cleanup shows its plan and asks first.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  {#if problem}
    <p class="card empty-note bad">{problem}</p>
  {:else if found === null}
    <p class="card empty-note">Reading the storage; sizing the caches takes a moment…</p>
  {:else}
    <section class="card part">
      <h2>Space</h2>
      {#each found.filesystems as filesystem (filesystem.path)}
        {@const used = 1 - filesystem.free / Math.max(filesystem.total, 1)}
        <div class="space">
          <span class="mono">{filesystem.path}</span>
          <span class="bar" role="meter" aria-label="Used on {filesystem.path}" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(used * 100)}>
            <span class="fill" class:full={used > 0.9} style="width: {Math.round(used * 100)}%"></span>
          </span>
          <span>{size(filesystem.free)} free of {size(filesystem.total)}</span>
        </div>
      {/each}
    </section>

    <section class="card part">
      <h2>What can be cleaned</h2>
      <ul class="items">
        {#each found.items as item (item.id)}
          <li>
            <span>
              <span class="name">{item.title}</span>
              <span class="sub">{item.detail}</span>
              {#if item.packages.length > 0}<span class="sub mono">{item.packages.join(', ')}</span>{/if}
            </span>
            <span class="size">{size(item.size)}</span>
            <span class="act">
              {#if item.not_offered}
                <span class="faint">{item.not_offered}</span>
              {:else}
                <span class="frees">{item.frees === null ? 'frees what it finds' : `frees ${size(item.frees)}`}</span>
                <button
                  class="button"
                  disabled={toolbox.preparing || toolbox.busy}
                  onclick={() => toolbox.reviewWith(() => api.prepareClean(item.id))}
                >
                  <Icon name="trash" size={14} /> Clean…
                </button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    </section>

    {#if found.kept.length > 0}
      <section class="card part">
        <h2>Kept, never cleaned here</h2>
        <dl class="kv">
          {#each found.kept as kept (kept.title)}
            <dt>{kept.title}</dt>
            <dd>{kept.detail}{kept.size !== null ? ` (${size(kept.size)})` : ''}</dd>
          {/each}
        </dl>
      </section>
    {/if}
  {/if}
</div>

<style>
  .screen {
    overflow-y: auto;
  }

  .part {
    padding: 16px 18px;
  }

  .part h2 {
    margin: 0 0 10px;
  }

  .space {
    display: grid;
    grid-template-columns: minmax(80px, auto) minmax(120px, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 4px 0;
  }

  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
  }

  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }

  .fill.full {
    background: var(--red);
  }

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .items li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 80px minmax(200px, auto);
    gap: 14px;
    align-items: center;
    padding: 10px 0;
    border-top: 1px solid var(--line);
  }

  .items li > span:first-child {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .size {
    color: var(--muted);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .act {
    display: flex;
    gap: 10px;
    align-items: center;
    justify-content: flex-end;
  }

  .frees {
    color: var(--green);
  }

  .bad {
    color: var(--red);
  }
</style>
