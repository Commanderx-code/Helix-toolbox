<script lang="ts">
  import { untrack } from 'svelte';
  import { arrows } from '../lib/keys';
  import { TOOL_GROUPS, groupOf, same, type ToolGroup } from '../lib/nav';
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

  // Which tool groups are open, remembered in this window's storage; the
  // group of the screen shown opens by itself.
  const KEY = 'helix-toolbox-open-groups';
  function remembered(): string[] {
    try {
      const value = JSON.parse(localStorage.getItem(KEY) ?? '[]');
      return Array.isArray(value) ? value.filter((id) => typeof id === 'string') : [];
    } catch {
      return [];
    }
  }
  let opened = $state<string[]>(remembered());

  function remember() {
    try {
      localStorage.setItem(KEY, JSON.stringify(opened));
    } catch {
      // Without storage the groups still open and close; they are just not kept.
    }
  }

  // Only a change of screen opens its group, so closing that group sticks.
  $effect(() => {
    const group = groupOf(toolbox.screen);
    untrack(() => {
      if (group && !opened.includes(group.id)) {
        opened = [...opened, group.id];
        remember();
      }
    });
  });

  function toggle(group: ToolGroup) {
    opened = opened.includes(group.id) ? opened.filter((id) => id !== group.id) : [...opened, group.id];
    remember();
  }

  const updatesWaiting = $derived(Object.values(toolbox.statuses).filter((status) => status.kind === 'update').length);

  /** What a tool's badge counts, if anything. */
  function countOf(screen: Screen): [number, string] {
    return screen.kind === 'updates' ? [updatesWaiting, 'with an update'] : [0, ''];
  }

  function open(screen: Screen) {
    toolbox.screen = screen;
    toolbox.search = '';
  }
</script>

<nav aria-label="Screens" use:arrows={{ items: '.item' }}>
  <div class="section">Main menu</div>
  {@render item({ kind: 'dashboard' }, 'dashboard', 'Dashboard')}
  {@render item({ kind: 'favorites' }, 'star', 'Favorites', toolbox.favorites.length, false, 'pinned')}
  {@render item({ kind: 'plan' }, 'plan', 'Install Plan', toolbox.queue.length, true, 'queued')}
  {@render item({ kind: 'history' }, 'history', 'History')}

  <div class="section">Catalog</div>
  {#each toolbox.areas as area, index (area.id)}
    {#if area.id !== 'doctor'}
      {@render item({ kind: 'area', index }, AREA_ICONS[area.id] ?? 'apps', area.name)}
    {/if}
  {/each}

  <div class="section">More tools</div>
  {#each TOOL_GROUPS as group (group.id)}
    {@const expanded = opened.includes(group.id)}
    {@const waiting = group.tools.reduce((sum, tool) => sum + countOf(tool.screen)[0], 0)}
    <button
      class="item group"
      class:holds={!expanded && groupOf(toolbox.screen)?.id === group.id}
      aria-expanded={expanded}
      aria-controls="tools-{group.id}"
      onclick={() => toggle(group)}
    >
      <Icon name={group.icon} />
      <span class="label">{group.name}</span>
      {#if !expanded && waiting > 0}
        <span class="count" aria-label="{waiting} with an update">{waiting}</span>
      {/if}
      <span class="chevron" class:open={expanded}><Icon name="chevron" size={14} /></span>
    </button>
    <div class="tools" id="tools-{group.id}" hidden={!expanded}>
      {#each group.tools as tool (tool.label)}
        {@const [count, counted] = countOf(tool.screen)}
        {@render item(tool.screen, tool.icon, tool.label, count, false, counted)}
      {/each}
    </div>
  {/each}

  <div class="rule"></div>
  {@render item({ kind: 'settings' }, 'gear', 'Settings')}
</nav>

{#snippet item(screen: Screen, icon: IconName, label: string, count = 0, accent = false, counted = '')}
  <button
    class="item"
    class:active={same(toolbox.screen, screen)}
    aria-current={same(toolbox.screen, screen) ? 'page' : undefined}
    onclick={() => open(screen)}
  >
    <Icon name={icon} />
    <span class="label">{label}</span>
    {#if count > 0}
      <span class="count" class:accent aria-label="{count} {counted}">{count}</span>
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

  /* A constant height: the sidebar scrolls rather than squeezing rows. */
  .item {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    gap: 12px;
    height: 34px;
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

  .group {
    color: var(--text);
  }

  /* A closed group holding the screen shown. */
  .group.holds {
    border-color: var(--selected-line);
  }

  .chevron {
    display: grid;
    place-items: center;
    color: var(--faint);
    transition: transform 0.15s ease;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  @media (prefers-reduced-motion: reduce) {
    .chevron {
      transition: none;
    }
  }

  .tools {
    display: flex;
    flex-shrink: 0;
    flex-direction: column;
    gap: 2px;
    margin: 0 0 4px 18px;
    padding-left: 8px;
    border-left: 1px solid var(--line);
  }

  .tools[hidden] {
    display: none;
  }

  .tools .item {
    height: 32px;
    gap: 10px;
  }

  .rule {
    height: 1px;
    margin: 10px 4px;
    background: var(--line);
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
