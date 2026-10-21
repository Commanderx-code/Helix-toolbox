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

  /** Notifications turn on or off at once: they are a timer, not a setting to save. */
  async function switchNotifications(on: boolean) {
    switching = true;
    try {
      const told = await api.saveNotifications(on);
      if (on) toolbox.notice = { title: 'Update notifications', text: told };
    } catch (error) {
      toolbox.error = `Update notifications could not be turned ${on ? 'on' : 'off'}: ${error}`;
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

      <h2 class="section">Updates</h2>
      <label class="toggle" class:off={view.notifications === null}>
        <input
          type="checkbox"
          checked={view.notifications === true}
          disabled={view.notifications === null || switching}
          onchange={(event) => switchNotifications(event.currentTarget.checked)}
        />
        <span>
          <span class="title">Tell me when updates are waiting</span>
          <span class="detail">
            {#if view.notifications === null}
              There is no home folder to keep the check's timer in.
            {:else}
              A desktop notification, checked every 6 hours and soon after you log in, the way Updates checks online. It never updates
              anything itself. This applies at once, and the terminal interface shares it.
            {/if}
          </span>
        </span>
      </label>

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
