<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { RecordView } from '../lib/types';

  let records = $state<RecordView[] | null>(null);
  let problem = $state<string | null>(null);

  async function load() {
    try {
      records = await api.runs();
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  // Read on opening, and again when a run is recorded.
  $effect(() => {
    void toolbox.readAgain.history;
    load();
  });
</script>

<div class="screen">
  <div class="top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">History</h1>
      <p class="screen-subtitle">Past runs, newest first, from every Helix Toolbox interface.</p>
    </div>
    <button class="button" onclick={load}><Icon name="refresh" size={16} /> Refresh</button>
  </div>

  <section class="card list">
    {#if problem}
      <p class="problem">{problem}</p>
    {:else if records === null}
      <p class="faint none">Reading the history…</p>
    {:else}
      <table>
        <thead>
          <tr><th>When</th><th>Entries</th><th>Result</th><th>Report</th></tr>
        </thead>
        <tbody>
          {#each records as record, index (index)}
            <tr>
              <td class="mono when">{record.date}</td>
              <td>{record.names.join(', ')}</td>
              <td>
                <span class="result" class:success={record.success}>
                  <Icon name={record.success ? 'check' : 'x'} size={14} />
                  {record.result}
                </span>
              </td>
              <td class="report">
                {#if record.success}
                  <button class="button ghost" title="Report that this run worked on this machine" onclick={() => toolbox.reportWorked(record.unix_timestamp, null)}>
                    Report it worked…
                  </button>
                {/if}
              </td>
            </tr>
          {:else}
            <tr><td colspan="4" class="faint">Nothing has run yet.</td></tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
</div>

<style>
  .screen {
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    padding: 20px 24px;
  }

  .top {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
  }

  .list {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
  }

  th {
    position: sticky;
    top: 0;
    padding: 10px 16px;
    background: var(--panel);
    border-bottom: 1px solid var(--line);
    color: var(--faint);
    font-size: 11px;
    letter-spacing: 0.1em;
    text-align: left;
    text-transform: uppercase;
  }

  td {
    padding: 9px 16px;
    border-bottom: 1px solid var(--line);
  }

  .when {
    width: 170px;
    color: var(--muted);
    font-size: 13px;
  }

  .result {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--red);
  }

  .result.success {
    color: var(--green);
  }

  .report {
    width: 1%;
    padding-top: 4px;
    padding-bottom: 4px;
    text-align: right;
    white-space: nowrap;
  }

  .problem {
    padding: 16px;
    color: var(--red);
  }

  .none {
    padding: 16px;
  }
</style>
