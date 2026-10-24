<script lang="ts">
  // The performance and boot analyzer, as `helix-toolbox performance`: how
  // long this boot took and where the time went, the machine now, and this
  // boot beside an earlier one with the runs in between. It only reads;
  // a hint's change is a plan, reviewed first.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { Comparison, PerformanceView } from '../lib/types';

  let found = $state<PerformanceView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  /** The earlier boot this one is set beside; the one before when null. */
  let against = $state<string | null>(null);
  let chosen = $state<Comparison | null>(null);

  const boot = $derived(found?.boot ?? null);
  const earlier = $derived(found?.earlier ?? []);
  const compared = $derived(against === null ? (found?.previous ?? null) : chosen);

  async function load() {
    reading = true;
    try {
      found = await api.performance();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  $effect(() => {
    void toolbox.readAgain.tools;
    load();
  });

  async function setAgainst(id: string) {
    const previous = found?.previous?.before.id ?? null;
    if (!boot || id === previous) {
      against = null;
      chosen = null;
      return;
    }
    try {
      chosen = await api.compareBoots(id, boot.id);
      against = id;
    } catch (error) {
      problem = String(error);
    }
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  }
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Performance</h1>
      <p class="screen-subtitle">How long the boot took and where the time went, and the machine now. It only reads; each boot is recorded to compare with later ones.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  {#if problem}
    <p class="card empty-note bad">{problem}</p>
  {:else if found === null}
    <p class="card empty-note">Reading the boot and the machine…</p>
  {:else}
    <section class="card part">
      <h2>This boot</h2>
      {#if boot}
        <p class="total"><strong>{boot.total}</strong> <span class="muted">on {when(boot.booted)}, kernel {boot.kernel}</span></p>
        <div class="phases" role="img" aria-label={boot.parts.map((part) => `${part.name} ${part.shown}`).join(', ')}>
          {#each boot.parts as part (part.name)}
            <span class="phase {part.tone}" style="flex-grow: {Math.max(part.took, 1)}" title="{part.name} {part.shown}"></span>
          {/each}
        </div>
        <ul class="legend">
          {#each boot.parts as part (part.name)}
            <li><span class="swatch {part.tone}"></span>{part.name} <span class="muted">{part.shown}</span></li>
          {/each}
        </ul>
      {:else}
        <p class="muted">{found.no_boot}</p>
      {/if}
    </section>

    {#if boot}
      <section class="card part">
        <div class="part-top">
          <h2>Compared with an earlier boot</h2>
          {#if earlier.length > 1}
            <select aria-label="The earlier boot" value={against ?? earlier[0].id} onchange={(event) => setAgainst((event.currentTarget as HTMLSelectElement).value)}>
              {#each earlier as other (other.id)}
                <option value={other.id}>{when(other.booted)} · {other.total}</option>
              {/each}
            </select>
          {/if}
        </div>
        {#if compared}
          <p class="total">
            <strong class:slower={compared.sign > 0} class:faster={compared.sign < 0}>
              {compared.total}
            </strong>
            <span class="muted">against {when(compared.before.booted)}, {compared.before.total}</span>
          </p>
          {#if compared.changes.length > 0}
            <ul class="changes">
              {#each compared.changes as change (change.name)}
                <li>
                  <span class="mono name">{change.name}</span>
                  <span class="muted">{change.now}</span>
                  <span class:slower={change.slower} class:faster={!change.slower}>{change.how}</span>
                </li>
              {/each}
            </ul>
          {/if}
          {#if compared.kernel}
            <p>The kernel changed: <span class="mono">{compared.kernel[0]}</span> to <span class="mono">{compared.kernel[1]}</span>.</p>
          {/if}
          {#if compared.runs.length === 0}
            <p class="faint">No Helix Toolbox runs in between.</p>
          {:else}
            <h3>Helix Toolbox ran in between</h3>
            <ul class="runs">
              {#each compared.runs as run (run.at)}
                <li><span class="muted">{when(run.at)}</span> {run.ran}</li>
              {/each}
            </ul>
          {/if}
        {:else}
          <p class="faint">No earlier boot is recorded yet; after the next boot, this one is compared with it.</p>
        {/if}
      </section>

      <div class="pair">
        <section class="card part">
          <h2>The login screen waited for</h2>
          <ul class="units">
            {#each boot.waited as link (link.name)}
              <li><span class="mono name">{link.name}</span><span class="muted">{link.took}</span></li>
            {:else}
              <li class="faint">systemd did not say.</li>
            {/each}
          </ul>
        </section>
        <section class="card part">
          <h2>Slowest services since the boot</h2>
          <ul class="units">
            {#each boot.slowest as timed (timed.name)}
              <li><span class="mono name">{timed.name}</span><span class="muted">{timed.took}</span></li>
            {:else}
              <li class="faint">systemd did not say.</li>
            {/each}
          </ul>
        </section>
      </div>
    {/if}

    {#if found.now.length > 0}
      <section class="card part">
        <h2>Now</h2>
        <dl class="kv">
          {#each found.now as fact (fact.term)}
            <dt>{fact.term}</dt>
            {#if fact.meter}
              <dd class="meter-line">
                <span class="bar" role="meter" aria-label="{fact.term} used" aria-valuemin={0} aria-valuemax={100} aria-valuenow={fact.meter.percent}>
                  <span class="fill" class:full={fact.meter.full} style="width: {fact.meter.percent}%"></span>
                </span>
                {fact.text}
              </dd>
            {:else}
              <dd>{fact.text}{#if fact.note}{' '}<span class="muted">{fact.note}</span>{/if}</dd>
            {/if}
          {/each}
        </dl>
      </section>
    {/if}

    {#if found.hints.length > 0}
      <section class="card part">
        <h2>Worth knowing</h2>
        <ul class="hints">
          {#each found.hints as hint (hint.title)}
            <li>
              <span class="hint-title"><Icon name="warning" size={14} /> {hint.title}</span>
              <span class="muted">{hint.detail}</span>
              {#if hint.unit && hint.command?.includes(' disable ')}
                <span class="actions">
                  <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.reviewWith(() => api.prepareService(hint.unit!, false, 'disable'))}>
                    <Icon name="x" size={14} /> Turn it off…
                  </button>
                </span>
              {:else if hint.command}
                <span class="mono small">{hint.command}</span>
              {/if}
              {#if hint.entry && toolbox.entry(hint.entry)}
                <span class="actions">
                  <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.review([hint.entry!], false)}>
                    <Icon name="play" size={14} /> Set it up…
                  </button>
                </span>
              {/if}
            </li>
          {/each}
        </ul>
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

  .part h3 {
    margin: 12px 0 6px;
    font-size: 0.95em;
  }

  .part-top {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: baseline;
    justify-content: space-between;
  }

  .part-top select {
    height: 30px;
    padding: 0 8px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .total {
    margin: 0 0 10px;
  }

  .total strong {
    font-size: 1.3em;
  }

  .phases {
    display: flex;
    height: 12px;
    border-radius: 6px;
    overflow: hidden;
    background: var(--line);
  }

  .phase,
  .swatch {
    display: block;
    min-width: 3px;
  }

  .firmware {
    background: var(--muted);
  }

  .loader {
    background: var(--yellow);
  }

  .kernel {
    background: var(--accent);
  }

  .initrd {
    background: var(--green);
  }

  .services {
    background: var(--text);
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    margin: 10px 0 0;
    padding: 0;
    list-style: none;
  }

  .legend li {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }

  .changes,
  .units,
  .runs,
  .hints {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .changes li,
  .units li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto auto;
    gap: 14px;
    padding: 5px 0;
    border-top: 1px solid var(--line);
    font-variant-numeric: tabular-nums;
  }

  .units li {
    grid-template-columns: minmax(0, 1fr) auto;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .runs li {
    padding: 3px 0;
  }

  .slower {
    color: var(--yellow);
  }

  .faster {
    color: var(--green);
  }

  .pair {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 14px;
  }

  .meter-line {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .bar {
    width: 160px;
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

  .hints li {
    display: grid;
    gap: 4px;
    padding: 10px 0;
    border-top: 1px solid var(--line);
  }

  .hint-title {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--yellow);
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .small {
    font-size: 0.9em;
  }

  .bad {
    color: var(--red);
  }
</style>
