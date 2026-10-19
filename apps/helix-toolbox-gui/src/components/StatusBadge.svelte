<script lang="ts">
  import type { StatusView } from '../lib/types';

  let { status }: { status: StatusView | undefined } = $props();
</script>

{#if status}
  <span class="badge {status.kind}" title={status.detail || status.label}>{status.label}</span>
{/if}

<style>
  .badge {
    display: inline-block;
    padding: 1px 8px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    border: 1px solid currentColor;
  }

  .installed {
    color: var(--green);
  }

  .update {
    color: var(--yellow);
  }

  .managed {
    color: var(--cyan);
  }

  .absent {
    color: var(--faint);
    border-color: var(--line-strong);
  }
</style>
