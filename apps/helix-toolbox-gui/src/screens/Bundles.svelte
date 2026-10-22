<script lang="ts">
  // Offline bundles, as `helix-toolbox bundle` makes, checks and installs
  // them: the Install Plan's entries downloaded into a folder for a machine
  // without reliable internet, and a bundle installed from its files.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { BundleContents, BundleView } from '../lib/types';

  let makeFolder = $state('~/helix-toolbox-bundle');
  let complete = $state(false);
  let contents = $state<BundleContents | null>(null);
  let reading = $state(false);
  let makeProblem = $state<string | null>(null);

  let installFolder = $state('');
  let trust = $state(false);
  let bundle = $state<BundleView | null>(null);
  let checking = $state(false);
  let installProblem = $state<string | null>(null);

  const distribution = $derived(toolbox.about?.overview.rows.find(([name]) => name === 'Distribution')?.[1] ?? 'this distribution');
  const queued = $derived(toolbox.queue.map((id) => toolbox.entry(id)?.name ?? id));
  const offline = $derived(contents?.entries.filter((entry) => entry.offline).length ?? 0);

  // What was read no longer applies once the entries or the choice change.
  $effect(() => {
    void toolbox.queue;
    void complete;
    contents = null;
  });

  async function readContents() {
    reading = true;
    makeProblem = null;
    try {
      contents = await api.bundleContents(toolbox.queue, complete);
    } catch (error) {
      makeProblem = String(error);
    } finally {
      reading = false;
    }
  }

  async function check() {
    checking = true;
    installProblem = null;
    bundle = null;
    try {
      bundle = await api.bundleCheck(installFolder);
    } catch (error) {
      installProblem = String(error);
    } finally {
      checking = false;
    }
  }

  /** What gets downloaded, as the command line says it. */
  function downloads(found: BundleContents): string[] {
    const every = complete ? 'every dependency' : 'the dependencies this machine lacks';
    const lines = [];
    if (found.dnf.length > 0) lines.push(`dnf downloads ${found.dnf.join(', ')} with their dependencies.`);
    else if (found.zypper.length > 0) lines.push(`zypper downloads ${found.zypper.join(', ')} with ${every}, as you, without root.`);
    else if (found.files > 0 || found.flatpaks.length === 0)
      lines.push(`${found.files} package ${found.files === 1 ? 'file' : 'files'} to download, with ${every}.`);
    if (found.flatpaks.length > 0)
      lines.push(`flatpak create-usb copies ${found.flatpaks.join(', ')}, with the runtimes they need, from this machine's installation.`);
    return lines;
  }

  const megabytes = (bytes: number) => `${(bytes / 1e6).toFixed(1)} MB`;
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Offline bundles</h1>
      <p class="screen-subtitle">For a machine of the same distribution without reliable internet: packages downloaded here, installed there.</p>
    </div>
  </div>

  <div class="split">
    <section class="pane card part">
      <h2>Make a bundle</h2>
      <p class="muted">
        The Install Plan's entries and the packages they need for {distribution}, downloaded with Helix Toolbox into a folder. Nothing is installed
        here.
      </p>

      {#if queued.length === 0}
        <p class="empty">
          The Install Plan is empty. Queue the entries to bundle there first.
          <button class="button ghost" onclick={() => toolbox.open({ kind: 'plan' })}><Icon name="plan" size={14} /> Install Plan</button>
        </p>
      {:else}
        <h3>{queued.length} {queued.length === 1 ? 'entry' : 'entries'} from the Install Plan</h3>
        <p class="queued">{queued.join(', ')}</p>
      {/if}

      <label class="field">
        <span class="label">Folder, new or empty</span>
        <input class="mono" bind:value={makeFolder} spellcheck="false" />
      </label>
      <label class="toggle">
        <input type="checkbox" bind:checked={complete} />
        <span>
          <span class="title">Every dependency</span>
          <span class="detail">Larger, for a machine set up less than this one. Off, only what this machine lacks.</span>
        </span>
      </label>

      <div class="actions">
        <button class="button" disabled={queued.length === 0 || reading} onclick={readContents}>
          <Icon name="search" size={14} />
          {reading ? 'Reading…' : 'Check what it needs'}
        </button>
        <button
          class="button primary"
          disabled={queued.length === 0 || (contents !== null && offline === 0) || toolbox.preparing || toolbox.busy}
          onclick={() => toolbox.reviewWith(() => api.prepareBundleCreate(makeFolder, toolbox.queue, complete))}
        >
          <Icon name="package" size={14} /> Make the bundle…
        </button>
      </div>

      {#if makeProblem}
        <p class="said bad" role="alert"><Icon name="warning" size={15} /> {makeProblem}</p>
      {:else if contents}
        <ul class="entries">
          {#each contents.entries as entry, index (index)}
            <li>
              <span class="pill" class:good={entry.offline} class:notice={!entry.offline}>{entry.offline ? 'Offline' : 'Needs the network'}</span>
              <span>
                <span class="name">{entry.name}</span>
                {#if entry.flatpak}<span class="faint"> · the Flatpak {entry.flatpak}</span>{/if}
                {#if entry.network}<span class="sub">{entry.network}</span>{/if}
              </span>
            </li>
          {/each}
        </ul>
        {#if offline === 0}
          <p class="said bad" role="alert">None of these entries can be bundled; they all need the network.</p>
        {:else}
          {#each downloads(contents) as line, index (index)}<p class="muted">{line}</p>{/each}
        {/if}
      {/if}
    </section>

    <section class="pane card part">
      <h2>Install from a bundle</h2>
      <p class="muted">
        A bundle is checked first: made for this distribution and architecture, and every file matching its checksum. Then its packages install
        from its files, without downloading, all together.
      </p>

      <label class="field">
        <span class="label">The bundle's folder</span>
        <input class="mono" bind:value={installFolder} placeholder="/run/media/me/USB/helix-toolbox-bundle" spellcheck="false" />
      </label>
      <label class="toggle">
        <input type="checkbox" bind:checked={trust} />
        <span>
          <span class="title">Take the bundle's word for what it holds</span>
          <span class="detail">
            Install Debian packages this machine's repository index does not know, and add a Flatpak remote this machine lacks with the
            bundle's copy of its key. Only for a bundle you made yourself.
          </span>
        </span>
      </label>

      <div class="actions">
        <button class="button" disabled={installFolder.trim() === '' || checking} onclick={check}>
          <Icon name="check" size={14} />
          {checking ? 'Checking…' : 'Check it'}
        </button>
        <button
          class="button primary"
          disabled={installFolder.trim() === '' || toolbox.preparing || toolbox.busy}
          onclick={() => toolbox.reviewWith(() => api.prepareBundleInstall(installFolder, trust))}
        >
          <Icon name="play" size={14} /> Install from it…
        </button>
      </div>

      {#if installProblem}
        <p class="said bad" role="alert"><Icon name="warning" size={15} /> {installProblem}</p>
      {:else if bundle}
        <p class="said good" role="status"><Icon name="check" size={15} /> Every one of its {bundle.files} files matches its checksum.</p>
        <dl class="kv">
          <dt>Made for</dt>
          <dd>{bundle.distribution} {bundle.architecture}</dd>
          <dt>Made with</dt>
          <dd>Helix Toolbox {bundle.version}{bundle.release ? ' (a release build)' : ''}</dd>
          <dt>Made</dt>
          <dd>{bundle.created}{bundle.complete ? ', with every dependency' : ''}</dd>
          <dt>Size</dt>
          <dd>{megabytes(bundle.bytes)}</dd>
        </dl>
        <ul class="entries">
          {#each bundle.entries as entry, index (index)}
            <li>
              <span class="pill" class:good={entry.offline} class:notice={!entry.offline}>{entry.offline ? 'Offline' : 'Needs the network'}</span>
              <span>
                <span class="name">{entry.name}</span>
                {#if entry.network}<span class="sub">{entry.network}</span>{/if}
              </span>
            </li>
          {/each}
          {#each bundle.flatpaks as flatpak, index (index)}
            <li><span class="pill plain">Flatpak</span><span class="sub">{flatpak}</span></li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>
</div>

<style>
  .part {
    padding: 18px;
  }

  .part > p {
    margin: 6px 0 12px;
  }

  .empty {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    color: var(--muted);
  }

  .queued {
    color: var(--text);
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

  .toggle > span {
    display: grid;
    gap: 2px;
  }

  .entries {
    display: grid;
    gap: 8px;
    margin: 14px 0 8px;
    padding: 0;
    list-style: none;
  }

  .entries li {
    display: grid;
    grid-template-columns: 140px minmax(0, 1fr);
    align-items: start;
    gap: 10px;
  }

  .entries .pill {
    justify-self: start;
  }

  .entries .name {
    font-weight: 600;
  }

  .entries .sub {
    display: block;
    color: var(--muted);
    font-size: 13px;
    overflow-wrap: anywhere;
  }

  .said {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 14px 0 8px;
  }

  .said.bad {
    color: var(--red);
  }

  .said.good {
    color: var(--green);
  }

  .said :global(svg) {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .kv {
    margin-bottom: 8px;
  }
</style>
