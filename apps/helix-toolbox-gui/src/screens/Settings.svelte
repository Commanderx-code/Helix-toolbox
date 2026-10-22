<script lang="ts">
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import { applyTheme } from '../lib/theme';
  import type { SettingsView, WindowTheme } from '../lib/types';

  const THEMES: { theme: WindowTheme; title: string; detail: string }[] = [
    { theme: 'system', title: 'Follow the desktop', detail: "Light or dark, as the desktop's color scheme is, and changing with it." },
    { theme: 'dark', title: 'Dark', detail: "The terminal interface's colors." },
    { theme: 'light', title: 'Light', detail: 'Dark text on light panels.' },
  ];

  let view = $state<SettingsView | null>(null);
  let problem = $state<string | null>(null);
  let keepGoing = $state(false);
  let snapshots = $state(true);
  let saving = $state(false);
  let switching = $state(false);

  const changed = $derived(view !== null && (keepGoing !== view.keep_going || snapshots !== view.snapshots));

  async function load() {
    try {
      view = await api.settings();
      keepGoing = view.keep_going;
      snapshots = view.snapshots;
      problem = null;
    } catch (error) {
      problem = String(error);
    }
  }

  /** A theme applies at once and is saved, without the Save button. */
  async function chooseTheme(theme: WindowTheme) {
    applyTheme(theme);
    try {
      const saved = await api.saveTheme(theme);
      if (view) view.theme = saved.theme;
    } catch (error) {
      toolbox.error = String(error);
    }
  }

  /** What the scheduled check tells of changes at once: it is a timer, not a setting to save. */
  async function switchWatch(change: { updates?: boolean; health?: boolean; drift?: string | null }) {
    if (!view) return;
    const updates = change.updates ?? view.notifications === true;
    const health = change.health ?? view.health === true;
    const drift = change.drift !== undefined ? change.drift : view.drift;
    switching = true;
    try {
      const told = await api.saveWatch(updates, health, drift);
      const turnedOn = change.updates === true || change.health === true || (change.drift ?? null) !== null;
      if (turnedOn) toolbox.notice = { title: 'Notifications', text: told };
    } catch (error) {
      toolbox.error = `Notifications could not be changed: ${error}`;
    } finally {
      switching = false;
      await load();
    }
  }

  /** The timed restic backup: off, every day or every week, at once. */
  async function switchBackup(every: string) {
    switching = true;
    try {
      toolbox.notice = { title: 'Timed backup', text: await api.setResticTimer(every !== '', every === 'week') };
    } catch (error) {
      toolbox.error = `The timed backup could not be changed: ${error}`;
    } finally {
      switching = false;
      await load();
    }
  }

  async function save() {
    saving = true;
    try {
      view = await api.saveSettings(keepGoing, snapshots);
      problem = null;
    } catch (error) {
      toolbox.error = String(error);
    } finally {
      saving = false;
    }
  }

  load();
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Settings</h1>
      <p class="screen-subtitle">How runs go, shared with the terminal interface, and how the window looks.</p>
    </div>
    <button class="button" onclick={() => (toolbox.wizard = true)}><Icon name="check" size={16} /> Set up this machine…</button>
    <button class="button primary" disabled={!changed || saving} onclick={save}><Icon name="save" size={16} /> Save</button>
  </div>

  <div class="pane card content">
    {#if problem}
      <p class="empty-note bad">{problem}</p>
    {:else if view === null}
      <p class="empty-note">Reading the settings…</p>
    {:else}
      <h2 class="section">Runs</h2>
      <label class="toggle">
        <input type="checkbox" bind:checked={keepGoing} />
        <span>
          <span class="title">After a failed step, continue with the steps that do not need it</span>
          <span class="detail">Off, the run stops at the first failure; the steps after it are listed as not run.</span>
        </span>
      </label>
      <label class="toggle" class:off={view.snapshot_tool === null}>
        <input type="checkbox" bind:checked={snapshots} disabled={view.snapshot_tool === null} />
        <span>
          <span class="title">Take a snapshot before system changes</span>
          <span class="detail">
            {#if view.snapshot_tool}
              With {view.snapshot_tool}, when a plan is risky or changes privileged files, the kernel or disks. Backups says how to restore one.
            {:else}
              No snapshot tool (Snapper or Timeshift) is set up on this machine.
            {/if}
          </span>
        </span>
      </label>

      <h2 class="section">Notifications</h2>
      <p class="muted">One check, every 6 hours and soon after you log in, tells you of what is new. It never updates, repairs or puts back anything itself; the terminal interface shares these choices.</p>
      <label class="toggle" class:off={view.notifications === null}>
        <input
          type="checkbox"
          checked={view.notifications === true}
          disabled={view.notifications === null || switching}
          onchange={(event) => switchWatch({ updates: event.currentTarget.checked })}
        />
        <span>
          <span class="title">Tell me when updates are waiting</span>
          <span class="detail">
            {#if view.notifications === null}
              There is no home folder to keep the check's timer in.
            {:else}
              A desktop notification, the way Updates checks online; a click on it opens Updates.
            {/if}
          </span>
        </span>
      </label>
      <label class="toggle" class:off={view.health === null}>
        <input
          type="checkbox"
          checked={view.health === true}
          disabled={view.health === null || switching}
          onchange={(event) => switchWatch({ health: event.currentTarget.checked })}
        />
        <span>
          <span class="title">Tell me when Helix Doctor finds a new problem</span>
          <span class="detail">
            The health checks, a problem told once: a failed service, a disk filling up, a check after a reboot that did not hold. The network and DNS
            checks, which come and go, are left out.
          </span>
        </span>
      </label>
      <div class="toggle">
        <span>
          <label class="title" for="watch-drift">Tell me when this machine drifts from a baseline</label>
          <span class="detail">
            {#if view.baselines.length === 0}
              Save this machine as a baseline in Drift first.
            {:else}
              Each new difference from the baseline, told once: an application gone, a configuration file changed, a setting different.
            {/if}
          </span>
          <select
            id="watch-drift"
            value={view.drift ?? ''}
            disabled={view.health === null || view.baselines.length === 0 || switching}
            onchange={(event) => switchWatch({ drift: event.currentTarget.value || null })}
          >
            <option value="">No</option>
            {#each view.baselines as [slug, name] (slug)}
              <option value={slug}>From {name}</option>
            {/each}
          </select>
        </span>
      </div>

      <div class="toggle">
        <span>
          <label class="title" for="restic-timer">Back up with restic on a timer</label>
          <span class="detail">
            {#if view.restic_ready}
              The folders Restic backs up, into its repository, while you are logged in; a run missed while the machine was off runs at the next login.
            {:else}
              Name a restic repository first; Restic says how.
            {/if}
          </span>
          <select
            id="restic-timer"
            value={view.restic_timer ?? ''}
            disabled={!view.restic_ready || view.notifications === null || switching}
            onchange={(event) => switchBackup(event.currentTarget.value)}
          >
            <option value="">No</option>
            <option value="day">Every day</option>
            <option value="week">Every week</option>
          </select>
        </span>
      </div>

      <h2 class="section">Appearance</h2>
      <div class="themes" role="radiogroup" aria-label="Theme">
        {#each THEMES as choice (choice.theme)}
          <label class="toggle">
            <input
              type="radio"
              name="theme"
              value={choice.theme}
              checked={view.theme === choice.theme}
              onchange={() => chooseTheme(choice.theme)}
            />
            <span>
              <span class="title">{choice.title}</span>
              <span class="detail">{choice.detail}</span>
            </span>
          </label>
        {/each}
      </div>
      <p class="faint">The run's terminal stays dark in either theme, as programs expect.</p>

      <h2 class="section">Reporting a problem</h2>
      <p class="muted">
        A bug report says which version this is and how it was installed, the system, your settings, the last few runs and the health
        checks, ready to paste into an issue on GitHub.
      </p>
      <button class="button" onclick={() => toolbox.makeReport()}><Icon name="info" size={15} /> Make a bug report…</button>

      <h2 class="section">Elsewhere</h2>
      <p class="muted">
        The terminal interface keeps its own theme and icons, under its Settings screen. Favorites and the history are shared with it.
      </p>
      {#if view.path}<p class="faint">These settings are kept in <span class="mono">{view.path}</span>.</p>{/if}
    {/if}
  </div>
</div>

<style>
  .content {
    padding: 4px 18px 18px;
  }

  select {
    justify-self: start;
    height: 32px;
    margin-top: 6px;
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    color: var(--text);
    font: inherit;
  }

  .toggle .title[for] {
    cursor: default;
  }

  .toggle + .toggle {
    margin-top: 8px;
  }

  .themes {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 8px;
  }

  .themes .toggle + .toggle {
    margin-top: 0;
  }

  .toggle.off {
    opacity: 0.6;
    cursor: default;
  }

  .toggle > span {
    display: grid;
    gap: 2px;
  }

  .bad {
    color: var(--red);
  }
</style>
