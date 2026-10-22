<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { DriftView, TargetView } from '../lib/types';

  let targets = $state<TargetView[]>([]);
  let key = $state<string>('');
  let view = $state<DriftView | null>(null);
  let problem = $state<string | null>(null);
  let comparing = $state(false);

  const target = $derived(targets.find((candidate) => candidate.key === key));
  const counts = $derived.by(() => {
    const out = new Map<string, number>();
    for (const difference of view?.report.differences ?? []) {
      out.set(difference.kind, (out.get(difference.kind) ?? 0) + 1);
    }
    return [...out.entries()];
  });
  const restoreNames = $derived((view?.restore ?? []).map((id) => toolbox.entry(id)?.name ?? id));

  async function loadTargets() {
    try {
      targets = await api.driftTargets();
      if (!targets.some((candidate) => candidate.key === key)) key = targets[0]?.key ?? '';
      problem = null;
    } catch (error) {
      problem = String(error);
    }
    await compare();
  }

  async function compare() {
    if (!key) {
      view = null;
      return;
    }
    comparing = true;
    try {
      view = await api.driftCheck(key);
      problem = null;
    } catch (error) {
      view = null;
      problem = String(error);
    } finally {
      comparing = false;
    }
  }

  async function saveBaseline() {
    const name = await toolbox.ask('Save a baseline', 'A name for this machine as it is now: what is installed, the configuration files entries write, the login shell, the display manager and what ran.', 'Before the upgrade');
    if (!name) return;
    if (await toolbox.tell('Baseline saved', () => api.saveBaseline(name))) {
      await loadTargets();
    }
  }

  loadTargets();

  const KIND_LABELS: Record<string, string> = {
    MissingEntry: 'Not installed here',
    ExtraEntry: 'Installed here only',
    VersionChanged: 'Different version',
    FileChanged: 'File differs',
    FileMissing: 'File missing',
    SettingChanged: 'Setting differs',
    NotApplied: 'Not applied here',
  };
  const label = (kind: string) => KIND_LABELS[kind] ?? kind.replace(/([a-z])([A-Z])/g, '$1 $2');
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Drift</h1>
      <p class="screen-subtitle">How this machine differs from a saved baseline or a profile.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" onclick={saveBaseline} disabled={toolbox.checking}>
        <Icon name="save" size={16} /> Save this machine as a baseline
      </button>
      <button class="button" disabled={comparing || !key} onclick={compare}>
        <Icon name="refresh" size={16} /> {comparing ? 'Comparing…' : 'Compare again'}
      </button>
    </div>
  </div>

  <div class="card chooser">
    <label>
      <span class="faint">Compare with</span>
      <select bind:value={key} onchange={compare} disabled={targets.length === 0}>
        {#each targets as candidate (candidate.key)}
          <option value={candidate.key}>{candidate.kind === 'baseline' ? 'Baseline' : 'Profile'}: {candidate.name}</option>
        {/each}
      </select>
    </label>
    {#if target}<span class="muted detail">{target.detail}</span>{/if}
  </div>

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if targets.length === 0}
        <p class="empty-note">No baseline saved yet. Save this machine as one to compare with it later, or with another machine.</p>
      {:else if view === null}
        <p class="empty-note">{comparing ? 'Comparing…' : 'Choose what to compare with.'}</p>
      {:else}
        <div class="summary">
          <span class="pill good">{view.report.matched} matched</span>
          {#each counts as [kind, count] (kind)}
            <span class="pill notice">{count} {label(kind).toLowerCase()}</span>
          {/each}
          {#if view.report.differences.length === 0}<span class="muted">Nothing differs.</span>{/if}
        </div>
        {#if view.report.unknown.length > 0}
          <p class="empty-note">Not compared: {view.report.unknown.join(', ')}</p>
        {/if}
        <table>
          <thead><tr><th>What</th><th>Subject</th><th>Was</th><th>Now</th></tr></thead>
          <tbody>
            {#each view.report.differences as difference, index (index)}
              <tr>
                <td class="kind">{label(difference.kind)}</td>
                <td class="mono subject" title={difference.note}>{difference.subject}</td>
                <td class="muted">{difference.was ?? '–'}</td>
                <td class="muted">{difference.now ?? '–'}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>

    <aside class="pane side card">
      <h2>Put it back</h2>
      {#if view && view.restore.length > 0}
        <p class="muted">These entries would restore what is missing or changed:</p>
        <ul>
          {#each restoreNames as name, index (index)}<li>{name}</li>{/each}
        </ul>
        <div class="actions">
          <button class="button" onclick={() => toolbox.enqueue(view!.restore)}>
            <Icon name="plus" size={14} /> Add to plan
          </button>
          <button class="button primary" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.review(view!.restore, false)}>
            <Icon name="play" size={14} /> Run now
          </button>
        </div>
      {:else if view}
        <p class="muted">Nothing to put back.</p>
      {:else}
        <p class="faint">A comparison lists here what would restore the baseline.</p>
      {/if}
      {#if view && view.unavailable.length > 0}
        <h3>Not on this machine</h3>
        <ul>
          {#each view.unavailable as id (id)}<li>{toolbox.entry(id)?.name ?? id}</li>{/each}
        </ul>
      {/if}
    </aside>
  </div>
</div>

<style>
  .chooser {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 14px;
  }

  .chooser label {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  select {
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .detail {
    font-size: 13px;
  }

  .summary {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    position: sticky;
    top: 0;
    padding: 8px 16px;
    background: var(--panel);
    color: var(--faint);
    font-size: 11px;
    letter-spacing: 0.1em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 7px 16px;
    border-bottom: 1px solid var(--line);
    font-size: 13px;
  }

  .kind {
    white-space: nowrap;
    color: var(--yellow);
  }

  .subject {
    word-break: break-all;
  }

  .bad {
    color: var(--red);
  }
</style>
