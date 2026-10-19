<script lang="ts">
  import { flags } from '../lib/flags';

  let { field }: { field: string } = $props();
</script>

<span class="flags">
  {#each flags(field) as { flag, meaning, high } (flag)}
    <span class="flag" class:high title={meaning}>{flag}</span>
  {/each}
</span>

<style>
  .flags {
    display: inline-flex;
    gap: 4px;
  }

  .flag {
    padding: 0 6px;
    border-radius: 4px;
    background: var(--panel-raised);
    border: 1px solid var(--line-strong);
    color: var(--muted);
    font-family: var(--mono);
    font-size: 11px;
    line-height: 18px;
  }

  .flag.high {
    color: var(--red);
    border-color: rgba(255, 120, 140, 0.5);
  }
</style>
