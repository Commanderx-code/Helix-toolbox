<script lang="ts">
  import { arrows } from '../lib/keys';
  import EntryDetails from '../components/EntryDetails.svelte';
  import EntryRow from '../components/EntryRow.svelte';
  import { toolbox } from '../lib/state.svelte';

  const pinned = $derived(toolbox.favorites.map((id) => toolbox.byId.get(id)).filter((place) => place !== undefined));
</script>

<div class="screen">
  <div>
    <h1 class="screen-title">Favorites</h1>
    <p class="screen-subtitle">Entries you pinned, here and in the terminal interface.</p>
  </div>
  <div class="body">
    <div class="list card" use:arrows={{ items: '.select, .group', follow: '.select' }}>
      {#each pinned as { entry, area, group } (entry.id)}
        <EntryRow {entry} place={group ? `${area} › ${group}` : area} />
      {:else}
        <p class="faint none">Nothing pinned yet. Select an entry and choose Pin.</p>
      {/each}
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

  .body {
    display: flex;
    gap: 16px;
    flex: 1;
    min-height: 0;
  }

  .list {
    flex: 1;
    min-width: 0;
    padding: 8px;
    overflow-y: auto;
  }

  .none {
    padding: 12px;
  }
</style>
