<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { DesktopView } from '../lib/types';

  let view = $state<DesktopView | null>(null);
  let problem = $state<string | null>(null);

  async function load() {
    try {
      view = await api.desktop();
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  load();

  async function saveLook() {
    const name = await toolbox.ask('Save this look', 'A name for the appearance as it is now: theme, icons, fonts and the rest, to put back later.', 'Dark and quiet');
    if (!name) return;
    if (await toolbox.tell('Look saved', () => api.saveLook(name))) await load();
  }

  const title = $derived.by(() => {
    const desktop = view?.desktop;
    if (!desktop) return '';
    if (desktop.name && desktop.session) return `${desktop.name}, ${desktop.session} session`;
    return desktop.name ?? 'No desktop environment found';
  });
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Desktop</h1>
      <p class="screen-subtitle">The running desktop and its appearance; looks can be saved and put back.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" onclick={saveLook} disabled={!view?.desktop.kind}><Icon name="save" size={16} /> Save this look</button>
      <button class="button" onclick={load}><Icon name="refresh" size={16} /> Read again</button>
    </div>
  </div>

  <div class="split">
    <div class="pane card content">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if view === null}
        <p class="empty-note">Reading the desktop…</p>
      {:else}
        <h2 class="section">This desktop</h2>
        <p class="big">{title}</p>
        {#if view.desktop.display_manager}
          <p class="muted">
            Login screen: {view.desktop.display_manager.name}{view.desktop.display_manager.theme ? `, theme ${view.desktop.display_manager.theme}` : ''}
            {#if view.desktop.display_manager.files.length > 0}
              <span class="faint">({view.desktop.display_manager.files.join(', ')})</span>
            {/if}
          </p>
        {/if}
        {#each view.desktop.owners as owner, index (index)}<p class="muted">Note: {owner}</p>{/each}
        {#if view.desktop.blocked}<p class="warning"><Icon name="warning" size={14} /> Not changed here: {view.desktop.blocked}</p>{/if}

        {#if view.desktop.appearance.length > 0}
          <h2 class="section">Appearance</h2>
          <dl class="kv">
            {#each view.desktop.appearance as value (value.key)}
              <dt>{value.label}</dt>
              <dd class:faint={value.value === null}>{value.value ?? 'not set'}</dd>
            {/each}
          </dl>
        {/if}

        {#if view.suggestions.length > 0}
          <h2 class="section">For this desktop</h2>
          {#each view.suggestions as suggestion (suggestion.id)}
            <div class="line">
              <span class="grow">
                <span class="name">{suggestion.name}{toolbox.isQueued(suggestion.id) ? ' · planned' : ''}</span>
                {#if suggestion.note}<span class="sub">{suggestion.note}</span>{/if}
              </span>
              {#if toolbox.entry(suggestion.id)?.queueable}
                <button class="button small" onclick={() => toolbox.toggleQueued(suggestion.id)}>
                  {toolbox.isQueued(suggestion.id) ? 'Take out of plan' : 'Add to plan'}
                </button>
              {/if}
              <button class="button small" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.review([suggestion.id], false)}>Run now</button>
            </div>
          {/each}
        {/if}
      {/if}
    </div>

    <aside class="pane side card">
      <h2>Saved looks</h2>
      {#if view && view.looks.length > 0}
        {#each view.looks as look (look.slug)}
          <div class="look">
            <span class="grow">
              <span class="name">{look.name}</span>
              <span class="sub">{look.other ? `saved on ${look.desktop}` : `${look.settings} settings`}</span>
            </span>
            <button class="button small" disabled={toolbox.preparing || toolbox.busy} onclick={() => toolbox.reviewWith(() => api.prepareLook(look.slug))}>Put back…</button>
          </div>
        {/each}
      {:else}
        <p class="faint">No looks saved yet. Save this look to put it back later, after trying another.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .content {
    padding: 4px 18px 18px;
  }

  .big {
    font-size: 17px;
    font-weight: 600;
  }

  .warning {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--yellow);
  }

  .line,
  .look {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: var(--radius-small);
  }

  .line:hover,
  .look:hover {
    background: var(--panel-raised);
  }

  .grow {
    display: grid;
    flex: 1;
    min-width: 0;
  }

  .name {
    font-weight: 600;
  }

  .sub {
    color: var(--muted);
    font-size: 13px;
  }

  .button.small {
    height: 28px;
    padding: 0 10px;
  }

  .bad {
    color: var(--red);
  }
</style>
