<script lang="ts">
  // Recovery readiness, as `helix-toolbox recovery`: whether this machine
  // could be brought back after a bad update or a lost disk. It only
  // reads; the snapshot check and the entries that set something up are
  // plans, reviewed first.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { RecoveryView } from '../lib/types';

  let found = $state<RecoveryView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);

  async function load() {
    reading = true;
    try {
      found = await api.recovery();
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

  const counts = $derived({
    good: found?.checks.filter((check) => check.health === 'good').length ?? 0,
    notice: found?.checks.filter((check) => check.health !== 'good').length ?? 0,
  });
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Recovery</h1>
      <p class="screen-subtitle">Whether this machine could be brought back after a bad update or a lost disk. It only reads.</p>
    </div>
    <div class="screen-buttons">
      {#if found}
        <span class="pill notice">{counts.notice} to look at</span>
        <span class="pill good">{counts.good} ready</span>
      {/if}
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Checking…' : 'Check again'}</button>
    </div>
  </div>

  {#if problem}
    <p class="card empty-note bad">{problem}</p>
  {:else if found === null}
    <p class="card empty-note">Checking snapshots, kernels and backups…</p>
  {:else}
    {#each found.checks as check (check.id)}
      {@const entry = check.entry ? toolbox.entry(check.entry) : undefined}
      <section class="card part">
        <div class="head">
          <span class="pill {check.health}">{check.health_label}</span>
          <h2>{check.title}</h2>
        </div>
        <p>{check.summary}.</p>
        {#if check.evidence.length > 0}
          <ul class="evidence">
            {#each check.evidence as line, index (index)}<li class="muted">{line}</li>{/each}
          </ul>
        {/if}
        {#if check.advice && (check.health !== 'good' || check.id === 'snapshots')}
          <p class="muted">{check.advice.replace('helix-toolbox recovery --check-snapshots', 'Check snapshots')}</p>
        {/if}
        {#if (check.id === 'snapshots' && found.snapshots) || entry}
          <div class="actions">
            {#if check.id === 'snapshots' && found.snapshots}
              <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.reviewWith(() => api.prepareSnapshotCheck())}>
                <Icon name="eye" size={14} /> Check snapshots…
              </button>
            {/if}
            {#if entry}
              <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.review([entry.id], false)}>
                <Icon name="play" size={14} /> Set it up with {entry.name}…
              </button>
            {/if}
          </div>
        {/if}
      </section>
    {/each}
  {/if}
</div>

<style>
  .screen {
    overflow-y: auto;
  }

  .part {
    padding: 14px 18px;
  }

  .part p {
    margin: 6px 0;
  }

  .head {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .head h2 {
    margin: 0;
  }

  .evidence {
    margin: 4px 0;
    padding-left: 18px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }

  .bad {
    color: var(--red);
  }
</style>
