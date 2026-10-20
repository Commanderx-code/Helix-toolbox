<script lang="ts">
  // The change preview of the plan being confirmed, as the terminal
  // interface's: each step's expected changes, marked by how they are known,
  // and a file now and afterward side by side.
  import Icon from './Icon.svelte';
  import * as api from '../lib/api';
  import type { Certainty, ChangesView } from '../lib/types';

  let { onback }: { onback: () => void } = $props();

  let changes = $state<ChangesView | null>(null);
  let error = $state('');
  let downloading = $state(false);
  /** The changes whose file is shown side by side, by step and change. */
  let open = $state<Record<string, boolean>>({});

  const MARKS: Record<Certainty, string> = { known: '✓', expected: '~', unknown: '?' };
  const LABELS: Record<Certainty, string> = { known: 'known', expected: 'expected', unknown: 'not known in advance' };

  async function load(read: () => Promise<ChangesView>) {
    error = '';
    try {
      changes = await read();
    } catch (problem) {
      error = String(problem);
    }
  }

  load(api.changes);

  async function download() {
    downloading = true;
    await load(api.downloadSources);
    downloading = false;
  }
</script>

<div class="legend">
  {#each ['known', 'expected', 'unknown'] as const as certainty (certainty)}
    <span class={certainty}><b>{MARKS[certainty]}</b> {LABELS[certainty]}</span>
  {/each}
</div>

{#if error}
  <p class="error" role="alert"><Icon name="warning" size={15} /><span>{error}</span></p>
{/if}

<div class="body">
  {#if changes}
    {#if changes.download}
      <div class="download">
        <Icon name="info" size={15} />
        <span>
          A pinned source this plan reads is not downloaded yet. Downloading it changes nothing else, and lets the
          files it writes be compared.
        </span>
        <button class="button" disabled={downloading} onclick={download}>
          {downloading ? 'Downloading…' : 'Download the source'}
        </button>
      </div>
    {/if}
    {#each changes.steps as step, stepIndex (stepIndex)}
      <section>
        <h3>{step.name}</h3>
        {#if step.changes.length === 0}
          <p class="faint">No changes listed.</p>
        {/if}
        <ul>
          {#each step.changes as change, changeIndex (changeIndex)}
            {@const key = `${stepIndex}:${changeIndex}`}
            <li class="change">
              <span class="mark {change.certainty}" title={LABELS[change.certainty]}>{MARKS[change.certainty]}</span>
              <span class="text">{change.text}</span>
              {#if change.diff}
                <button class="button ghost compare" aria-expanded={!!open[key]} onclick={() => (open[key] = !open[key])}>
                  {open[key] ? 'Hide' : 'Side by side'}
                </button>
              {/if}
            </li>
            {#if change.diff && open[key]}
              <li class="diff mono">
                <div class="head">Now: {change.diff.path}</div>
                <div class="head">After</div>
                {#if !change.diff.changed}
                  <div class="whole faint">Nothing in this file changes.</div>
                {:else}
                  {#each change.diff.rows as row, rowIndex (rowIndex)}
                    {#if row}
                      <div class="cell {row.tag} before">{row.before ?? ''}</div>
                      <div class="cell {row.tag} after">{row.after ?? ''}</div>
                    {:else}
                      <div class="whole gap">⋯</div>
                    {/if}
                  {/each}
                {/if}
              </li>
            {/if}
          {/each}
        </ul>
      </section>
    {/each}
  {:else if !error}
    <p class="faint">Working out what this plan changes…</p>
  {/if}
</div>

<div class="buttons">
  <button class="button" onclick={onback}>Back to the confirmation</button>
</div>

<style>
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 18px;
    margin-bottom: 12px;
    font-size: 13px;
  }

  .known {
    color: var(--green);
  }

  .expected {
    color: var(--yellow);
  }

  .unknown {
    color: var(--muted);
  }

  .error,
  .download {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0 0 12px;
  }

  .error {
    color: var(--red);
  }

  .download {
    align-items: center;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    color: var(--muted);
  }

  .download span {
    flex: 1;
  }

  .error :global(svg),
  .download :global(svg) {
    flex-shrink: 0;
  }

  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 14px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: var(--bg);
  }

  h3 {
    margin: 12px 0 6px;
    font-size: 14px;
  }

  ul {
    margin: 0 0 8px;
    padding: 0;
    list-style: none;
  }

  .change {
    display: flex;
    gap: 10px;
    align-items: baseline;
    padding: 3px 0;
  }

  .mark {
    flex: 0 0 14px;
    font-family: var(--mono);
    font-weight: 700;
  }

  .text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .compare {
    height: 26px;
    padding: 0 10px;
    color: var(--cyan);
  }

  .diff {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    margin: 4px 0 10px 24px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    font-size: 12px;
    overflow: hidden;
  }

  .head {
    padding: 6px 10px;
    font-weight: 700;
    background: var(--panel-raised);
    border-bottom: 1px solid var(--line);
    overflow-wrap: anywhere;
  }

  .cell {
    padding: 1px 10px;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: var(--muted);
  }

  .before {
    border-right: 1px solid var(--line);
  }

  .head:first-child {
    border-right: 1px solid var(--line);
  }

  .cell.changed {
    color: var(--yellow);
    background: color-mix(in srgb, var(--yellow) 9%, transparent);
  }

  .cell.removed.before {
    color: var(--red);
    background: color-mix(in srgb, var(--red) 11%, transparent);
  }

  .cell.added.after {
    color: var(--green);
    background: color-mix(in srgb, var(--green) 11%, transparent);
  }

  .whole {
    grid-column: 1 / -1;
    padding: 6px 10px;
  }

  .gap {
    padding: 0 10px;
    color: var(--faint);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }
</style>
