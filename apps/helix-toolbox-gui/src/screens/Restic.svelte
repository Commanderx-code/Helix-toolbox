<script lang="ts">
  // Restic backups of your own files, as `helix-toolbox restic`: the
  // repository and its snapshots, the files in one, a file's history, and
  // recovery tests against a SHA-256 recorded earlier. Reading only asks
  // restic; checking, backing up, restoring and testing are plans, reviewed
  // first. Restores go into a new folder, so nothing is overwritten.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { ResticHistory, ResticNode, ResticSnapshot, ResticView } from '../lib/types';

  let found = $state<ResticView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);

  // Browsing one snapshot.
  let browsing = $state<ResticSnapshot | null>(null);
  let folder = $state('/');
  let nodes = $state<ResticNode[]>([]);
  let listing = $state(false);
  let listProblem = $state<string | null>(null);

  // What a restore takes.
  let restoreFrom = $state('');
  let include = $state('');
  let target = $state('');

  // A file's history.
  let pattern = $state('');
  let ignoreCase = $state(false);
  let searching = $state(false);
  let history = $state<ResticHistory[] | null>(null);
  let searchProblem = $state<string | null>(null);

  // Recovery tests.
  let baselineFile = $state('');
  let testSnapshot = $state('latest');
  let said = $state<string | null>(null);

  async function load() {
    reading = true;
    try {
      found = await api.restic();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  $effect(() => {
    void toolbox.runs;
    load();
  });

  async function browse(snapshot: ResticSnapshot, path: string) {
    browsing = snapshot;
    folder = path;
    listing = true;
    try {
      nodes = await api.resticFiles(snapshot.id, path);
      listProblem = null;
    } catch (error) {
      nodes = [];
      listProblem = String(error);
    } finally {
      listing = false;
    }
  }

  function up() {
    if (!browsing || folder === '/') return;
    const parent = folder.replace(/\/[^/]+\/?$/, '') || '/';
    browse(browsing, parent);
  }

  function restoreThis(snapshot: string, path: string) {
    restoreFrom = snapshot;
    include = path;
    if (target === '') target = '~/restored';
  }

  async function search() {
    searching = true;
    try {
      history = await api.resticHistory(pattern.trim(), ignoreCase);
      searchProblem = null;
    } catch (error) {
      history = null;
      searchProblem = String(error);
    } finally {
      searching = false;
    }
  }

  async function record() {
    try {
      const recorded = await api.recordBaseline(baselineFile.trim());
      said = `Recorded ${recorded.path}: SHA-256 ${recorded.sha256}. Back it up, then test a restore of it.`;
      baselineFile = '';
      await load();
    } catch (error) {
      said = String(error);
    }
  }

  async function timer(on: boolean, weekly = false) {
    try {
      toolbox.notice = { title: 'Timed backup', text: await api.setResticTimer(on, weekly) };
      await load();
    } catch (error) {
      toolbox.notice = { title: 'The timer did not change', text: String(error) };
    }
  }

  const when = (time: string) => time.replace('T', ' ').slice(0, 16);
  const bytes = (size: number | null | undefined) =>
    size == null ? '' : size < 1024 ? `${size} B` : size < 1048576 ? `${(size / 1024).toFixed(1)} KiB` : `${(size / 1048576).toFixed(1)} MiB`;
  const busy = $derived(toolbox.preparing || toolbox.busy);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Restic</h1>
      <p class="screen-subtitle">Backups of your own files: the snapshots, a file's history, restores into a new folder and recovery tests.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  <section class="card part">
    <h2>The repository</h2>
    {#if found === null && !problem}
      <p class="muted">Asking restic…</p>
    {:else if found && !found.configured && !found.problem}
      <p>No restic repository is set up. Add it to <span class="mono">settings.toml</span> in Helix Toolbox's settings folder:</p>
      <pre class="mono">[restic]
repository = "/path/to/the/repository"
password_file = "~/.config/restic/password"
# paths = ["~/Documents"]   (the home folder when left out)</pre>
      <p class="muted">restic's own RESTIC_REPOSITORY and RESTIC_PASSWORD_FILE are read too. Helix Toolbox never keeps a password.</p>
    {:else if found}
      {#if found.repository}
        <dl class="facts">
          <dt>Repository</dt><dd class="mono">{found.repository}</dd>
          <dt>From</dt><dd>{found.origin}</dd>
          <dt>Password</dt><dd>from {found.password}</dd>
          <dt>Backs up</dt><dd class="mono">{found.paths.join(', ')}</dd>
          {#if found.excludes.length > 0}<dt>Leaves out</dt><dd class="mono">{found.excludes.join(', ')}</dd>{/if}
          <dt>Timer</dt><dd>{found.timer ?? 'off'}</dd>
        </dl>
        <div class="actions">
          <button class="button primary" disabled={busy} onclick={() => toolbox.reviewWith(() => api.prepareRestic('backup'))}><Icon name="save" size={14} /> Back up…</button>
          <button class="button" disabled={busy} onclick={() => toolbox.reviewWith(() => api.prepareRestic('check'))}><Icon name="check" size={14} /> Check…</button>
          {#if found.timer}
            <button class="button" onclick={() => timer(false)}>Stop the timer</button>
          {:else}
            <button class="button" onclick={() => timer(true)}>Back up every day</button>
            <button class="button" onclick={() => timer(true, true)}>Every week</button>
          {/if}
        </div>
      {/if}
      {#if found.problem}
        <p class="said bad" role="alert"><Icon name="warning" size={15} /> {found.problem}</p>
      {/if}
    {/if}
    {#if problem}
      <p class="said bad" role="alert">{problem}</p>
    {/if}
  </section>

  {#if found && found.configured}
    <section class="card part">
      <h2>Snapshots ({found.snapshots.length})</h2>
      <ul class="items" aria-label="Snapshots">
        {#each found.snapshots as snapshot (snapshot.id)}
          <li>
            <span class="mono">{snapshot.short_id}</span>
            <span>
              <span class="name">{when(snapshot.time)} · {snapshot.hostname}</span>
              <span class="sub mono">{snapshot.paths.join(', ')}</span>
            </span>
            <span class="row-buttons">
              <button class="button" onclick={() => browse(snapshot, snapshot.paths[0] ?? '/')}><Icon name="folder" size={14} /> Browse</button>
              <button class="button" onclick={() => restoreThis(snapshot.id, '')}>Restore…</button>
            </span>
          </li>
        {/each}
      </ul>
    </section>

    {#if browsing}
      <section class="card part">
        <div class="head">
          <h2>In {browsing.short_id}</h2>
          <span class="faint mono">{folder}</span>
        </div>
        <div class="actions">
          <button class="button" disabled={folder === '/' || listing} onclick={up}>Up</button>
        </div>
        {#if listProblem}
          <p class="said bad" role="alert">{listProblem}</p>
        {:else if listing}
          <p class="muted">Listing…</p>
        {:else}
          <ul class="items" aria-label="Files in the snapshot">
            {#each nodes as node (node.path)}
              <li>
                <span class="pill plain">{node.type === 'dir' ? 'Folder' : 'File'}</span>
                <span>
                  <span class="name">{node.name}</span>
                  <span class="sub">{bytes(node.size)}</span>
                </span>
                <span class="row-buttons">
                  {#if node.type === 'dir'}
                    <button class="button" onclick={() => browsing && browse(browsing, node.path)}>Open</button>
                  {/if}
                  <button class="button" onclick={() => browsing && restoreThis(browsing.id, node.path)}>Restore this…</button>
                </span>
              </li>
            {:else}
              <li class="muted">Nothing here.</li>
            {/each}
          </ul>
        {/if}
      </section>
    {/if}

    <section class="card part">
      <h2>Restore into a new folder</h2>
      <p class="muted">Nothing is overwritten: the folder must not be there yet, in your home folder. What is restored is read back.</p>
      {#if !found.restores}
        <p class="said bad">This restic cannot restore without overwriting; restic 0.17 or later can.</p>
      {/if}
      <div class="fields">
        <label class="field"><span class="label">Snapshot</span><input class="mono" bind:value={restoreFrom} placeholder="latest" spellcheck="false" /></label>
        <label class="field"><span class="label">Only this path (optional)</span><input class="mono" bind:value={include} placeholder="/home/me/Documents" spellcheck="false" /></label>
        <label class="field"><span class="label">The new folder</span><input class="mono" bind:value={target} placeholder="~/restored" spellcheck="false" /></label>
      </div>
      <div class="actions">
        <button
          class="button primary"
          disabled={busy || target.trim() === '' || !found.restores}
          onclick={() => toolbox.reviewWith(() => api.prepareRestic('restore', { snapshot: restoreFrom.trim() || 'latest', target: target.trim(), include: include.trim() }))}
        >
          <Icon name="play" size={14} /> Restore…
        </button>
      </div>
    </section>

    <section class="card part">
      <h2>A file's history</h2>
      <div class="fields">
        <label class="field"><span class="label">A file name or path, with * wildcards</span><input class="mono" bind:value={pattern} placeholder="notes.txt" spellcheck="false" /></label>
      </div>
      <label class="check"><input type="checkbox" bind:checked={ignoreCase} /> Ignore upper and lower case</label>
      <div class="actions">
        <button class="button" disabled={pattern.trim() === '' || searching} onclick={search}><Icon name="search" size={14} /> {searching ? 'Looking…' : 'Look for it'}</button>
      </div>
      {#if searchProblem}
        <p class="said bad" role="alert">{searchProblem}</p>
      {:else if history && history.length === 0}
        <p class="muted">No snapshot has a file that matches.</p>
      {:else if history}
        {#each history as file (file.path)}
          <h3 class="mono">{file.path}</h3>
          {#if file.missing_from > 0}
            <p class="muted">Not in the {file.missing_from} newer snapshot(s) of the same folder: deleted or moved since.</p>
          {/if}
          <ul class="items" aria-label={`Versions of ${file.path}`}>
            {#each file.versions as version (version.snapshot)}
              <li>
                <span class="pill {version.change === 'same' ? 'plain' : 'notice'}">{version.change === 'first' ? 'First' : version.change === 'changed' ? 'Changed' : 'Same'}</span>
                <span>
                  <span class="name">{when(version.time)} · {version.short_id}</span>
                  <span class="sub">{bytes(version.size)}</span>
                </span>
                <span class="row-buttons">
                  <button class="button" onclick={() => restoreThis(version.snapshot, file.path)}>Restore this version…</button>
                </span>
              </li>
            {/each}
          </ul>
        {/each}
      {/if}
    </section>

    <section class="card part">
      <h2>Recovery tests</h2>
      <p class="muted">Record a file's SHA-256, back it up, then restore it from a snapshot into a temporary folder and compare.</p>
      <div class="fields">
        <label class="field"><span class="label">A file of your home folder</span><input class="mono" bind:value={baselineFile} placeholder="~/Documents/taxes.ods" spellcheck="false" /></label>
      </div>
      <div class="actions">
        <button class="button" disabled={baselineFile.trim() === ''} onclick={record}>Record its SHA-256</button>
      </div>
      {#if said}<p class="muted" role="status">{said}</p>{/if}
      {#if found.baselines.length > 0}
        <label class="field narrow">
          <span class="label">Test against the snapshot</span>
          <select bind:value={testSnapshot}>
            <option value="latest">The latest</option>
            {#each found.snapshots as snapshot (snapshot.id)}<option value={snapshot.id}>{snapshot.short_id} · {when(snapshot.time)}</option>{/each}
          </select>
        </label>
        <ul class="items" aria-label="Baselines">
          {#each found.baselines as baseline (baseline.id)}
            <li>
              <span class="mono">{baseline.sha256.slice(0, 12)}</span>
              <span>
                <span class="name mono">{baseline.path}</span>
                <span class="sub">Recorded {new Date(baseline.recorded * 1000).toISOString().slice(0, 16).replace('T', ' ')} UTC</span>
              </span>
              <span class="row-buttons">
                <button class="button" disabled={busy} onclick={() => toolbox.reviewWith(() => api.prepareRestic('test', { baseline: baseline.id, snapshot: testSnapshot }))}>Test…</button>
              </span>
            </li>
          {/each}
        </ul>
      {/if}
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
    overflow-wrap: anywhere;
  }

  pre {
    margin: 8px 0;
    padding: 10px 12px;
    border-radius: var(--radius-small);
    background: var(--bg);
    font-size: 12px;
    white-space: pre-wrap;
  }

  .facts {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 4px 14px;
    margin: 0 0 6px;
  }

  .facts dt {
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
  }

  .facts dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .fields {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 10px;
  }

  .field {
    display: grid;
    gap: 6px;
    margin: 10px 0 4px;
  }

  .field.narrow {
    max-width: 320px;
  }

  .field .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .field input,
  .field select {
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    font-size: 13px;
  }

  .check {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 8px 0;
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
    grid-template-columns: 96px minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }

  .items li > span:nth-child(2) {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .row-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    justify-content: flex-end;
  }

  .name {
    font-weight: 600;
    overflow-wrap: anywhere;
  }

  .sub {
    color: var(--muted);
    font-size: 12px;
    overflow-wrap: anywhere;
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
