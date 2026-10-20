<script lang="ts">
  // The setup wizard: what this machine is, a choice of workstation, then
  // the entries it suggests, ticked where they are not installed. It ends
  // by putting the ticked entries in the Install Plan; it runs nothing.
  import { modal } from '../lib/keys';
  import Icon from './Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { PickView, SetupView } from '../lib/types';

  type Page = 'machine' | 'choose' | 'pick';

  let page = $state<Page>('machine');
  let setup = $state<SetupView | null>(null);
  let problem = $state<string | null>(null);
  let chosen = $state<string | null>(null);
  let picks = $state<PickView[]>([]);
  let loading = $state(false);
  const firstRun = toolbox.about?.first_run ?? false;

  const chosenName = $derived(setup?.workstations.find((known) => known.id === chosen)?.name ?? '');
  const ticked = $derived(picks.filter((pick) => pick.ticked));

  async function start() {
    loading = true;
    try {
      setup = await api.setupStart();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      loading = false;
    }
  }

  start();

  async function choose(id: string) {
    chosen = id;
    loading = true;
    try {
      picks = await api.setupChoose(id);
      problem = null;
      page = 'pick';
    } catch (error) {
      problem = String(error);
    } finally {
      loading = false;
    }
  }

  /** Done with the wizard, whatever was chosen: it no longer opens by itself. */
  async function close() {
    toolbox.wizard = false;
    if (firstRun) {
      if (toolbox.about) toolbox.about.first_run = false;
      try {
        await api.setupDone();
      } catch (error) {
        toolbox.error = String(error);
      }
    }
  }

  /** Put the ticked entries in the Install Plan and show it. */
  async function queue() {
    const added: string[] = [];
    const alone: string[] = [];
    for (const pick of ticked) {
      const entry = toolbox.entry(pick.id);
      if (!entry) continue;
      if (!entry.queueable) alone.push(entry.name);
      else if (toolbox.enqueue([pick.id]) > 0) added.push(entry.name);
    }
    await close();
    toolbox.open({ kind: 'plan' });
    let text =
      added.length === 0
        ? 'Nothing was added to the Install Plan.'
        : `Added to the Install Plan: ${added.join(', ')}.\n\nNothing has run. Review the plan, and Review and run runs it.`;
    if (alone.length > 0) text += `\n\nThese run on their own; run each from its area: ${alone.join(', ')}.`;
    toolbox.notice = { title: 'Install Plan', text };
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape' && !toolbox.prompt && !toolbox.notice) close();
  }}
/>

<div class="backdrop" role="presentation">
  <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="wizard-title" data-page={page} use:modal>
    <div class="top">
      <span class="pill">Set up this machine</span>
      <span class="faint">{page === 'machine' ? '1 of 3' : page === 'choose' ? '2 of 3' : '3 of 3'}</span>
    </div>

    {#if page === 'machine'}
      <h2 id="wizard-title">{firstRun ? 'Welcome to Helix Toolbox' : 'This machine'}</h2>
      <p class="muted">
        {firstRun
          ? 'A look at this machine, then a workstation to choose from. The wizard only fills the Install Plan for you to review; it runs nothing by itself, and it can be opened again from Settings.'
          : 'What the wizard sees. Next, choose a workstation; its entries for this machine go to the Install Plan for review.'}
      </p>
      {#if problem}
        <p class="bad">{problem}</p>
      {:else if setup}
        <dl class="kv">
          {#each setup.machine as [label, value] (label)}
            <dt>{label}</dt>
            <dd>{value}</dd>
          {/each}
        </dl>
      {:else}
        <p class="faint">Looking at the machine…</p>
      {/if}
      <div class="buttons">
        <button class="button" onclick={close}>{firstRun ? 'Not now' : 'Close'}</button>
        <button class="button primary" disabled={!setup} onclick={() => (page = 'choose')}>Choose a workstation <Icon name="chevron" size={14} /></button>
      </div>
    {:else if page === 'choose'}
      <h2 id="wizard-title">Choose a workstation</h2>
      <p class="muted">Each is a set of entries; the next page shows which of them this machine can run, and lets you untick any.</p>
      <div class="choices">
        {#each setup?.workstations ?? [] as known (known.id)}
          <button class="choice" disabled={loading} onclick={() => choose(known.id)}>
            <span class="name">{known.name}</span>
            <span class="sub">{known.description}</span>
          </button>
        {/each}
      </div>
      {#if problem}<p class="bad">{problem}</p>{/if}
      <div class="buttons">
        <button class="button" onclick={() => (page = 'machine')}>Back</button>
        <button class="button" onclick={close}>Close</button>
      </div>
    {:else}
      <h2 id="wizard-title">{chosenName}</h2>
      <p class="muted">Ticked entries go to the Install Plan. Greyed ones cannot run here; those that run on their own are started from their area.</p>
      <div class="picks">
        {#each picks as pick (pick.id)}
          <label class="pick" class:unavailable={!pick.available}>
            <input type="checkbox" bind:checked={pick.ticked} disabled={!pick.available} />
            <span>
              <span class="name">{pick.name}</span>
              {#if pick.note}<span class="sub">{pick.note}</span>{/if}
            </span>
          </label>
        {/each}
        {#if picks.length === 0}
          <p class="faint">This workstation has nothing for this machine.</p>
        {/if}
      </div>
      <div class="buttons">
        <button class="button" onclick={() => (page = 'choose')}>Back</button>
        <button class="button" onclick={close}>Close</button>
        <button class="button primary" disabled={ticked.length === 0} onclick={queue}>
          <Icon name="plan" size={15} /> Add {ticked.length} to the Install Plan
        </button>
      </div>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--backdrop);
    z-index: 40;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(760px, calc(100vw - 80px));
    max-height: calc(100vh - 80px);
    padding: 22px 24px;
    border-color: var(--selected-line);
    box-shadow: 0 24px 60px var(--shadow);
  }

  .top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }

  h2 {
    margin: 0 0 8px;
    font-size: 20px;
    color: var(--purple);
  }

  .muted {
    margin: 0 0 14px;
    color: var(--muted);
  }

  .choices,
  .picks {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
    overflow-y: auto;
  }

  .choice,
  .pick {
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }

  .choice {
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
  }

  .choice:hover,
  .pick:hover {
    border-color: var(--cyan);
  }

  .pick > span {
    display: grid;
  }

  .pick.unavailable {
    color: var(--faint);
    cursor: default;
  }

  .name {
    font-weight: 600;
  }

  .sub {
    color: var(--muted);
    font-size: 13px;
  }

  .bad {
    color: var(--red);
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }
</style>
