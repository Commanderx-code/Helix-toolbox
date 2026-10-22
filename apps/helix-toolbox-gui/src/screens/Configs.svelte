<script lang="ts">
  // Ghostty's and Fastfetch's configuration, as `helix-toolbox config`:
  // which file is edited, its text, a few common fields and the backups kept
  // beside it. Changes stay a draft here; Save… reviews a plan that checks
  // the file is as it was read, keeps it as a backup and writes the draft.
  import Icon from '../components/Icon.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { ConfigChecked, ConfigField, ConfigKind, ConfigView } from '../lib/types';

  let found = $state<ConfigView[] | null>(null);
  let problem = $state<string | null>(null);
  let reading = $state(false);
  // The draft of each configuration, its fields' values and what its checks said.
  let drafts = $state<Record<string, string>>({});
  let fields = $state<Record<string, ConfigField[]>>({});
  let typed = $state<Record<string, string>>({});
  let checks = $state<Record<string, ConfigChecked | null>>({});
  let said = $state<Record<string, string | null>>({});

  async function load() {
    reading = true;
    try {
      const read = await api.configs();
      found = read;
      for (const config of read) {
        drafts[config.kind] = config.content;
        fields[config.kind] = config.fields;
        checks[config.kind] = config.checked;
        said[config.kind] = null;
        for (const field of config.fields) typed[`${config.kind}:${field.key}`] = field.value ?? '';
      }
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

  async function apply(kind: ConfigKind, field: ConfigField) {
    try {
      const draft = await api.setConfigField(kind, drafts[kind] ?? '', field.key, typed[`${kind}:${field.key}`] ?? '');
      drafts[kind] = draft.content;
      if (draft.fields.length > 0) fields[kind] = draft.fields;
      checks[kind] = null;
      said[kind] = `${field.label} is changed in the draft; Save… writes it.`;
    } catch (error) {
      said[kind] = String(error);
    }
  }

  async function check(kind: ConfigKind) {
    try {
      checks[kind] = await api.validateConfig(kind, drafts[kind] ?? '');
      said[kind] = null;
    } catch (error) {
      said[kind] = String(error);
    }
  }

  function discard(config: ConfigView) {
    drafts[config.kind] = config.content;
    fields[config.kind] = config.fields;
    for (const field of config.fields) typed[`${config.kind}:${field.key}`] = field.value ?? '';
    checks[config.kind] = config.checked;
    said[config.kind] = null;
  }

  const label = (checked: ConfigChecked | null) =>
    checked === null ? 'Not checked yet' : checked.state === 'valid' ? 'Valid' : checked.state === 'invalid' ? 'Invalid' : 'Not checked';
  const tone = (checked: ConfigChecked | null) =>
    checked?.state === 'valid' ? 'good' : checked?.state === 'invalid' ? 'bad' : 'plain';
  const busy = $derived(toolbox.preparing || toolbox.busy);
</script>

<div class="screen">
  <div class="screen-top">
    <div>
      <p class="screen-eyebrow">{toolbox.section}</p>
      <h1 class="screen-title">Configs</h1>
      <p class="screen-subtitle">Ghostty's and Fastfetch's configuration. Each save is checked and shown first, and the file there now is kept as a backup.</p>
    </div>
    <div class="screen-buttons">
      <button class="button" disabled={reading} onclick={load}><Icon name="refresh" size={16} /> {reading ? 'Reading…' : 'Read again'}</button>
    </div>
  </div>

  {#if problem}
    <p class="card part said bad" role="alert">{problem}</p>
  {:else if found === null}
    <p class="card part muted">Reading the configurations…</p>
  {:else}
    {#each found as config (config.kind)}
      <section class="card part" aria-label={`${config.label}'s configuration`}>
        <div class="head">
          <h2>{config.label}</h2>
          {#if !config.problem}<span class="pill {tone(checks[config.kind] ?? null)}">{label(checks[config.kind] ?? null)}</span>{/if}
        </div>
        {#if config.problem}
          <p class="said bad" role="alert"><Icon name="warning" size={15} /> {config.problem}</p>
        {:else}
          <dl class="facts">
            <dt>File</dt><dd class="mono">{config.path}{config.exists ? '' : ' (not there yet)'}</dd>
            <dt>Named by</dt><dd>{config.origin}</dd>
            {#if config.live !== config.path}<dt>{config.label} reads</dt><dd class="mono">{config.live}{config.managed ? ', a Home Manager link' : ''}</dd>{/if}
          </dl>
          {#if config.flake}
            <p class="muted">It is part of the Home Manager flake in <span class="mono">{config.flake}</span>: run Home Manager after a save to make it live.</p>
          {/if}
          {@const checked = checks[config.kind]}
          {#if checked && checked.state !== 'valid'}
            <p class="said {checked.state === 'invalid' ? 'bad' : 'muted'}" role="status">{checked.message}</p>
          {/if}

          <h3>Fields</h3>
          <div class="fields">
            {#each fields[config.kind] ?? [] as field (field.key)}
              <div class="field">
                <label class="label" for={`${config.kind}-${field.key}`}>{field.label}</label>
                <div class="inline">
                  <input id={`${config.kind}-${field.key}`} class="mono" bind:value={typed[`${config.kind}:${field.key}`]} placeholder={field.hint} spellcheck="false" />
                  <button class="button" aria-label={`Apply ${field.label}`} onclick={() => apply(config.kind, field)}>Apply</button>
                </div>
              </div>
            {/each}
          </div>

          <label class="label" for={`${config.kind}-text`}>The file</label>
          <textarea id={`${config.kind}-text`} class="mono" rows="14" spellcheck="false" bind:value={drafts[config.kind]}></textarea>
          {#if said[config.kind]}<p class="muted" role="status">{said[config.kind]}</p>{/if}
          <div class="actions">
            <button class="button" onclick={() => check(config.kind)}><Icon name="check" size={14} /> Check</button>
            <button
              class="button primary"
              disabled={busy || drafts[config.kind] === config.content || checks[config.kind]?.state === 'invalid'}
              onclick={() => toolbox.reviewWith(() => api.prepareConfigSave(config.kind, config.path ?? '', config.revision ?? '', drafts[config.kind] ?? ''))}
            >
              <Icon name="save" size={14} /> Save…
            </button>
            <button class="button" disabled={drafts[config.kind] === config.content} onclick={() => discard(config)}>Discard changes</button>
          </div>

          {#if config.backups.length > 0}
            <h3>Backups kept ({config.backups.length})</h3>
            <ul class="items" aria-label={`Backups of ${config.label}'s configuration`}>
              {#each config.backups as kept (kept.backup)}
                <li>
                  <span>
                    <span class="name">{kept.taken ?? 'A backup'}</span>
                    <span class="sub mono">{kept.backup}</span>
                  </span>
                  <span class="row-buttons">
                    <button class="button" disabled={busy} onclick={() => toolbox.reviewWith(() => api.prepareConfigRestore(config.kind, kept.backup))}>Put back…</button>
                  </span>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>
    {/each}
  {/if}
</div>

<style>
  .screen {
    overflow-y: auto;
  }

  .part {
    padding: 16px 18px;
  }

  .part p {
    margin: 6px 0;
  }

  .head {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 8px;
  }

  .head h2 {
    margin: 0;
  }

  h3 {
    margin: 16px 0 6px;
    font-size: 13px;
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
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: 10px;
  }

  .field {
    display: grid;
    gap: 6px;
  }

  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }

  .inline {
    display: flex;
    gap: 6px;
  }

  .inline input {
    flex: 1;
    min-width: 0;
  }

  input,
  textarea {
    padding: 0 10px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    font-size: 13px;
  }

  input {
    height: 34px;
  }

  textarea {
    display: block;
    width: 100%;
    margin: 6px 0;
    padding: 10px;
    resize: vertical;
    line-height: 1.45;
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
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }

  .items li > span:first-child {
    display: grid;
    gap: 2px;
    min-width: 0;
  }

  .row-buttons {
    display: flex;
    gap: 6px;
  }

  .name {
    font-weight: 600;
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
