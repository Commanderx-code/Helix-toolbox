<script lang="ts">
  import { modal } from '../lib/keys';
  import Changes from './Changes.svelte';
  import Icon from './Icon.svelte';
  import { toolbox } from '../lib/state.svelte';

  const prepared = $derived(toolbox.confirming?.prepared);
  /** Whether the change preview is shown instead of the steps. */
  let showingChanges = $state(false);

  $effect(() => {
    if (!prepared) showingChanges = false;
  });

  const TITLES = {
    nothing: 'Nothing to run',
    blocked: 'Cannot run yet',
    error: 'Cannot plan',
    ready: 'Run this plan?',
  };

  const tone = (line: string) =>
    line.startsWith('Blocked:') ? 'blocked' : line.startsWith('Warning:') ? 'warning' : line.startsWith('Risk:') ? 'risk' : '';
</script>

<svelte:window
  onkeydown={(event) => {
    if (!prepared) return;
    if (event.key !== 'Escape') return;
    if (showingChanges) showingChanges = false;
    else toolbox.decline();
  }}
/>

{#if prepared}
  <div class="backdrop" role="presentation">
    <div class="dialog card" class:wide={showingChanges} role="dialog" aria-modal="true" aria-labelledby="confirm-title" use:modal>
      <h2 id="confirm-title">{showingChanges ? 'What it changes' : TITLES[prepared.kind]}</h2>
      {#if prepared.kind === 'ready' && showingChanges}
        <Changes onback={() => (showingChanges = false)} />
      {:else if prepared.kind === 'ready'}
        <ul class="header">
          {#each prepared.header as line, index (index)}
            <li class={tone(line)}>
              {#if tone(line) === 'warning'}<Icon name="warning" size={15} />{:else if tone(line) === 'risk'}<Icon
                  name={prepared.privileged ? 'lock' : 'info'}
                  size={15}
                />{:else}<Icon name="info" size={15} />{/if}
              <span>{line}</span>
            </li>
          {/each}
        </ul>
        <div class="steps-title">Steps · {prepared.lines.filter((line) => line.detail).length}</div>
        <ol class="steps">
          {#each prepared.lines as line, index (index)}
            <li class:note={!line.detail}>
              <div class="name">{line.name}</div>
              {#if line.detail}<div class="mono command">{line.detail}</div>{/if}
            </li>
          {/each}
        </ol>
        <div class="buttons">
          <button class="button ghost changes" onclick={() => (showingChanges = true)}>
            <Icon name="eye" size={14} /> What it changes
          </button>
          <button class="button" data-autofocus onclick={() => toolbox.decline()}>Cancel</button>
          <button class="button primary" onclick={() => toolbox.confirm()}>
            <Icon name="play" size={14} /> Run
          </button>
        </div>
      {:else}
        <ul class="header">
          {#each prepared.lines as line, index (index)}
            <li class={tone(line)}><Icon name={prepared.kind === 'nothing' ? 'info' : 'warning'} size={15} /><span>{line}</span></li>
          {/each}
        </ul>
        {#if prepared.kind === 'blocked'}
          <p class="faint">Nothing was changed.</p>
        {/if}
        <div class="buttons">
          <button class="button primary" data-autofocus onclick={() => toolbox.decline()}>OK</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--backdrop);
    z-index: 10;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(760px, calc(100vw - 80px));
    max-height: calc(100vh - 80px);
    padding: 22px 24px;
    border-color: var(--selected-line);
    box-shadow: 0 24px 60px var(--shadow);
  }

  .dialog.wide {
    width: min(1100px, calc(100vw - 80px));
    height: calc(100vh - 80px);
  }

  h2 {
    margin: 0 0 14px;
    font-size: 19px;
    color: var(--purple);
  }

  .header {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .header li {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    color: var(--muted);
  }

  .header li :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .header .risk {
    color: var(--text);
    font-weight: 600;
  }

  .header .warning {
    color: var(--yellow);
  }

  .header .blocked {
    color: var(--red);
  }

  .steps-title {
    margin: 18px 0 8px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--faint);
  }

  .steps {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    list-style: none;
    counter-reset: step;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: var(--bg);
  }

  .steps li {
    padding: 9px 14px 9px 44px;
    border-bottom: 1px solid var(--line);
    position: relative;
  }

  .steps li:last-child {
    border-bottom: 0;
  }

  .steps li:not(.note)::before {
    counter-increment: step;
    content: counter(step);
    position: absolute;
    left: 14px;
    top: 9px;
    color: var(--cyan);
    font-family: var(--mono);
    font-weight: 700;
  }

  .steps li.note {
    color: var(--muted);
    padding-left: 14px;
  }

  .name {
    font-weight: 600;
  }

  .command {
    margin-top: 2px;
    color: var(--muted);
    font-size: 12px;
    word-break: break-all;
  }

  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
  }

  .buttons .changes {
    margin-right: auto;
    color: var(--cyan);
  }
</style>
