<script lang="ts">
  // The migration hub, as `helix-toolbox migrate`: what this machine has,
  // exported to a file to take to another machine or distribution, and
  // such a file read here. Catalog entries and Flatpaks translate; the
  // other packages are left to you when the families differ. Exporting
  // only reads; installing is a plan, reviewed first.
  import { untrack } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { MigrationExportView, Standing, TranslationView } from '../lib/types';

  let here = $state<MigrationExportView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  let exporting = $state(false);
  let file = $state('');
  let opened = $state<TranslationView | null>(null);
  let openProblem = $state<string | null>(null);
  let opening = $state(false);

  async function load() {
    reading = true;
    try {
      here = await api.migrationExport();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  $effect(() => {
    void toolbox.readAgain.tools;
    load();
  });

  // After a run, what the file means here has changed too.
  $effect(() => {
    void toolbox.readAgain.tools;
    const path = untrack(() => opened?.path);
    if (path) read(path);
  });

  async function exportIt() {
    exporting = true;
    try {
      const said = await api.exportMigration();
      if (said) toolbox.notice = { title: 'Migration exported', text: said };
    } catch (error) {
      toolbox.notice = { title: 'The migration was not exported', text: String(error) };
    } finally {
      exporting = false;
    }
  }

  async function read(path: string | null) {
    opening = true;
    try {
      const found = await api.openMigration(path);
      if (found) {
        opened = found;
        file = found.path;
        openProblem = null;
      }
    } catch (error) {
      openProblem = String(error);
    } finally {
      opening = false;
    }
  }

  const GROUPS: { standing: Standing; title: string; pill: string }[] = [
    { standing: 'install', title: 'To install', pill: 'notice' },
    { standing: 'here', title: 'Here already', pill: 'good' },
    { standing: 'unavailable', title: 'Cannot be installed here', pill: 'problem' },
    { standing: 'yours', title: 'Left to you', pill: 'plain' },
  ];

  const KINDS = { entry: 'Entry', flatpak: 'Flatpak', package: 'Package' } as const;
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Migration</h1>
      <p class="screen-subtitle">Take this machine to another one or another distribution: export what it has, then open the file there.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  <section class="card part">
    <h2>This machine</h2>
    {#if problem}
      <p class="said bad" role="alert">{problem}</p>
    {:else if here === null}
      <p class="muted">Reading what is installed…</p>
    {:else}
      <p>
        {here.migration.from}: {here.migration.entries.length} catalog entries, {here.migration.flatpaks.length} Flatpaks and
        {here.migration.packages.length} packages you installed that no entry covers.
      </p>
      {#if !here.packages_known}
        <p class="muted">The packages you installed yourself cannot be told apart here, so only the entries and Flatpaks go in the file.</p>
      {/if}
      <p class="muted">It only reads. The file is suggested as <span class="mono">{here.file_name}</span>.</p>
      <div class="actions">
        <button class="button primary" disabled={exporting} onclick={exportIt}><Icon name="save" size={14} /> Export…</button>
      </div>
    {/if}
  </section>

  <section class="card part">
    <h2>Bring a migration here</h2>
    <p class="muted">
      Entries install with this distribution's package for them, Flatpaks from Flathub. Packages from another family are listed for you,
      with whether a package of the same name is here.
    </p>
    <label class="field">
      <span class="label">The migration file</span>
      <input class="mono" bind:value={file} placeholder="/home/me/helix-toolbox-migration.toml" spellcheck="false" />
    </label>
    <div class="actions">
      <button class="button" disabled={opening} onclick={() => read(null)}><Icon name="folder" size={14} /> Open…</button>
      <button class="button" disabled={file.trim() === '' || opening} onclick={() => read(file.trim())}>
        <Icon name="search" size={14} /> {opening ? 'Reading…' : 'Read it'}
      </button>
    </div>
    {#if openProblem}
      <p class="said bad" role="alert"><Icon name="warning" size={15} /> {openProblem}</p>
    {/if}
  </section>

  {#if opened}
    {@const translation = opened}
    <section class="card part">
      <div class="head">
        <h2>From {translation.migration.from}</h2>
        <span class="faint">{translation.migration.architecture}{translation.same_family ? ' · the same family' : ''}</span>
      </div>
      <div class="actions">
        <button
          class="button primary"
          disabled={translation.install === 0 || toolbox.preparing || toolbox.busy}
          onclick={() => toolbox.reviewWith(() => api.prepareMigration(translation.path))}
        >
          <Icon name="play" size={14} /> Install {translation.install}…
        </button>
      </div>
      {#each GROUPS as group (group.standing)}
        {@const items = translation.items.filter((item) => item.standing === group.standing)}
        {#if items.length > 0}
          <h3>{group.title} ({items.length})</h3>
          <ul class="items" aria-label={group.title}>
            {#each items as item (`${item.kind}:${item.id}`)}
              <li>
                <span class="pill {group.pill}">{KINDS[item.kind]}</span>
                <span>
                  <span class="name">{item.name}</span>
                  <span class="sub">{item.detail}</span>
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    </section>
  {/if}
</div>

<style>
  .screen {
    overflow-y: auto;
  }

  .part {
    padding: 16px 18px;
  }

  .part h2 {
    margin: 0 0 10px;
  }

  .part p {
    margin: 6px 0;
  }

  .head {
    display: flex;
    gap: 10px;
    align-items: baseline;
  }

  .head h2 {
    margin: 0;
  }

  h3 {
    margin: 16px 0 4px;
    font-size: 13px;
  }

  .field {
    display: grid;
    gap: 6px;
    margin: 14px 0 10px;
  }

  .field .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .field input {
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    font-size: 13px;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 8px;
  }

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .items li {
    display: grid;
    grid-template-columns: 80px minmax(0, 1fr);
    gap: 12px;
    align-items: center;
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }

  .items li > span:last-child {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .sub {
    color: var(--muted);
    font-size: 12px;
  }

  .said {
    display: flex;
    gap: 6px;
    align-items: center;
  }

  .bad {
    color: var(--red);
  }
</style>
