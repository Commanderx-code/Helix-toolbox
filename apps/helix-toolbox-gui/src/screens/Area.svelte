<script lang="ts">
  import EntryDetails from '../components/EntryDetails.svelte';
  import EntryRow from '../components/EntryRow.svelte';
  import ItemList from '../components/ItemList.svelte';
  import SearchField from '../components/SearchField.svelte';
  import { toolbox } from '../lib/state.svelte';

  let { index }: { index: number } = $props();

  const area = $derived(toolbox.areas[index]);
  const matches = $derived(toolbox.matches(toolbox.search));
</script>

<div class="screen">
  <div class="top">
    <div>
      <h1 class="screen-title">{toolbox.search ? 'Search' : area?.name}</h1>
      <p class="screen-subtitle">
        {#if toolbox.search}
          {matches.length} {matches.length === 1 ? 'entry matches' : 'entries match'} in every area
        {:else}
          Tick entries for the Install Plan, or select one to run it now.
        {/if}
      </p>
    </div>
    <SearchField />
  </div>
  <div class="body">
    <div class="list card">
      <div class="columns faint">
        <span></span><span>Entry</span><span>Status</span><span class="right">Flags</span>
      </div>
      <div class="scroll">
        {#if toolbox.search}
          {#each matches as { entry, area: where, group } (entry.id)}
            <EntryRow {entry} place={group ? `${where} › ${group}` : where} />
          {:else}
            <p class="faint none">Nothing matches “{toolbox.search}”.</p>
          {/each}
        {:else if area}
          {#key area.id}
            <ItemList items={area.items} />
          {/key}
        {/if}
      </div>
    </div>
    <EntryDetails />
  </div>
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

  .body {
    display: flex;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .list {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }

  .columns {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr) auto 92px;
    gap: 12px;
    padding: 10px 28px 8px;
    border-bottom: 1px solid var(--line);
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }

  .right {
    text-align: right;
  }

  .scroll {
    flex: 1;
    overflow-y: auto;
    padding: 8px;
  }

  .none {
    padding: 12px;
  }
</style>
