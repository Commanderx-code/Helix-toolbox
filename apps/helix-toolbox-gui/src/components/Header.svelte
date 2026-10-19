<script lang="ts">
  import emblem from '../../../helix-toolbox/src/tui/emblem.png';
  import { toolbox } from '../lib/state.svelte';
</script>

<header>
  <img class="emblem" src={emblem} alt="" />
  <div class="brand">
    <div class="wordmark"><span class="helix">HELIX</span><span class="toolbox">TOOLBOX</span></div>
    <div class="tagline">BUILD • CONFIGURE • OPTIMIZE</div>
  </div>
  <div class="spacer"></div>
  {#if toolbox.about}
    <span class="version">v{toolbox.about.version}</span>
  {/if}
  {#if toolbox.busy}
    <span class="status running">● RUNNING</span>
  {:else}
    <span class="status ready">● SYSTEM READY</span>
  {/if}
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 72px;
    padding: 0 20px;
    border-bottom: 1px solid var(--line-strong);
    background: var(--bg);
  }

  .emblem {
    height: 52px;
    width: auto;
  }

  .brand {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .wordmark {
    display: flex;
    gap: 0.9em;
    font-size: 17px;
    font-weight: 800;
    letter-spacing: 0.42em;
  }

  .helix {
    color: var(--cyan);
  }

  .toolbox {
    color: var(--purple);
  }

  .tagline {
    font-size: 11px;
    letter-spacing: 0.18em;
    color: var(--faint);
  }

  .spacer {
    flex: 1;
  }

  .version {
    color: var(--faint);
    font-size: 12px;
  }

  .status {
    font-family: var(--mono);
    font-size: 13px;
    font-weight: 700;
    padding: 4px 10px;
    border-radius: 999px;
    border: 1px solid currentColor;
  }

  .ready {
    color: var(--green);
  }

  .running {
    color: var(--purple);
  }
</style>
