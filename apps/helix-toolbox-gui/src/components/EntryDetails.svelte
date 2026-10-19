<script lang="ts">
  import Icon from './Icon.svelte';
  import StatusBadge from './StatusBadge.svelte';
  import { flags, KINDS } from '../lib/flags';
  import { toolbox } from '../lib/state.svelte';

  const place = $derived(toolbox.selected ? toolbox.byId.get(toolbox.selected) : undefined);
  const entry = $derived(place?.entry);
  const status = $derived(entry ? toolbox.statuses[entry.id] : undefined);
  const queued = $derived(entry ? toolbox.isQueued(entry.id) : false);
  const pinned = $derived(entry ? toolbox.favorites.includes(entry.id) : false);
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
