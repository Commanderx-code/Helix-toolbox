<script lang="ts">
  import { arrows } from '../lib/keys';
  import EntryRow from '../components/EntryRow.svelte';
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { Diagnosis } from '../lib/types';

  let checks = $state<Diagnosis[] | null>(null);
  let problem = $state<string | null>(null);
  let selectedId = $state<string | null>(null);
  let examining = $state(false);

  const selected = $derived(checks?.find((check) => check.id === selectedId) ?? null);
  const counts = $derived({
    problem: checks?.filter((check) => check.health === 'problem').length ?? 0,
    notice: checks?.filter((check) => check.health === 'notice').length ?? 0,
    good: checks?.filter((check) => check.health === 'good').length ?? 0,
  });
  /** The Helix Doctor area's entries: reports and repair tools. */
  const tools = $derived(toolbox.areas.find((area) => area.id === 'doctor')?.items ?? []);
  const repairEntry = $derived(selected?.entry ? toolbox.entry(selected.entry) : undefined);

  async function examine() {
    examining = true;
    try {
      checks = await api.doctor();
      problem = null;
      if (!selectedId || !checks.some((check) => check.id === selectedId)) {
        selectedId = checks.find((check) => check.health !== 'good')?.id ?? checks[0]?.id ?? null;
      }
    } catch (error) {
      problem = String(error);
    } finally {
      examining = false;
    }
  }

  examine();
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Helix Doctor</h1>
      <p class="screen-subtitle">The checks only read the system; a repair runs after its own confirmation.</p>
    </div>
    <div class="screen-buttons">
      {#if checks}
        <span class="pill problem">{counts.problem} {counts.problem === 1 ? 'problem' : 'problems'}</span>
        <span class="pill notice">{counts.notice} {counts.notice === 1 ? 'notice' : 'notices'}</span>
        <span class="pill good">{counts.good} OK</span>
      {/if}
      <button class="button" disabled={examining} onclick={examine}>
        <Icon name="refresh" size={16} />
        {examining ? 'Checking…' : 'Check again'}
      </button>
    </div>
  </div>

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if checks === null}
        <p class="empty-note">Running the health checks…</p>
      {:else}
        <div class="rows" use:arrows={{ items: '.row-item', follow: '.row-item' }}>
          {#each checks as check (check.id)}
            <button class="row-item check" class:selected={selectedId === check.id} onclick={() => (selectedId = check.id)}>
              <span class="pill {check.health}">{check.health_label}</span>
              <span>
                <span class="name">{check.title}</span>
                <span class="sub">{check.summary}</span>
              </span>
              {#if check.repair || check.entry}
                <span class="faint fix" title="Has a repair"><Icon name="maintenance" size={14} /></span>
              {/if}
            </button>
          {/each}
        </div>
        {#if tools.length > 0}
          <div class="heading-row">Reports and tools</div>
          <div class="rows" use:arrows={{ items: '.select', follow: '.select' }}>
            {#each tools as item (item.id)}
              {#if item.type === 'entry'}
                <EntryRow entry={item} />
              {/if}
            {/each}
          </div>
        {/if}
      {/if}
    </div>

    <aside class="pane side card">
      {#if selected}
        <span class="pill {selected.health}">{selected.health_label}</span>
        <h2>{selected.title}</h2>
        <p>{selected.summary}</p>
        {#if selected.evidence.length > 0}
          <h3>What was found</h3>
          <ul>
            {#each selected.evidence as line, index (index)}<li class="mono small">{line}</li>{/each}
          </ul>
        {/if}
        {#if selected.advice}
          <h3>What to do</h3>
          <p>{selected.advice}</p>
        {/if}
        {#if selected.repair}
          <h3>Repair</h3>
          <p>{selected.repair.label}</p>
          <ul>
            {#each selected.repair.commands as command, index (index)}<li class="mono small">{command}</li>{/each}
          </ul>
        {:else if repairEntry}
          <h3>Repair</h3>
          <p>The <strong>{repairEntry.name}</strong> entry addresses it.</p>
        {/if}
        {#if selected.repair || repairEntry}
          <div class="actions">
            <button
              class="button primary"
              disabled={toolbox.preparing || toolbox.busy}
              onclick={() => toolbox.reviewWith(() => api.prepareRepair(selected.id))}
            >
              <Icon name="maintenance" size={15} /> Repair…
            </button>
          </div>
        {/if}
      {:else if toolbox.selected && toolbox.entry(toolbox.selected)}
        <h2>{toolbox.entry(toolbox.selected)?.name}</h2>
        <p>{toolbox.entry(toolbox.selected)?.description}</p>
        <div class="actions">
          <button class="button primary" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.review([toolbox.selected!], false)}>
            <Icon name="play" size={14} /> Run now
          </button>
        </div>
      {:else}
        <p class="faint">Select a check to see what it found.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .check {
    grid-template-columns: 100px minmax(0, 1fr) auto;
  }

  .check > span:nth-child(2) {
    display: grid;
    min-width: 0;
  }

  .fix {
    display: inline-flex;
  }

  .small {
    font-size: 12.5px;
  }

  .bad {
    color: var(--red);
  }
</style>
