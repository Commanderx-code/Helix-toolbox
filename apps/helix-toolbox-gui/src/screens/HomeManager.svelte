<script lang="ts">
  // Home Manager, as `helix-toolbox home-manager`: the configuration, the
  // generations it built and which is active, its Git repository, and the
  // configuration files it links. Building, switching, going back to a
  // generation and pulling the repository are plans, reviewed first.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { HomeManagerView } from '../lib/types';

  let found = $state<HomeManagerView | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);

  async function load() {
    reading = true;
    try {
      found = await api.homeManager();
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

  const ready = $derived(found !== null && found.installed && found.config !== null);
  const busy = $derived(toolbox.preparing || toolbox.busy);

  function review(action: 'build' | 'switch' | 'activate' | 'pull', generation: number | null = null) {
    toolbox.reviewWith(() => api.prepareHomeManager(action, generation));
  }
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <h1 class="screen-title">Home Manager</h1>
      <p class="screen-subtitle">Its configuration and generations. Building, switching and going back are plans you review first.</p>
    </div>
    <div class="screen-buttons">
      {#if ready}
        <button class="button" disabled={busy} onclick={() => review('build')}><Icon name="package" size={16} /> Build…</button>
        <button class="button primary" disabled={busy} onclick={() => review('switch')}><Icon name="play" size={16} /> Switch…</button>
      {/if}
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  {#if problem}
    <p class="card empty-note bad">{problem}</p>
  {:else if found === null}
    <p class="card empty-note">Reading Home Manager…</p>
  {:else}
    <section class="card part" aria-label="The configuration">
      <h2>{found.version ? `Home Manager ${found.version}` : found.installed ? 'Home Manager' : 'Home Manager is not installed here'}</h2>
      {#if found.config}
        <dl>
          <dt>Configuration</dt>
          <dd><code>{found.reference ?? `${found.config.folder}/home.nix`}</code> ({found.config.flake ? 'a flake' : 'home.nix'})</dd>
          <dt>Named in</dt><dd>{found.origin}</dd>
        </dl>
      {:else if found.problem}
        <p class="bad">{found.problem}.</p>
      {:else}
        <p class="muted">No configuration was found. Name its folder as <code>flake</code> under <code>[home_manager]</code> in settings.toml.</p>
      {/if}
      {#if found.repository}
        {@const repository = found.repository}
        <h3>Its repository</h3>
        {#if repository.problem}
          <p class="bad">{repository.path}: {repository.problem}</p>
        {:else}
          <p>
            <code>{repository.path}</code>: {repository.branch ?? 'detached'}, {repository.changed} changed{#if repository.ahead !== null && repository.behind !== null}, {repository.ahead} ahead and {repository.behind} behind its upstream{/if}.
          </p>
          {#if ready && repository.upstream}
            <div class="actions">
              <button class="button" disabled={busy} onclick={() => review('pull')}><Icon name="down" size={14} /> Pull…</button>
            </div>
          {/if}
        {/if}
      {/if}
    </section>

    {#if found.generations.length > 0}
      <section class="card part" aria-label="Generations">
        <h2>Generations</h2>
        <ul class="list">
          {#each found.generations as generation (generation.id)}
            <li>
              <span class="id">{generation.id}</span>
              <span class="muted">{generation.built}</span>
              {#if generation.current}
                <span class="pill good">Active</span>
              {:else}
                <button class="button small" disabled={busy} onclick={() => review('activate', generation.id)}>
                  <Icon name="history" size={14} /> Go back to it…
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if found.links.length > 0}
      <section class="card part" aria-label="Configuration files">
        <h2>Configuration files</h2>
        <ul class="list">
          {#each found.links as link (link.path)}
            <li>
              <span class="id">{link.name}</span>
              <code class="muted">{link.path}</code>
              {#if link.managed}
                <span class="pill notice">Linked by Home Manager</span>
              {:else}
                <span class="pill">{link.target ? 'A link of its own' : 'A file of its own'}</span>
              {/if}
            </li>
          {/each}
        </ul>
        <p class="faint">A file Home Manager links is changed in its configuration, then a switch puts it in place.</p>
      </section>
    {/if}
  {/if}
</div>

<style>
  .screen {
    overflow-y: auto;
  }

  .part {
    padding: 14px 18px;
  }

  .part h2,
  .part h3 {
    margin: 0 0 8px;
  }

  .part h3 {
    margin-top: 12px;
    font-size: 1em;
  }

  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 4px 14px;
    margin: 0;
  }

  dt {
    color: var(--muted);
  }

  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    padding: 5px 0;
  }

  .id {
    min-width: 5em;
    font-weight: 600;
  }

  .actions {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }

  .bad {
    color: var(--red);
  }
</style>
