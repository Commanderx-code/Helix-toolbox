<script lang="ts">
  // The service and startup manager, as `helix-toolbox services` and
  // `startup`: systemd's units of the machine and the session, searchable,
  // and the applications started at login. Each change is a plan, reviewed
  // first, and what the machine needs says what stopping it costs.
  import { arrows } from '../lib/keys';
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { ServiceDetails, ServicesView, UnitView } from '../lib/types';

  type View = 'system' | 'timers' | 'user' | 'login' | 'failed';
  const VIEWS: { view: View; label: string }[] = [
    { view: 'system', label: 'Services' },
    { view: 'timers', label: 'Timers and sockets' },
    { view: 'user', label: 'Your session' },
    { view: 'login', label: 'At login' },
    { view: 'failed', label: 'Failed' },
  ];

  let found = $state<ServicesView | null>(null);
  let problem = $state<string | null>(null);
  let view = $state<View>('system');
  let filter = $state('');
  let selected = $state<string | null>(null);
  let details = $state<ServiceDetails | null>(null);

  const key = (unit: UnitView) => `${unit.user ? 'user' : 'system'}:${unit.name}`;
  const matches = (...texts: string[]) => {
    const words = filter.toLowerCase().split(/\s+/).filter(Boolean);
    return words.every((word) => texts.some((text) => text.toLowerCase().includes(word)));
  };
  const units = $derived(
    (found?.units ?? []).filter(
      (unit) =>
        (view === 'system'
          ? !unit.user && unit.kind === 'service'
          : view === 'timers'
            ? unit.kind !== 'service'
            : view === 'user'
              ? unit.user && unit.kind === 'service'
              : view === 'failed'
                ? unit.failed
                : false) && matches(unit.name, unit.description),
    ),
  );
  const applications = $derived(view === 'login' ? (found?.autostart ?? []).filter((item) => matches(item.name, item.file, item.exec)) : []);
  const unit = $derived(found?.units.find((unit) => key(unit) === selected) ?? null);
  const application = $derived(found?.autostart.find((item) => `login:${item.file}` === selected) ?? null);

  async function load() {
    try {
      found = await api.services();
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  $effect(() => {
    void toolbox.runs;
    load();
  });

  $effect(() => {
    const chosen = unit;
    details = null;
    if (chosen) {
      api.serviceDetails(chosen.name, chosen.user).then((answer) => {
        if (unit === chosen) details = answer;
      });
    }
  });

  function change(chosen: UnitView, verb: string) {
    toolbox.reviewWith(() => api.prepareService(chosen.name, chosen.user, verb));
  }
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Services</h1>
      <p class="screen-subtitle">systemd's units and what starts at login. Each change asks first and says what it costs.</p>
    </div>
    <div class="screen-buttons">
      <input class="search" type="search" placeholder="Filter by name or description" aria-label="Filter services" bind:value={filter} />
      <button class="button" onclick={load}><Icon name="refresh" size={16} /> Read again</button>
    </div>
  </div>

  <div class="tabs" role="tablist" aria-label="Which units">
    {#each VIEWS as choice (choice.view)}
      <button
        class="tab"
        role="tab"
        aria-selected={view === choice.view}
        class:active={view === choice.view}
        onclick={() => {
          view = choice.view;
          selected = null;
        }}
      >
        {choice.label}
      </button>
    {/each}
  </div>

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if found === null}
        <p class="empty-note">Reading the services…</p>
      {:else if !found.systemd && view !== 'login'}
        <p class="empty-note">systemd does not run this machine, so there are no units. At login shows what the desktop starts.</p>
      {:else}
        <div class="rows" use:arrows={{ items: '.row-item', follow: '.row-item' }}>
          {#each units as item (key(item))}
            <button class="row-item unit" class:selected={selected === key(item)} onclick={() => (selected = key(item))}>
              <span class="pill" class:good={item.running} class:problem={item.failed} class:plain={!item.running && !item.failed}>
                {item.state}
              </span>
              <span>
                <span class="name">{item.name}</span>
                <span class="sub">{item.detail}</span>
              </span>
            </button>
          {:else}
            {#each applications as item (item.file)}
              <button class="row-item unit" class:selected={selected === `login:${item.file}`} onclick={() => (selected = `login:${item.file}`)}>
                <span class="pill" class:good={item.enabled && item.here} class:plain={!item.enabled || !item.here}>
                  {item.state}
                </span>
                <span>
                  <span class="name">{item.name}</span>
                  <span class="sub mono">{item.exec}</span>
                </span>
              </button>
            {:else}
              <p class="empty-note">Nothing here{filter ? ' with that filter' : ''}.</p>
            {/each}
          {/each}
        </div>
      {/if}
    </div>

    <aside class="pane side card">
      {#if unit}
        <h2>{unit.name}</h2>
        {#if unit.description}<p>{unit.description}</p>{/if}
        <dl class="kv">
          <dt>Whose</dt>
          <dd>{unit.user ? 'Your session' : 'The machine'}</dd>
          <dt>At boot or login</dt>
          <dd>{unit.enablement}</dd>
          <dt>Now</dt>
          <dd>{unit.now}</dd>
          {#if details && details.needed_by.length > 0}<dt>Needed by</dt><dd>{details.needed_by.join(', ')}</dd>{/if}
        </dl>
        {#if details?.consequence}
          <p class="warning"><Icon name="warning" size={14} /> {details.consequence}</p>
        {/if}
        {#if unit.note}
          <p class="faint">{unit.note}</p>
        {/if}
        <div class="actions">
          {#if unit.running}
            <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => change(unit, 'stop')}><Icon name="stop" size={14} /> Stop…</button>
            <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => change(unit, 'restart')}><Icon name="refresh" size={14} /> Restart…</button>
          {:else}
            <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => change(unit, 'start')}><Icon name="play" size={14} /> Start…</button>
          {/if}
          {#if unit.switchable}
            {#if unit.enabled}
              <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => change(unit, 'disable')}><Icon name="x" size={14} /> Disable…</button>
            {:else}
              <button class="button" disabled={toolbox.preparing || toolbox.busy} onclick={() => change(unit, 'enable')}><Icon name="check" size={14} /> Enable…</button>
            {/if}
          {/if}
        </div>
      {:else if application}
        <h2>{application.name}</h2>
        {#if application.comment}<p>{application.comment}</p>{/if}
        <dl class="kv">
          <dt>Runs</dt>
          <dd class="mono">{application.runs}</dd>
          <dt>From</dt>
          <dd>{application.from}</dd>
          <dt>At login</dt>
          <dd>{application.at_login}</dd>
        </dl>
        <div class="actions">
          <button
            class="button"
            disabled={toolbox.preparing || toolbox.busy}
            onclick={() => toolbox.reviewWith(() => api.prepareAutostart(application.file, !application.enabled))}
          >
            <Icon name={application.enabled ? 'x' : 'check'} size={14} />
            {application.enabled ? 'Do not start it at login…' : 'Start it at login…'}
          </button>
        </div>
        <p class="faint">For you only: the system's file stays as it is.</p>
      {:else}
        <p class="faint">Select a unit or an application to see what it is and change it.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .search {
    width: 260px;
    height: 32px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .tab {
    padding: 6px 12px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: transparent;
    color: var(--muted);
    font: inherit;
    cursor: pointer;
  }

  .tab.active {
    border-color: var(--accent);
    color: var(--text);
  }

  .unit {
    grid-template-columns: 96px minmax(0, 1fr);
  }

  .unit > span:nth-child(2) {
    display: grid;
    min-width: 0;
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
