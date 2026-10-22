<script lang="ts">
  // The repositories dashboard and project workspaces, as `helix-toolbox
  // repos`: the repositories under the project folders, and one of them with
  // its changes, diffs, branches, stashes, tasks and recent commits. Reading
  // changes nothing; staging, commits, branches, stashes, syncing and tasks
  // are plans, reviewed first.
  import { untrack } from 'svelte';
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { RepoChange, RepoDiff, RepoFile, ReposView, RepositoryView } from '../lib/types';

  let found = $state<ReposView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  let open = $state<RepositoryView | null>(null);
  let openProblem = $state<string | null>(null);
  let diff = $state<{ file: string; staged: RepoDiff | null; unstaged: RepoDiff | null } | null>(null);
  let message = $state('');
  let branchName = $state('');
  let stashMessage = $state('');
  let taskCommand = $state('');
  let taskShell = $state<'direct' | 'fish' | 'bash'>('direct');

  async function load() {
    reading = true;
    try {
      found = await api.repositories();
      problem = null;
    } catch (error) {
      problem = String(error);
    } finally {
      reading = false;
    }
  }

  async function show(path: string) {
    try {
      open = await api.repository(path);
      openProblem = null;
      diff = null;
    } catch (error) {
      openProblem = String(error);
    }
  }

  $effect(() => {
    void toolbox.runs;
    load();
  });

  // After a run, the open repository has changed too.
  $effect(() => {
    void toolbox.runs;
    const path = untrack(() => open?.summary.path);
    if (path) show(path);
  });

  const sync = (repo: { upstream: string | null; ahead: number | null; behind: number | null }) => {
    if (!repo.upstream) return 'no upstream';
    const parts = [];
    if (repo.ahead) parts.push(`${repo.ahead} ahead`);
    if (repo.behind) parts.push(`${repo.behind} behind`);
    return parts.length ? parts.join(', ') : 'up to date';
  };

  const date = (unix: number | null) => (unix ? new Date(unix * 1000).toISOString().slice(0, 10) : '');

  function review(change: RepoChange) {
    const path = open?.summary.path;
    if (path) toolbox.reviewWith(() => api.prepareRepositoryChange(path, change));
  }

  async function showDiff(file: RepoFile) {
    if (!open) return;
    const path = open.summary.path;
    try {
      diff = {
        file: file.path,
        staged: file.staged ? await api.repositoryDiff(path, file.path, true) : null,
        unstaged: file.unstaged ? await api.repositoryDiff(path, file.path, false) : null,
      };
    } catch (error) {
      toolbox.notice = { title: 'No diff', text: String(error) };
    }
  }

  async function keep(change: Parameters<typeof api.saveWorkspace>[1], path: string) {
    try {
      await api.saveWorkspace(path, change);
      if (open?.summary.path === path) await show(path);
      await load();
    } catch (error) {
      toolbox.notice = { title: 'It was not kept', text: String(error) };
    }
  }

  function keepTask() {
    if (!open) return;
    const command = taskCommand.trim();
    const id =
      command
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .slice(0, 40) || 'task';
    keep({ task: { id, name: command.slice(0, 80), command, shell: taskShell } }, open.summary.path);
    taskCommand = '';
  }

  function push() {
    if (!open) return;
    if (open.push_to) return review({ kind: 'push' });
    const remote = open.remotes.includes('origin') ? 'origin' : open.remotes[0];
    if (remote) review({ kind: 'publish', remote });
  }

  const staged = $derived(open?.changes.filter((file) => file.staged).length ?? 0);
  const busy = $derived(toolbox.preparing || toolbox.busy);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Repositories</h1>
      <p class="screen-subtitle">The Git repositories under your project folders. Reading changes nothing; every change is reviewed first.</p>
    </div>
    <div class="screen-buttons">
      {#if open}
        <button class="button" onclick={() => (open = null)}><Icon name="apps" size={16} /> All repositories</button>
      {/if}
      <button class="button" disabled={reading} onclick={() => (open ? show(open.summary.path) : load())}>
        <Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}
      </button>
    </div>
  </div>

  {#if openProblem}
    <p class="card empty-note bad" role="alert">{openProblem}</p>
  {/if}

  {#if open}
    {@const repo = open}
    <section class="card part">
      <div class="head">
        <h2>{repo.summary.name}</h2>
        <span class="pill plain">{repo.summary.branch ?? 'detached'}</span>
        <span class="faint">{sync(repo.summary)}</span>
        <button
          class="star"
          aria-pressed={repo.favorite}
          aria-label={repo.favorite ? 'No longer a favorite' : 'Mark as a favorite'}
          onclick={() => keep({ favorite: !repo.favorite }, repo.summary.path)}
        >
          <Icon name="star" size={16} />
        </button>
      </div>
      <p class="muted mono">{repo.summary.path}</p>
      {#if repo.summary.origin}<p class="muted mono">origin {repo.summary.origin}</p>{/if}
      {#if repo.busy}<p class="said bad">In the middle of {repo.busy}: finish it with git.</p>{/if}
      {#if repo.remotes.length > 0}
        <div class="actions">
          <button class="button" disabled={busy} onclick={() => review({ kind: 'fetch' })}><Icon name="refresh" size={14} /> Fetch…</button>
          <button class="button" disabled={busy || !repo.summary.upstream} onclick={() => review({ kind: 'pull' })}><Icon name="down" size={14} /> Pull…</button>
          <button class="button" disabled={busy || !repo.summary.branch} onclick={push}>
            <Icon name="up" size={14} /> {repo.push_to ? 'Push…' : 'Publish…'}
          </button>
        </div>
      {/if}
    </section>

    <section class="card part">
      <div class="head">
        <h2>Changes</h2>
        <span class="faint">{repo.changes.length} changed, {staged} staged</span>
      </div>
      {#if repo.changes.length === 0}
        <p class="muted">No changes.</p>
      {:else}
        <ul class="items" aria-label="Changes">
          {#each repo.changes as file (file.path)}
            <li>
              <span class="pill {file.conflict ? 'problem' : file.staged && !file.unstaged ? 'good' : 'notice'}">{file.state}</span>
              <span class="name mono">{file.path}{file.original ? ` (was ${file.original})` : ''}</span>
              <span class="row-buttons">
                <button class="button small" onclick={() => showDiff(file)}>Diff</button>
                {#if file.unstaged || file.conflict}
                  <button class="button small" disabled={busy} onclick={() => review({ kind: 'stage', files: [file.path] })}>Stage…</button>
                {:else}
                  <button class="button small" disabled={busy} onclick={() => review({ kind: 'unstage', files: [file.path] })}>Unstage…</button>
                {/if}
              </span>
            </li>
          {/each}
        </ul>
        <div class="actions">
          <button class="button" disabled={busy} onclick={() => review({ kind: 'stage_all' })}>Stage every change…</button>
        </div>
      {/if}
      {#if diff}
        {@const shown = diff}
        <div class="diff">
          <div class="head">
            <h3 class="mono">{shown.file}</h3>
            <button class="button small" onclick={() => (diff = null)}>Close</button>
          </div>
          {#if shown.staged}<p class="label">Staged</p><pre aria-label="Staged diff">{shown.staged.text}</pre>{/if}
          {#if shown.unstaged}<p class="label">Not staged</p><pre aria-label="Diff">{shown.unstaged.text}</pre>{/if}
        </div>
      {/if}
      <label class="field">
        <span class="label">Commit message</span>
        <textarea bind:value={message} rows="2" placeholder="What the staged changes do"></textarea>
      </label>
      <div class="actions">
        <button
          class="button primary"
          disabled={busy || staged === 0 || message.trim() === ''}
          onclick={() => review({ kind: 'commit', message })}
        >
          <Icon name="check" size={14} /> Commit {staged}…
        </button>
      </div>
    </section>

    <section class="card part">
      <h2>Branches and stashes</h2>
      <ul class="items" aria-label="Branches">
        {#each repo.branches as branch (branch)}
          <li>
            <span class="pill {branch === repo.summary.branch ? 'good' : 'plain'}">{branch === repo.summary.branch ? 'current' : 'branch'}</span>
            <span class="name mono">{branch}</span>
            <span class="row-buttons">
              {#if branch !== repo.summary.branch}
                <button class="button small" disabled={busy} onclick={() => review({ kind: 'switch_branch', name: branch })}>Switch…</button>
              {/if}
            </span>
          </li>
        {/each}
        {#each repo.stashes as stash (stash.id)}
          <li>
            <span class="pill plain">stash</span>
            <span class="name">{stash.subject}</span>
            <span class="row-buttons">
              <button class="button small" disabled={busy} onclick={() => review({ kind: 'apply_stash', id: stash.id })}>Apply…</button>
            </span>
          </li>
        {/each}
      </ul>
      <div class="inline">
        <label class="field">
          <span class="label">A new branch</span>
          <input class="mono" bind:value={branchName} spellcheck="false" />
        </label>
        <button class="button" disabled={busy || branchName.trim() === ''} onclick={() => review({ kind: 'create_branch', name: branchName.trim() })}>
          Create…
        </button>
      </div>
      <div class="inline">
        <label class="field">
          <span class="label">A stash's message</span>
          <input bind:value={stashMessage} />
        </label>
        <button
          class="button"
          disabled={busy || repo.changes.length === 0}
          onclick={() => review({ kind: 'stash', message: stashMessage, untracked: true })}
        >
          Stash…
        </button>
      </div>
    </section>

    <section class="card part">
      <h2>Tasks</h2>
      {#if repo.tasks.length === 0}
        <p class="muted">None found in its manifests. Keep a command of yours below.</p>
      {:else}
        <ul class="items" aria-label="Tasks">
          {#each repo.tasks as task (task.id)}
            <li>
              <span class="pill {task.detected ? 'plain' : 'notice'}">{task.detected ? 'found' : 'yours'}</span>
              <span>
                <span class="name">{task.name}</span>
                <span class="sub mono">{task.shell === 'direct' ? '' : `${task.shell}: `}{task.command}</span>
              </span>
              <span class="row-buttons">
                <button class="button small" disabled={busy} onclick={() => toolbox.reviewWith(() => api.prepareTask(repo.summary.path, task.id))}>
                  Run…
                </button>
                {#if !task.detected}
                  <button class="button small" onclick={() => keep({ remove: task.id }, repo.summary.path)}>Remove</button>
                {/if}
              </span>
            </li>
          {/each}
        </ul>
      {/if}
      <div class="inline">
        <label class="field">
          <span class="label">A command of yours, run in its folder</span>
          <input class="mono" bind:value={taskCommand} placeholder="npm run serve" spellcheck="false" />
        </label>
        <label class="field narrow">
          <span class="label">Run with</span>
          <select bind:value={taskShell}>
            <option value="direct">As it is</option>
            <option value="fish">fish</option>
            <option value="bash">bash</option>
          </select>
        </label>
        <button class="button" disabled={taskCommand.trim() === ''} onclick={keepTask}>Keep</button>
      </div>
    </section>

    {#if repo.log.length > 0}
      <section class="card part">
        <h2>Recent commits</h2>
        <ul class="items" aria-label="Recent commits">
          {#each repo.log as commit (commit.hash)}
            <li>
              <span class="mono faint">{commit.hash}</span>
              <span class="name">{commit.subject}</span>
              <span class="faint">{date(commit.at)}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {:else if problem}
    <p class="card empty-note bad" role="alert">{problem}</p>
  {:else if found === null}
    <p class="card empty-note">Looking for repositories…</p>
  {:else}
    <section class="card part">
      {#if found.repositories.length === 0}
        <p>No repositories found.</p>
      {:else}
        <ul class="items" aria-label="Repositories">
          {#each found.repositories as repo (repo.path)}
            <li class="repo">
              <button
                class="star"
                aria-pressed={repo.favorite}
                aria-label={repo.favorite ? `${repo.name} is a favorite` : `Mark ${repo.name} as a favorite`}
                onclick={() => keep({ favorite: !repo.favorite }, repo.path)}
              >
                <Icon name="star" size={15} />
              </button>
              <button class="open" onclick={() => show(repo.path)}>
                <span class="name">{repo.name}</span>
                <span class="sub">{repo.problem ?? repo.last ?? ''}</span>
              </button>
              <span class="pill plain">{repo.branch ?? 'detached'}</span>
              <span class="pill {repo.problem ? 'problem' : repo.changed ? 'notice' : 'good'}">
                {repo.problem ? 'unread' : repo.changed ? `${repo.changed} changed` : 'clean'}
              </span>
              <span class="faint">{sync(repo)}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <p class="muted">
        Under {found.roots.length ? found.roots.join(', ') : 'no folders'}, {found.depth} folders deep (from the {found.origin}).
        <span class="mono">[repositories] roots</span> in settings.toml changes it.
      </p>
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
    flex-wrap: wrap;
    gap: 10px;
    align-items: baseline;
  }

  .head h2,
  .head h3 {
    margin: 0;
  }

  .items {
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .items li {
    display: grid;
    grid-template-columns: 90px minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 7px 0;
    border-top: 1px solid var(--line);
  }

  .items li.repo {
    grid-template-columns: 28px minmax(0, 1fr) auto auto 120px;
  }

  .items li > span:nth-child(2) {
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
    overflow-wrap: anywhere;
  }

  .open {
    display: grid;
    gap: 2px;
    min-width: 0;
    padding: 0;
    border: 0;
    background: none;
    color: inherit;
    text-align: left;
    cursor: pointer;
  }

  .star {
    display: grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: var(--radius-small);
    background: none;
    color: var(--muted);
    cursor: pointer;
  }

  .star[aria-pressed='true'] {
    color: var(--accent);
  }

  .small {
    height: 28px;
    padding: 0 10px;
    font-size: 12px;
  }

  .row-buttons,
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .actions {
    margin-top: 10px;
  }

  .inline {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: flex-end;
    margin-top: 10px;
  }

  .field {
    display: grid;
    flex: 1;
    gap: 6px;
    min-width: 220px;
    margin-top: 12px;
  }

  .field.narrow {
    flex: 0 0 auto;
    min-width: 120px;
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .field input,
  .field select,
  .field textarea {
    min-height: 34px;
    padding: 6px 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: inherit;
    font: inherit;
    font-size: 13px;
  }

  .diff {
    margin-top: 12px;
  }

  .diff pre {
    max-height: 360px;
    overflow: auto;
    padding: 10px;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: var(--bg);
    font-size: 12px;
  }

  .bad {
    color: var(--red);
  }
</style>
