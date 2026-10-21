<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusBadge from './StatusBadge.svelte';
  import * as api from '../lib/api';
  import { flags, KINDS } from '../lib/flags';
  import { toolbox } from '../lib/state.svelte';

  const place = $derived(toolbox.selected ? toolbox.byId.get(toolbox.selected) : undefined);
  const entry = $derived(place?.entry);
  const status = $derived(entry ? toolbox.statuses[entry.id] : undefined);
  const queued = $derived(entry ? toolbox.isQueued(entry.id) : false);
  const pinned = $derived(entry ? toolbox.favorites.includes(entry.id) : false);
  // As the terminal interface's `r`: a package entry that offers removal,
  // once it is found on this machine.
  const removable = $derived(entry?.removable && (status?.kind === 'installed' || status?.kind === 'update'));

  async function exportProfile(id: string) {
    try {
      const said = await api.exportProfile(id);
      if (said) toolbox.notice = { title: 'Profile exported', text: said };
    } catch (error) {
      toolbox.notice = { title: 'The profile was not exported', text: String(error) };
    }
  }

  async function deleteProfile(id: string, name: string) {
    const word = await toolbox.ask(
      'Delete the profile',
      `${name} is deleted from your profiles; the entries it runs stay in the catalog. Export it first to keep a copy. Type DELETE to delete it:`,
      'DELETE',
      'DELETE',
    );
    if (word !== 'DELETE') return;
    try {
      toolbox.notice = { title: 'Profile deleted', text: await api.deleteProfile(id) };
      await toolbox.profilesChanged(id);
    } catch (error) {
      toolbox.notice = { title: 'The profile was not deleted', text: String(error) };
    }
  }
</script>

<aside class="card">
  {#if entry && place}
    <div class="where">{place.area}{place.group ? ` › ${place.group}` : ''}</div>
    <h2>{entry.name}</h2>
    <div class="tags">
      <span class="kind">{KINDS[entry.kind] ?? entry.kind}</span>
      {#if entry.interactive}<span class="kind">Asks questions</span>{/if}
      <StatusBadge {status} />
    </div>
    {#if entry.description}
      <p>{entry.description}</p>
    {/if}
    {#if status?.detail}
      <div class="fact"><span class="faint">Installed from</span> <span class="mono">{status.detail}</span></div>
    {/if}
    {#if entry.runs.length > 0}
      <div class="heading">Runs, in order</div>
      <ol>
        {#each entry.runs as id (id)}
          <li>{toolbox.entry(id)?.name ?? id}</li>
        {/each}
      </ol>
    {/if}
    {#if entry.flags.trim()}
      <div class="heading">What it does</div>
      <ul class="meanings">
        {#each flags(entry.flags) as { flag, meaning, high } (flag)}
          <li><span class="mono flag" class:high>{flag}</span> {meaning}</li>
        {/each}
      </ul>
    {/if}
    <div class="actions">
      {#if entry.kind === 'profile' && entry.runs.length > 0}
        <button
          class="button"
          title="Queue the profile's entries one by one, to change the plan before it runs"
          onclick={() => {
            const added = toolbox.enqueue(entry.runs);
            toolbox.open({ kind: 'plan' });
            toolbox.notice = {
              title: entry.name,
              text: added === 0 ? 'Its entries are already in the Install Plan.' : `${added} of its entries were added to the Install Plan. Nothing has run.`,
            };
          }}
        >
          <Icon name="plan" size={15} /> Put its entries in the plan
        </button>
      {/if}
      {#if entry.queueable}
        <button class="button" class:primary={!queued} onclick={() => toolbox.toggleQueued(entry.id)}>
          <Icon name={queued ? 'minus' : 'plus'} size={16} />
          {queued ? 'Take out of plan' : 'Add to plan'}
        </button>
      {/if}
      <button
        class="button"
        class:primary={!entry.queueable}
        disabled={toolbox.preparing || toolbox.busy}
        onclick={() => toolbox.review([entry.id], false)}
      >
        <Icon name="play" size={14} /> Run now
      </button>
      {#if entry.yours}
        <button class="button" onclick={() => exportProfile(entry.id)}>
          <Icon name="save" size={14} /> Export…
        </button>
        <button class="button danger" onclick={() => deleteProfile(entry.id, entry.name)}>
          <Icon name="trash" size={14} /> Delete…
        </button>
      {/if}
      {#if removable}
        <button
          class="button danger"
          disabled={toolbox.preparing || toolbox.busy}
          onclick={() => toolbox.reviewWith(() => api.prepareRemove([entry.id]), [entry.id])}
        >
          <Icon name="trash" size={14} /> Remove…
        </button>
      {/if}
      <button class="button ghost" title={pinned ? 'Unpin' : 'Pin to Favorites'} onclick={() => toolbox.togglePin(entry.id)}>
        <Icon name="star" size={16} />
        {pinned ? 'Unpin' : 'Pin'}
      </button>
    </div>
    {#if !entry.queueable}
      <p class="faint note">This entry runs on its own and cannot be queued with others.</p>
    {/if}
  {:else}
    <div class="empty">
      <Icon name="info" size={28} />
      <p>Select an entry to see what it does.</p>
      <p class="faint">Tick entries to queue them in the Install Plan, then review and run them together.</p>
    </div>
  {/if}
</aside>

<style>
  aside {
    width: 340px;
    flex-shrink: 0;
    padding: 18px;
    overflow-y: auto;
  }

  .where {
    color: var(--faint);
    font-size: 12px;
  }

  h2 {
    margin: 4px 0 10px;
    font-size: 20px;
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }

  .kind {
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--panel-raised);
    border: 1px solid var(--line-strong);
    color: var(--muted);
    font-size: 12px;
  }

  p {
    margin: 14px 0 0;
    color: var(--text);
  }

  .fact {
    margin-top: 12px;
    font-size: 13px;
  }

  .heading {
    margin: 18px 0 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--purple);
  }

  ol,
  ul {
    margin: 0;
    padding-left: 20px;
    color: var(--muted);
  }

  .meanings {
    list-style: none;
    padding: 0;
  }

  .meanings li {
    margin: 4px 0;
  }

  .flag {
    display: inline-block;
    min-width: 30px;
    color: var(--cyan);
  }

  .flag.high {
    color: var(--red);
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 22px;
  }

  .note {
    font-size: 12px;
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 4px;
    margin-top: 40px;
    color: var(--muted);
    text-align: center;
  }

  .empty p {
    margin: 4px 0 0;
    color: inherit;
  }

  .empty .faint {
    color: var(--faint);
    font-size: 13px;
  }
</style>
