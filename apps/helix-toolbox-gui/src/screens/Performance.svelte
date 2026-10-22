<script lang="ts">
  // The performance and boot analyzer, as `helix-toolbox performance`: how
  // long this boot took and where the time went, the machine now, and this
  // boot beside an earlier one with the runs in between. It only reads;
  // a hint's change is a plan, reviewed first.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { Boot, BootChange, Comparison, PerformanceView } from '../lib/types';

  let found = $state<PerformanceView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  /** The earlier boot this one is set beside; the one before when null. */
  let against = $state<string | null>(null);
  let chosen = $state<Comparison | null>(null);

  const boot = $derived(found?.found.boot ?? null);
  const earlier = $derived(boot ? (found?.boots ?? []).filter((other) => other.id !== boot.id && other.booted <= boot.booted) : []);
  const compared = $derived(against === null ? (found?.found.previous ?? null) : chosen);
  const now = $derived(found?.found.now ?? null);

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
    void toolbox.runs;
    load();
  });

  async function setAgainst(id: string) {
    const previous = found?.found.previous?.before.id ?? null;
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

  /** A span in milliseconds, as the command line writes it. */
  function span(ms: number): string {
    if (ms < 1000) return `${ms} ms`;
    if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
    const seconds = Math.round(ms / 1000);
    return `${Math.floor(seconds / 60)} min ${seconds % 60} s`;
  }

  function signed(ms: number): string {
    return `${ms < 0 ? '-' : '+'}${span(Math.abs(ms))}`;
  }

  function bytes(value: number): string {
    for (const [unit, scale] of [
      ['TB', 1e12],
      ['GB', 1e9],
      ['MB', 1e6],
      ['KB', 1e3],
    ] as const) {
      const scaled = value / scale;
      if (scaled >= 1) return `${scaled >= 100 ? scaled.toFixed(0) : scaled.toFixed(1)} ${unit}`;
    }
    return `${value} B`;
  }

  function when(seconds: number): string {
    return new Date(seconds * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  }

  function parts(of: Boot): { name: string; took: number; tone: string }[] {
    const phases = of.phases;
    return [
      { name: 'Firmware', took: phases.firmware, tone: 'firmware' },
      { name: 'Boot loader', took: phases.loader, tone: 'loader' },
      { name: 'Kernel', took: phases.kernel, tone: 'kernel' },
      { name: 'Initial RAM disk', took: phases.initrd, tone: 'initrd' },
      { name: 'Services', took: phases.userspace, tone: 'services' },
    ].filter((part): part is { name: string; took: number; tone: string } => part.took !== null);
  }

  /** The changes worth a line: a tenth of a second or more, or new. */
  function changes(list: BootChange[]): BootChange[] {
    return list.filter((change) => change.before === null || change.after === null || Math.abs(change.after - change.before) >= 100);
  }

  function difference(change: BootChange): string {
    if (change.before !== null && change.after !== null) return signed(change.after - change.before);
    return change.before === null ? 'new' : 'gone';
  }

  const slower = (change: BootChange) => change.before === null || (change.after !== null && change.after > change.before);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
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
        <p class="total"><strong>{span(boot.phases.total)}</strong> <span class="muted">on {when(boot.booted)}, kernel {boot.kernel}</span></p>
        <div class="phases" role="img" aria-label={parts(boot).map((part) => `${part.name} ${span(part.took)}`).join(', ')}>
          {#each parts(boot) as part (part.name)}
            <span class="phase {part.tone}" style="flex-grow: {Math.max(part.took, 1)}" title="{part.name} {span(part.took)}"></span>
          {/each}
        </div>
        <ul class="legend">
          {#each parts(boot) as part (part.name)}
            <li><span class="swatch {part.tone}"></span>{part.name} <span class="muted">{span(part.took)}</span></li>
          {/each}
        </ul>
      {:else}
        <p class="muted">{found.found.no_boot}</p>
      {/if}
    </section>

    {#if boot}
      <section class="card part">
        <div class="part-top">
          <h2>Compared with an earlier boot</h2>
          {#if earlier.length > 1}
            <select aria-label="The earlier boot" value={against ?? earlier[0].id} onchange={(event) => setAgainst((event.currentTarget as HTMLSelectElement).value)}>
              {#each earlier as other (other.id)}
                <option value={other.id}>{when(other.booted)} · {span(other.phases.total)}</option>
              {/each}
            </select>
          {/if}
        </div>
        {#if compared}
          <p class="total">
            <strong class:slower={compared.after.phases.total > compared.before.phases.total} class:faster={compared.after.phases.total < compared.before.phases.total}>
              {signed(compared.after.phases.total - compared.before.phases.total)}
            </strong>
            <span class="muted">against {when(compared.before.booted)}, {span(compared.before.phases.total)}</span>
          </p>
          {@const listed = changes([...compared.phases.slice(1), ...compared.units])}
          {#if listed.length > 0}
            <ul class="changes">
              {#each listed as change (change.name)}
                <li>
                  <span class="mono name">{change.name}</span>
                  <span class="muted">{change.after === null ? 'not now' : span(change.after)}</span>
                  <span class:slower={slower(change)} class:faster={!slower(change)}>{difference(change)}</span>
                </li>
              {/each}
            </ul>
          {/if}
          {#if compared.before.kernel && compared.before.kernel !== compared.after.kernel}
            <p>The kernel changed: <span class="mono">{compared.before.kernel}</span> to <span class="mono">{compared.after.kernel}</span>.</p>
          {/if}
          {#if compared.runs.length === 0}
            <p class="faint">No Helix Toolbox runs in between.</p>
          {:else}
            <h3>Helix Toolbox ran in between</h3>
            <ul class="runs">
              {#each compared.runs as run (run.at)}
                <li><span class="muted">{when(run.at)}</span> {run.names.join(', ')}{run.success ? '' : ' (failed)'}</li>
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
            {#each boot.chain.filter((link) => link.took !== null) as link (link.name)}
              <li><span class="mono name">{link.name}</span><span class="muted">{span(link.took ?? 0)}</span></li>
            {:else}
              <li class="faint">systemd did not say.</li>
            {/each}
          </ul>
        </section>
        <section class="card part">
          <h2>Slowest services since the boot</h2>
          <ul class="units">
            {#each boot.slowest as timed (timed.name)}
              <li><span class="mono name">{timed.name}</span><span class="muted">{span(timed.took)}</span></li>
            {:else}
              <li class="faint">systemd did not say.</li>
            {/each}
          </ul>
        </section>
      </div>
    {/if}

    {#if now}
      <section class="card part">
        <h2>Now</h2>
        <dl class="kv">
          {#if now.load}
            <dt>Load</dt>
            <dd>{now.load.map((value) => value.toFixed(2)).join(' ')} <span class="muted">(1, 5 and 15 minutes) on {now.cpus} processors</span></dd>
          {/if}
          {#if now.memory_total > 0}
            {@const used = 1 - now.memory_available / now.memory_total}
            <dt>Memory</dt>
            <dd class="meter-line">
              <span class="bar" role="meter" aria-label="Memory used" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(used * 100)}>
                <span class="fill" class:full={used > 0.9} style="width: {Math.round(used * 100)}%"></span>
              </span>
              {bytes(now.memory_available)} free of {bytes(now.memory_total)}
            </dd>
          {/if}
          <dt>Swap</dt>
          <dd>
            {#if now.swaps.length === 0}
              none
            {:else}
              {bytes(now.swaps.reduce((sum, swap) => sum + swap.used, 0))} used of {bytes(now.swaps.reduce((sum, swap) => sum + swap.size, 0))}{now.swaps.some((swap) => swap.zram) ? ' (zram)' : ''}
            {/if}
          </dd>
          {#if now.pressure.cpu !== null || now.pressure.memory !== null || now.pressure.io !== null}
            <dt>Waiting, last minute</dt>
            <dd>
              {[
                ['processor', now.pressure.cpu],
                ['memory', now.pressure.memory],
                ['disk', now.pressure.io],
              ]
                .filter(([, value]) => value !== null)
                .map(([what, value]) => `${what} ${Math.round(value as number)}%`)
                .join(', ')}
            </dd>
          {/if}
          {#if now.governor || now.power_profile}
            <dt>Processor</dt>
            <dd>{[now.governor && `governor ${now.governor}`, now.power_profile && `power profile ${now.power_profile}`].filter(Boolean).join(', ')}</dd>
          {/if}
          {#if now.heaviest.length > 0}
            <dt>Most memory</dt>
            <dd>{now.heaviest.slice(0, 5).map((process) => `${process.name} ${bytes(process.memory)}`).join(', ')}</dd>
          {/if}
        </dl>
      </section>
    {/if}

    {#if found.found.hints.length > 0}
      <section class="card part">
        <h2>Worth knowing</h2>
        <ul class="hints">
          {#each found.found.hints as hint (hint.title)}
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
