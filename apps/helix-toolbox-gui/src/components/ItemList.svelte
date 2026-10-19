<script lang="ts">
  import Icon from './Icon.svelte';
  import EntryRow from './EntryRow.svelte';
  import Self from './ItemList.svelte';
  import type { ItemView } from '../lib/types';

  let { items, depth = 0 }: { items: ItemView[]; depth?: number } = $props();

  // Groups start open at the top level and closed below it.
  let closed = $state<Record<string, boolean>>({});
  const isOpen = (id: string) => !(closed[id] ?? depth > 0);
</script>

{#each items as item (item.id)}
  {#if item.type === 'entry'}
    <EntryRow entry={item} />
  {:else}
    <section class:nested={depth > 0}>
      <button class="group" onclick={() => (closed[item.id] = isOpen(item.id))}>
        <span class="chevron" class:open={isOpen(item.id)}><Icon name="chevron" size={14} /></span>
        <span class="name">{item.name}</span>
        <span class="faint description">{item.description}</span>
        <span class="faint count">{item.items.length}</span>
      </button>
      {#if isOpen(item.id)}
        <div class="items">
          <Self items={item.items} depth={depth + 1} />
        </div>
      {/if}
    </section>
  {/if}
{/each}

<style>
  section {
    margin-bottom: 10px;
  }

  section.nested {
    margin: 4px 0 4px 14px;
  }

  .group {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: var(--radius-small);
    background: none;
    cursor: pointer;
    text-align: left;
  }

  .group:hover {
    background: var(--panel-raised);
  }

  .chevron {
    display: inline-flex;
    color: var(--faint);
    transition: transform 0.12s;
  }

  .chevron.open {
    transform: rotate(90deg);
  }

  .name {
    font-weight: 700;
    color: var(--purple);
  }

  .description {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 13px;
  }

  .count {
    font-size: 12px;
  }

  .items {
    padding-left: 6px;
  }
</style>
