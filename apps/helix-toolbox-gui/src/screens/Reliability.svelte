<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { ReliabilityView } from '../lib/types';

  let view = $state<ReliabilityView | null>(null);
  let problem = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let criticalOnly = $state(false);

  const rows = $derived((view?.rows ?? []).filter((row) => !criticalOnly || row.assessment.critical));
  const selected = $derived(view?.rows.find((row) => row.id === selectedId) ?? null);
  const counts = $derived.by(() => {
    const out = new Map<string, number>();
    for (const row of view?.rows ?? []) out.set(row.level, (out.get(row.level) ?? 0) + 1);
    return [...out.entries()];
  });

  async function load() {
    try {
      view = await api.reliability();
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  load();

  const tone = (level: string) =>
    level.startsWith('Not yet') ? 'problem' : level.includes('stand-in') ? 'notice' : 'good';

  const date = (unix: number) => new Date(unix * 1000).toISOString().slice(0, 10);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Reliability</h1>
      <p class="screen-subtitle">{view?.summary ?? 'How far each entry has been tested, critical ones first.'}</p>
    </div>
    <div class="screen-buttons">
      <label class="toggle inline">
        <input type="checkbox" bind:checked={criticalOnly} />
        <span>Critical only</span>
      </label>
    </div>
  </div>

  {#if view && view.problems.length > 0}
    <div class="card problems">
      {#each view.problems as line, index (index)}<div><Icon name="warning" size={14} /> {line}</div>{/each}
    </div>
  {/if}

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if view === null}
        <p class="empty-note">Reading the record…</p>
      {:else}
        <div class="summary">
          {#each counts as [level, count] (level)}<span class="pill {tone(level)}">{count} {level.toLowerCase()}</span>{/each}
        </div>
        <div class="rows">
          {#each rows as row (row.id)}
            <button class="row-item entry" class:selected={selectedId === row.id} onclick={() => (selectedId = row.id)}>
              <span class="pill {tone(row.level)}">{row.level}</span>
              <span>
                <span class="name">{row.name}{#if row.assessment.critical}<span class="critical" title="Boot, login or drivers">critical</span>{/if}</span>
                <span class="sub">{row.assessment.systems.join(', ') || 'no system recorded'}{row.assessment.verified ? ` · last verified ${row.assessment.verified}` : ''}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <aside class="pane side card">
      {#if selected}
        <span class="pill {tone(selected.level)}">{selected.level}</span>
        <h2>{selected.name}</h2>
        <p>{selected.explain}</p>
        {#if selected.assessment.critical}
          <p class="warning"><Icon name="warning" size={14} /> A critical entry: if it fails, the machine may not boot, show its login screen or display, or it changes disks or the whole system. Keep a snapshot or a live USB at hand.</p>
        {/if}
        <dl class="kv">
          {#if selected.assessment.systems.length > 0}<dt>Tested on</dt><dd>{selected.assessment.systems.join(', ')}</dd>{/if}
          {#if selected.assessment.verified}<dt>Last verified</dt><dd>{selected.assessment.verified}</dd>{/if}
          {#if selected.assessment.here !== null}<dt>Ran here</dt><dd>{date(selected.assessment.here)}</dd>{/if}
        </dl>
        {#if selected.assessment.evidence.length > 0}
          <h3>What shows it</h3>
          <ul>
            {#each selected.assessment.evidence as line, index (index)}<li>{line}</li>{/each}
          </ul>
        {/if}
        <div class="actions">
          <button class="button" onclick={() => toolbox.tell(`Trust: ${selected.name}`, () => api.trust(selected.id), true)}>
            <Icon name="eye" size={15} /> Where it comes from
          </button>
        </div>
      {:else}
        <p class="faint">Select an entry to see how it was tested.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .inline {
    padding: 6px 12px;
    align-items: center;
  }

  .inline input {
    margin: 0;
  }

  .problems {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 14px;
    color: var(--yellow);
  }

  .problems div {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
  }

  .entry {
    grid-template-columns: 170px minmax(0, 1fr);
  }

  .entry > span:nth-child(2) {
    display: grid;
    min-width: 0;
  }

  .critical {
    margin-left: 8px;
    padding: 0 6px;
    border-radius: 4px;
    background: rgba(255, 120, 140, 0.15);
    color: var(--red);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
  }

  .warning {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--yellow);
  }

  .bad {
    color: var(--red);
  }
</style>
