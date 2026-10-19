<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { BackupsView } from '../lib/types';

  let view = $state<BackupsView | null>(null);
  let problem = $state<string | null>(null);
  let selectedIndex = $state<number | null>(null);

  const selected = $derived(view?.backups.find((backup) => backup.index === selectedIndex) ?? null);

  async function load() {
    try {
      view = await api.backups();
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  load();
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Backups</h1>
      <p class="screen-subtitle">Copies that entries and restores left beside the files they changed, newest first.</p>
    </div>
    <button class="button" onclick={load}><Icon name="refresh" size={16} /> Look again</button>
  </div>

  {#if view?.snapshots}
    <div class="card snapshots">
      <Icon name="backups" />
      <span><strong>{view.snapshots.tool}</strong> takes a snapshot before system changes (Settings turns that off). {view.snapshots.restore}</span>
    </div>
  {/if}

  <div class="split">
    <div class="pane card">
      {#if problem}
        <p class="empty-note bad">{problem}</p>
      {:else if view === null}
        <p class="empty-note">Looking for backups…</p>
      {:else if view.backups.length === 0}
        <p class="empty-note">No backups found. Entries that change a file keep the old one beside it, and they appear here.</p>
      {:else}
        <div class="rows">
          {#each view.backups as backup (backup.index)}
            <button class="row-item backup" class:selected={selectedIndex === backup.index} onclick={() => (selectedIndex = backup.index)}>
              <span class="faint"><Icon name={backup.directory ? 'folder' : 'backups'} size={16} /></span>
              <span>
                <span class="name mono">{backup.original}</span>
                <span class="sub">{backup.taken ?? 'taken at an unknown time'}{backup.system ? ' · system file' : ''}</span>
              </span>
            </button>
          {/each}
        </div>
      {/if}
    </div>

    <aside class="pane side card">
      {#if selected}
        <h2>Backup</h2>
        <dl class="kv">
          <dt>Of</dt>
          <dd class="mono">{selected.original}</dd>
          <dt>Taken</dt>
          <dd>{selected.taken ?? 'unknown'}</dd>
          <dt>Copy</dt>
          <dd class="mono">{selected.backup}</dd>
          <dt>Kind</dt>
          <dd>{selected.directory ? 'a folder' : 'a file'}, {selected.system ? 'a system file (restoring asks for your password)' : 'your own file'}</dd>
        </dl>
        <p class="muted">
          Restoring moves the current {selected.directory ? 'folder' : 'file'} aside as its own backup first, so the restore can be undone the same way.
        </p>
        <div class="actions">
          <button
            class="button primary"
            disabled={toolbox.preparing || toolbox.busy}
            onclick={() => toolbox.reviewWith(() => api.prepareRestore(selected.index))}
          >
            <Icon name="play" size={14} /> Restore…
          </button>
        </div>
      {:else}
        <p class="faint">Select a backup to see where it came from.</p>
      {/if}
    </aside>
  </div>
</div>

<style>
  .snapshots {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 14px;
    color: var(--cyan);
  }

  .snapshots span {
    color: var(--muted);
  }

  .backup {
    grid-template-columns: 22px minmax(0, 1fr);
  }

  .backup > span:nth-child(2) {
    display: grid;
    min-width: 0;
  }

  .backup .name {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .kv dd.mono {
    font-size: 12.5px;
    word-break: break-all;
    white-space: normal;
  }

  .bad {
    color: var(--red);
  }
</style>
