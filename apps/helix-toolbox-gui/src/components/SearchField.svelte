<script lang="ts">
  import Icon from './Icon.svelte';
  import { toolbox } from '../lib/state.svelte';
</script>

<label class="search">
  <Icon name="search" size={16} />
  <input
    type="search"
    placeholder="Search every area"
    bind:value={toolbox.search}
    onkeydown={(event) => {
      if (event.key === 'Escape') toolbox.search = '';
    }}
  />
</label>

<style>
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 320px;
    height: 36px;
    padding: 0 12px;
    border-radius: var(--radius-small);
    border: 1px solid var(--line-strong);
    background: var(--panel);
    color: var(--faint);
  }

  .search:focus-within {
    border-color: var(--cyan);
  }

  input {
    flex: 1;
    min-width: 0;
    border: 0;
    outline: 0;
    background: transparent;
  }

  input::placeholder {
    color: var(--faint);
  }
</style>
