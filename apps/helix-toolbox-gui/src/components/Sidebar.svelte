<script lang="ts">
  import Icon, { type IconName } from './Icon.svelte';
  import { toolbox, type Screen } from '../lib/state.svelte';

  // Areas by id get their own icon; any other gets the grid.
  const AREA_ICONS: Record<string, IconName> = {
    applications: 'apps',
    configurations: 'config',
    maintenance: 'maintenance',
    security: 'security',
    profiles: 'profiles',
  };

  const same = (a: Screen, b: Screen) =>
    a.kind === b.kind && (a.kind !== 'area' || (b.kind === 'area' && a.index === b.index));

  function open(screen: Screen) {
    toolbox.screen = screen;
    toolbox.search = '';
  }
</script>

<nav>
  <div class="section">Main menu</div>
  {@render item({ kind: 'dashboard' }, 'dashboard', 'Dashboard')}
  {@render item({ kind: 'favorites' }, 'star', 'Favorites', toolbox.favorites.length)}
  {@render item({ kind: 'plan' }, 'plan', 'Install Plan', toolbox.queue.length, true)}
  {@render item({ kind: 'history' }, 'history', 'History')}

  <div class="section">Catalog</div>
  {#each toolbox.areas as area, index (area.id)}
    {@render item({ kind: 'area', index }, AREA_ICONS[area.id] ?? 'apps', area.name)}
  {/each}
</nav>

{#snippet item(screen: Screen, icon: IconName, label: string, count = 0, accent = false)}
  <button class="item" class:active={same(toolbox.screen, screen)} onclick={() => open(screen)}>
    <Icon name={icon} />
    <span class="label">{label}</span>
    {#if count > 0}
      <span class="count" class:accent>{count}</span>
    {/if}
  </button>
{/snippet}

<style>
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    width: 232px;
    flex-shrink: 0;
    padding: 14px 12px;
    border-right: 1px solid var(--line-strong);
    overflow-y: auto;
  }

  .section {
    margin: 14px 10px 6px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .section:first-child {
    margin-top: 0;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 38px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: var(--radius-small);
    background: none;
    color: var(--muted);
    cursor: pointer;
    text-align: left;
    font-weight: 600;
  }

  .item:hover {
    background: var(--panel);
    color: var(--text);
  }

  .item.active {
    background: var(--selected);
    border-color: var(--selected-line);
    color: var(--text);
  }

  .item.active :global(svg) {
    color: var(--cyan);
  }

  .label {
    flex: 1;
  }

  .count {
    min-width: 22px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--panel-raised);
    border: 1px solid var(--line-strong);
    color: var(--muted);
    font-size: 12px;
    text-align: center;
  }

  .count.accent {
    background: var(--cyan);
    border-color: var(--cyan);
    color: var(--on-accent);
  }
</style>
