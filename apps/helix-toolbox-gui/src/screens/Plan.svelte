<script lang="ts">
  import Flags from '../components/Flags.svelte';
  import Icon from '../components/Icon.svelte';
  import StatusBadge from '../components/StatusBadge.svelte';
  import { KINDS } from '../lib/flags';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';

  const queued = $derived(toolbox.queue.map((id) => toolbox.byId.get(id)).filter((place) => place !== undefined));

  async function saveProfile() {
    const name = await toolbox.ask('Save as a profile', 'A name for this plan, to run again later from Profiles.', 'My workstation');
    if (!name) return;
    const ids = [...toolbox.queue];
    if (await toolbox.tell('Profile saved', () => api.saveProfile(name, ids))) {
      await toolbox.reloadAreas();
    }
  }

  function move(index: number, by: number) {
    const queue = [...toolbox.queue];
    const [id] = queue.splice(index, 1);
    queue.splice(index + by, 0, id);
    toolbox.queue = queue;
  }
</script>

<div class="screen">
  <div class="top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Install Plan</h1>
      <p class="screen-subtitle">Review planned changes before they run. Steps that need another come after it.</p>
    </div>
    <div class="buttons">
      <button class="button ghost" disabled={queued.length === 0} onclick={() => (toolbox.queue = [])}>
        <Icon name="x" size={16} /> Clear
      </button>
      <button class="button" disabled={queued.length === 0} onclick={saveProfile}>
        <Icon name="save" size={16} /> Save as profile
      </button>
      <button
        class="button primary"
        disabled={queued.length === 0 || toolbox.preparing || toolbox.busy}
        onclick={() => toolbox.review(toolbox.queue, true)}
      >
        <Icon name="play" size={14} />
        {toolbox.preparing ? 'Planning…' : 'Review and run'}
      </button>
    </div>
  </div>

  <section class="card list">
    <h2 class="card-title">Queued entries · {queued.length}</h2>
    <div class="rows">
      {#each queued as { entry, area, group }, index (entry.id)}
        <div class="row">
          <span class="number faint">{index + 1}</span>
          <div class="text">
            <div class="name">{entry.name}</div>
            <div class="faint where">{area}{group ? ` › ${group}` : ''} · {KINDS[entry.kind]}</div>
          </div>
          <span><StatusBadge status={toolbox.statuses[entry.id]} /></span>
          <Flags field={entry.flags} />
          <div class="order">
            <button class="button ghost small" title="Earlier" aria-label="Move {entry.name} earlier" disabled={index === 0} onclick={() => move(index, -1)}>↑</button>
            <button class="button ghost small" title="Later" aria-label="Move {entry.name} later" disabled={index === queued.length - 1} onclick={() => move(index, 1)}>↓</button>
            <button class="button ghost small" title="Take out" aria-label="Take {entry.name} out of the plan" onclick={() => toolbox.toggleQueued(entry.id)}>
              <Icon name="x" size={14} />
            </button>
          </div>
        </div>
      {:else}
        <div class="empty">
          <Icon name="plan" size={28} />
          <p>The Install Plan is empty.</p>
          <p class="faint">Tick entries in the catalog to queue them here.</p>
        </div>
      {/each}
    </div>
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
    gap: 16px;
  }

  .buttons {
    display: flex;
    gap: 8px;
  }

  .list {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
  }

  .rows {
    flex: 1;
    overflow-y: auto;
    padding: 10px 8px;
  }

  .row {
    display: grid;
    grid-template-columns: 28px minmax(0, 1fr) auto auto auto;
    align-items: center;
    gap: 14px;
    padding: 8px 10px;
    border-radius: var(--radius-small);
  }

  .row:hover {
    background: var(--panel-raised);
  }

  .number {
    text-align: right;
    font-family: var(--mono);
  }

  .name {
    font-weight: 600;
  }

  .where {
    font-size: 13px;
  }

  .order {
    display: flex;
    gap: 2px;
  }

  .small {
    height: 28px;
    padding: 0 8px;
  }

  .empty {
    display: grid;
    justify-items: center;
    gap: 2px;
    margin-top: 60px;
    color: var(--muted);
  }

  .empty p {
    margin: 4px 0 0;
  }
</style>
