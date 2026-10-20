<script lang="ts">
  import ConfirmDialog from './components/ConfirmDialog.svelte';
  import Header from './components/Header.svelte';
  import Icon from './components/Icon.svelte';
  import RunPanel from './components/RunPanel.svelte';
  import Sidebar from './components/Sidebar.svelte';
  import { toolbox } from './lib/state.svelte';
  import Area from './screens/Area.svelte';
  import Dashboard from './screens/Dashboard.svelte';
  import Favorites from './screens/Favorites.svelte';
  import History from './screens/History.svelte';
  import Plan from './screens/Plan.svelte';
  import Doctor from './screens/Doctor.svelte';
  import Updates from './screens/Updates.svelte';
  import Backups from './screens/Backups.svelte';
  import Drift from './screens/Drift.svelte';
  import Reliability from './screens/Reliability.svelte';
  import Hardware from './screens/Hardware.svelte';
  import Desktop from './screens/Desktop.svelte';
  import Settings from './screens/Settings.svelte';
  import Dialogs from './components/Dialogs.svelte';
  import Palette from './components/Palette.svelte';
  import Wizard from './components/Wizard.svelte';

  toolbox.load();

  /** Ctrl-K (or Cmd-K) opens the command palette from anywhere. */
  function onkeydown(event: KeyboardEvent) {
    if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
      event.preventDefault();
      if (toolbox.prompt || toolbox.confirming || toolbox.running || toolbox.wizard) return;
      toolbox.palette = !toolbox.palette;
    }
  }
</script>

<svelte:window {onkeydown} />

<div class="app">
  <Header />
  <div class="middle">
    <Sidebar />
    <main>
      {#if toolbox.screen.kind === 'dashboard'}
        <Dashboard />
      {:else if toolbox.screen.kind === 'area'}
        <Area index={toolbox.screen.index} />
      {:else if toolbox.screen.kind === 'favorites'}
        <Favorites />
      {:else if toolbox.screen.kind === 'plan'}
        <Plan />
      {:else if toolbox.screen.kind === 'history'}
        <History />
      {:else if toolbox.screen.kind === 'doctor'}
        <Doctor />
      {:else if toolbox.screen.kind === 'updates'}
        <Updates />
      {:else if toolbox.screen.kind === 'backups'}
        <Backups />
      {:else if toolbox.screen.kind === 'drift'}
        <Drift />
      {:else if toolbox.screen.kind === 'reliability'}
        <Reliability />
      {:else if toolbox.screen.kind === 'hardware'}
        <Hardware />
      {:else if toolbox.screen.kind === 'desktop'}
        <Desktop />
      {:else if toolbox.screen.kind === 'settings'}
        <Settings />
      {/if}
    </main>
  </div>
  <footer>
    <span>
      {#if toolbox.checking}
        <span class="spin"><Icon name="refresh" size={13} /></span> Checking what is installed…
      {:else}
        {Object.keys(toolbox.statuses).length} package entries checked
      {/if}
    </span>
    <button class="button ghost small" disabled={toolbox.checking} onclick={() => toolbox.refresh()}>
      <Icon name="refresh" size={14} /> Check again
    </button>
    <span class="spacer"></span>
    <span class="faint">Every change is planned and confirmed first.</span>
  </footer>
</div>

{#if toolbox.error}
  <div class="toast" role="alert">
    <Icon name="warning" />
    <span>{toolbox.error}</span>
    <button class="button ghost small" onclick={() => (toolbox.error = null)}><Icon name="x" size={14} /></button>
  </div>
{/if}

{#if toolbox.palette}
  <Palette />
{/if}
{#if toolbox.wizard}
  <Wizard />
{/if}
<Dialogs />
<ConfirmDialog />
{#if toolbox.running}
  <RunPanel />
{/if}

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .middle {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  main {
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  footer {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 36px;
    padding: 0 16px;
    border-top: 1px solid var(--line-strong);
    color: var(--muted);
    font-size: 12px;
  }

  footer > span:first-child {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .spacer {
    flex: 1;
  }

  .small {
    height: 26px;
    padding: 0 8px;
    font-size: 12px;
  }

  .spin {
    display: inline-flex;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .toast {
    position: fixed;
    right: 20px;
    bottom: 50px;
    display: flex;
    align-items: center;
    gap: 10px;
    max-width: 520px;
    padding: 10px 12px 10px 14px;
    border-radius: var(--radius);
    border: 1px solid var(--red);
    background: var(--panel);
    color: var(--red);
    box-shadow: 0 12px 30px rgba(0, 0, 0, 0.4);
    z-index: 30;
  }

  .toast span {
    color: var(--text);
  }
</style>
