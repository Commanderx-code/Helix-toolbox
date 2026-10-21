<script lang="ts">
  // The notice (something to read), the prompt (a name or a word to type)
  // and the bug report that every screen uses, from the shared state.
  import * as api from '../lib/api';
  import { modal } from '../lib/keys';
  import Icon from './Icon.svelte';
  import { toolbox } from '../lib/state.svelte';

  let answer = $state('');
  let input: HTMLInputElement | undefined = $state();

  $effect(() => {
    if (toolbox.prompt) {
      answer = '';
      setTimeout(() => input?.focus(), 0);
    }
  });

  const ready = $derived(
    toolbox.prompt !== null && answer.trim() !== '' && (!toolbox.prompt.expect || answer === toolbox.prompt.expect),
  );

  function submit() {
    if (toolbox.prompt && ready) toolbox.prompt.resolve(answer.trim());
  }

  let copied = $state(false);
  $effect(() => {
    if (toolbox.report) copied = false;
  });

  async function copyReport(text: string) {
    try {
      await api.copyText(text);
      copied = true;
    } catch (error) {
      toolbox.error = `The report could not be copied: ${error}`;
    }
  }
</script>

<svelte:window
  onkeydown={(event) => {
    if (event.key !== 'Escape') return;
    if (toolbox.prompt) toolbox.prompt.resolve(null);
    else if (toolbox.notice) toolbox.notice = null;
    else if (toolbox.report) toolbox.report = null;
  }}
/>

{#if toolbox.notice}
  <div class="backdrop" role="presentation">
    <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="notice-title" use:modal>
      <h2 id="notice-title">{toolbox.notice.title}</h2>
      <pre class:mono={toolbox.notice.mono}>{toolbox.notice.text}</pre>
      <div class="buttons">
        <button class="button primary" onclick={() => (toolbox.notice = null)}>OK</button>
      </div>
    </div>
  </div>
{/if}

{#if toolbox.report}
  {@const report = toolbox.report}
  <div class="backdrop" role="presentation">
    <div class="dialog card" role="dialog" aria-modal="true" aria-labelledby="report-title" use:modal>
      <h2 id="report-title">Bug report</h2>
      <p>
        Paste it into a new issue at <span class="mono">github.com/Commanderx-code/Helix-toolbox/issues</span>, with what happened and how to
        make it happen again. It shows your home folder as ~ and holds no host name or logs, but read it before posting.
      </p>
      {#if report.problem}
        <p class="problem" role="alert"><Icon name="warning" size={15} /> {report.problem}</p>
      {:else if report.text === null}
        <p class="faint" role="status">Gathering the report; the health checks take a few seconds…</p>
      {:else}
        <pre class="mono">{report.text}</pre>
      {/if}
      <div class="buttons">
        {#if copied}<span class="copied" role="status"><Icon name="check" size={14} /> Copied</span>{/if}
        <button class="button" data-autofocus onclick={() => (toolbox.report = null)}>Close</button>
        <button class="button primary" disabled={report.text === null} onclick={() => report.text && copyReport(report.text)}>
          <Icon name="save" size={14} /> Copy
        </button>
      </div>
    </div>
  </div>
{/if}

{#if toolbox.prompt}
  <div class="backdrop" role="presentation">
    <div class="dialog card narrow" role="dialog" aria-modal="true" aria-labelledby="prompt-title" use:modal>
      <h2 id="prompt-title">{toolbox.prompt.title}</h2>
      <p>{toolbox.prompt.text}</p>
      <input
        bind:this={input}
        bind:value={answer}
        placeholder={toolbox.prompt.placeholder ?? ''}
        onkeydown={(event) => {
          if (event.key === 'Enter') submit();
        }}
      />
      <div class="buttons">
        <button class="button" onclick={() => toolbox.prompt?.resolve(null)}>Cancel</button>
        <button class="button primary" disabled={!ready} onclick={submit}>
          <Icon name="check" size={14} /> OK
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--backdrop);
    z-index: 40;
  }

  .dialog {
    display: flex;
    flex-direction: column;
    width: min(760px, calc(100vw - 80px));
    max-height: calc(100vh - 80px);
    padding: 22px 24px;
    border-color: var(--selected-line);
    box-shadow: 0 24px 60px var(--shadow);
  }

  .dialog.narrow {
    width: min(520px, calc(100vw - 80px));
  }

  h2 {
    margin: 0 0 12px;
    font-size: 19px;
    color: var(--purple);
  }

  p {
    margin: 0 0 12px;
    color: var(--muted);
  }

  pre {
    flex: 1;
    min-height: 0;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: var(--bg);
    font-family: var(--sans);
    font-size: 14px;
    line-height: 1.5;
  }

  pre.mono {
    font-family: var(--mono);
    font-size: 12.5px;
  }

  input {
    height: 36px;
    padding: 0 12px;
    border: 1px solid var(--line-strong);
    border-radius: var(--radius-small);
    background: var(--bg);
    outline: 0;
  }

  input:focus {
    border-color: var(--cyan);
  }

  .buttons {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 16px;
  }

  .problem {
    display: flex;
    gap: 8px;
    color: var(--red);
  }

  .copied {
    display: inline-flex;
    gap: 6px;
    align-items: center;
    margin-right: auto;
    color: var(--green);
  }
</style>
