<script lang="ts">
  import Icon, { type IconName } from '../components/Icon.svelte';
  import { toolbox } from '../lib/state.svelte';

  const counts = $derived.by(() => {
    const all = Object.values(toolbox.statuses);
    return {
      installed: all.filter((status) => status.kind === 'installed' || status.kind === 'managed').length,
      updates: all.filter((status) => status.kind === 'update').length,
      packages: all.length,
    };
  });

  const applications = $derived(Math.max(0, toolbox.areas.findIndex((area) => area.id === 'applications')));

  const actions: { icon: IconName; title: string; detail: string; go: () => void }[] = [
    { icon: 'apps', title: 'Browse Applications', detail: 'Search and install tools', go: () => toolbox.open({ kind: 'area', index: applications }) },
    { icon: 'plan', title: 'Review the Install Plan', detail: 'See what is queued and run it', go: () => toolbox.open({ kind: 'plan' }) },
    { icon: 'check', title: 'Set up this machine', detail: 'A workstation to choose, as an Install Plan to review', go: () => (toolbox.wizard = true) },
    { icon: 'search', title: 'Find anything', detail: 'Screens, actions and entries (Ctrl K)', go: () => (toolbox.palette = true) },
    { icon: 'star', title: 'Favorites', detail: 'Pinned entries', go: () => toolbox.open({ kind: 'favorites' }) },
    { icon: 'history', title: 'View History', detail: 'Past runs and their results', go: () => toolbox.open({ kind: 'history' }) },
  ];
</script>

<div class="screen">
  <div>
    <h1 class="screen-title">Dashboard</h1>
    <p class="screen-subtitle">Welcome to your Linux command center.</p>
  </div>

  {#if toolbox.about && !toolbox.about.command_line}
    <div class="notice">
      <Icon name="warning" />
      <span>The <span class="mono">helix-toolbox</span> command was not found beside this program or on the PATH. Built-in steps, such as the system report, need it.</span>
    </div>
  {/if}

  <div class="stats">
    {@render stat(counts.installed, 'installed', 'Package entries installed', 'green')}
    {@render stat(counts.updates, 'updates', 'With an update waiting', 'yellow')}
    {@render stat(toolbox.queue.length, 'queued', 'In the Install Plan', 'cyan')}
    {@render stat(toolbox.favorites.length, 'pinned', 'Favorites', 'purple')}
  </div>

  <div class="columns">
    <section class="card overview">
      <h2 class="card-title">System Overview</h2>
      <dl>
        {#each toolbox.about?.overview.rows ?? [] as [label, value] (label)}
          <dt>{label}</dt>
          <dd>{value}</dd>
        {/each}
      </dl>
    </section>

    <section class="card">
      <h2 class="card-title">Quick Actions</h2>
      <div class="actions">
        {#each actions as action (action.title)}
          <button class="action" onclick={action.go}>
            <span class="icon"><Icon name={action.icon} size={20} /></span>
            <span>
              <span class="title">{action.title}</span>
              <span class="detail">{action.detail}</span>
            </span>
            <Icon name="chevron" size={16} />
          </button>
        {/each}
      </div>
    </section>
  </div>
</div>

{#snippet stat(value: number, unit: string, label: string, color: string)}
  <div class="card stat">
    <div class="value" style:color="var(--{color})">
      {#if toolbox.checking && (unit === 'installed' || unit === 'updates') && counts.packages === 0}…{:else}{value}{/if}
    </div>
    <div class="faint">{label}</div>
  </div>
{/snippet}

<style>
  .screen {
    display: flex;
    flex-direction: column;
    gap: 18px;
    height: 100%;
    padding: 20px 24px;
    overflow-y: auto;
  }

  .notice {
    display: flex;
    gap: 10px;
    align-items: center;
    padding: 10px 14px;
    border-radius: var(--radius);
    border: 1px solid color-mix(in srgb, var(--yellow) 50%, transparent);
    background: color-mix(in srgb, var(--yellow) 8%, transparent);
    color: var(--yellow);
  }

  .notice span {
    color: var(--text);
  }

  .stats {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 14px;
  }

  .stat {
    padding: 14px 16px;
  }

  .value {
    font-size: 28px;
    font-weight: 800;
    line-height: 1.1;
  }

  .columns {
    display: grid;
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
    gap: 14px;
    align-items: start;
  }

  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 8px 24px;
    margin: 0;
    padding: 12px 16px 16px;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .actions {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
  }

  .action {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: var(--radius-small);
    border: 1px solid var(--line);
    background: var(--panel-raised);
    color: var(--faint);
    cursor: pointer;
    text-align: left;
  }

  .action:hover {
    border-color: var(--selected-line);
    background: var(--selected);
  }

  .icon {
    display: grid;
    place-items: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--cyan);
  }

  .title {
    display: block;
    font-weight: 700;
    color: var(--text);
  }

  .detail {
    display: block;
    font-size: 13px;
    color: var(--muted);
  }
</style>
