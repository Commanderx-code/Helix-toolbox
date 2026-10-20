<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { UpdatesView } from '../lib/types';

  let view = $state<UpdatesView | null>(null);
  let problem = $state<string | null>(null);
  let gathering = $state(false);
  let selectedId = $state<string | null>(null);

  const selected = $derived(view?.channels.find((channel) => channel.id === selectedId) ?? null);
  const withUpdates = $derived(
    view?.channels.filter((channel) => (channel.count ?? 0) > 0 && channel.command_line !== '') ?? [],
  );
  /** Package entries whose status says an update waits. */
  const entries = $derived(
    Object.entries(toolbox.statuses)
      .filter(([, status]) => status.kind === 'update')
      .map(([id, status]) => ({ id, status, entry: toolbox.entry(id) }))
      .filter((item) => item.entry !== undefined),
  );

  async function gather(online: boolean) {
    gathering = true;
    try {
      view = await api.updates(online);
      problem = null;
      if (!selectedId) selectedId = view.channels[0]?.id ?? null;
    } catch (error) {
      problem = String(error);
    } finally {
      gathering = false;
    }
  }

  // The palette's "Check updates online" opens the screen asking online once.
  gather(toolbox.wantOnline);
  toolbox.wantOnline = false;

  const versions = (from: string | null, to: string | null) => (from && to ? `${from} → ${to}` : (to ?? ''));
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Updates</h1>
      <p class="screen-subtitle">What each tool has waiting; updating runs that tool, after confirmation.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={gathering} onclick={() => gather(true)}>
        <Icon name="refresh" size={16} />
        {gathering ? 'Checking…' : 'Check online'}
      </button>
      <button
        class="button primary"
        disabled={withUpdates.length === 0 || toolbox.preparing || toolbox.busy}
        onclick={() => toolbox.reviewWith(() => api.prepareChannels(withUpdates.map((channel) => channel.id)))}
      >
        <Icon name="play" size={14} /> Update all ({withUpdates.length})
      </button>
    </div>
  </div>

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if view === null}
        <p class="empty-note">Looking at the update channels…</p>
      {:else}
        <div class="heading-row">Channels · {view.online ? 'checked online' : 'from what the tools already know'}</div>
        <div class="rows">
          {#each view.channels as channel (channel.id)}
            <button class="row-item channel" class:selected={selectedId === channel.id} onclick={() => (selectedId = channel.id)}>
              <span class="name">{channel.name}</span>
              <span class="count" class:waiting={(channel.count ?? 0) > 0}>
                {channel.count === null ? 'unknown' : channel.count === 0 ? 'up to date' : `${channel.count} updates`}
              </span>
              <span class="sub">{channel.summary}</span>
            </button>
          {/each}
        </div>
        {#if entries.length > 0}
          <div class="heading-row">Entries with an update</div>
          <div class="rows">
            {#each entries as { id, status, entry } (id)}
              <div class="row-item entry">
                <span class="name">{entry?.name}</span>
                <span><StatusBadge {status} /></span>
                <span class="sub">{status.detail}</span>
                <button
                  class="button small"
                  disabled={toolbox.preparing || toolbox.busy}
                  onclick={() => toolbox.reviewWith(() => api.prepareUpdate([id]), [id])}
                >
                  Update
                </button>
              </div>
            {/each}
            <div class="all">
              <button
                class="button"
                disabled={toolbox.preparing || toolbox.busy}
                onclick={() => toolbox.reviewWith(() => api.prepareUpdate(entries.map((item) => item.id)), entries.map((item) => item.id))}
              >
                <Icon name="play" size={14} /> Update every entry ({entries.length})
              </button>
            </div>
          </div>
        {/if}
      {/if}
    </div>

    <aside class="pane side card">
      {#if selected}
        <h2>{selected.name}</h2>
        <p>{selected.summary}</p>
        {#if selected.note}<p class="muted">{selected.note}</p>{/if}
        {#if selected.pending && selected.pending.length > 0}
          <h3>Waiting</h3>
          <ul class="pending">
            {#each selected.pending as item, index (index)}
              <li><span class="mono">{item.name}</span> <span class="faint">{versions(item.from, item.to)}</span></li>
            {/each}
          </ul>
        {/if}
        <h3>Updating runs</h3>
        {#if selected.command_line}
          <p class="mono small">{selected.command_line}</p>
        {:else}
          <p class="muted">There is nothing to run here.</p>
        {/if}
        {#if selected.caveat}<p class="caveat"><Icon name="warning" size={14} /> {selected.caveat}</p>{/if}
        {#if selected.command_line}
          <div class="actions">
            <button
              class="button primary"
              disabled={toolbox.preparing || toolbox.busy}
              onclick={() => toolbox.reviewWith(() => api.prepareChannels([selected.id]))}
            >
              <Icon name="play" size={14} /> Update this channel
            </button>
          </div>
        {/if}
      {:else}
        <p class="faint">Select a channel to see what waits in it.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .channel {
    grid-template-columns: 180px 120px minmax(0, 1fr);
  }

  .entry {
    grid-template-columns: 180px auto minmax(0, 1fr) auto;
    cursor: default;
  }

  .count {
    color: var(--green);
    font-weight: 600;
  }

  .count.waiting {
    color: var(--yellow);
  }

  .all {
    padding: 8px 12px;
  }

  .small {
    font-size: 12.5px;
    word-break: break-all;
  }

  .pending {
    list-style: none;
    padding: 0;
    max-height: 260px;
    overflow-y: auto;
  }

  .pending li {
    padding: 2px 0;
  }

  .caveat {
    display: flex;
    gap: 6px;
    align-items: flex-start;
    color: var(--yellow);
  }

  .button.small {
    height: 28px;
    padding: 0 10px;
  }

  .bad {
    color: var(--red);
  }
</style>
