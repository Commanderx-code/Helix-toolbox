<script lang="ts">
  import { modal } from '../lib/keys';
  import Icon from './Icon.svelte';
  import Terminal from './Terminal.svelte';
  import * as api from '../lib/api';
  import { toolbox } from '../lib/state.svelte';
  import type { Finished, RunEvent, StepInfo } from '../lib/types';

  type Shown = StepInfo & { state: 'waiting' | 'running' | 'done' | 'failed' | 'skipped' | 'not_run' | 'interrupted'; exit?: number };

  let terminal: ReturnType<typeof Terminal> | undefined = $state();
  let steps = $state<Shown[]>([]);
  let ended = $state(0);
  let warnings = $state<string[]>([]);
  let finished = $state<Finished | null>(null);
  let problem = $state<string | null>(null);
  let stops = $state(0);
  /** Output bytes drawn so far, to skip any sent twice. */
  let drawn = 0;

  const total = $derived(steps.length);
  const title = $derived(
    finished
      ? finished.succeeded
        ? 'Finished'
        : finished.outcome.kind === 'interrupted'
          ? 'Stopped'
          : 'Failed'
      : problem
        ? 'Could not start'
        : 'Running',
  );

  function mark(step: StepInfo, state: Shown['state'], exit?: number) {
    steps = steps.map((shown) => (shown.index === step.index ? { ...shown, state, exit } : shown));
  }

  function received(event: RunEvent) {
    switch (event.event) {
      case 'run_started':
        steps = event.steps.map((step) => ({ ...step, state: 'waiting' }));
        break;
      case 'step_started':
        mark(event.step, 'running');
        break;
      case 'step_completed':
        mark(event.step, 'done');
        break;
      case 'step_failed':
        mark(event.step, event.interrupted ? 'interrupted' : 'failed', event.exit_code);
        break;
      case 'step_skipped':
        mark(event.step, 'skipped');
        break;
      case 'progress':
        ended = event.ended;
        break;
      case 'warning':
        warnings = [...warnings, event.message];
        break;
      case 'output': {
        const bytes = api.bytes(event.data);
        const skip = Math.max(0, drawn - event.offset);
        if (skip < bytes.length) {
          terminal?.write(bytes.subarray(skip));
          drawn = event.offset + bytes.length;
        }
        break;
      }
      case 'completed':
      case 'interrupted': {
        const { event: _kind, ...rest } = event;
        finished = rest;
        toolbox.busy = false;
        steps = steps.map((shown) => {
          const final = rest.steps.find((step) => step.index === shown.index);
          if (!final) return shown;
          return { ...shown, state: final.state, exit: 'exit_code' in final ? final.exit_code : undefined };
        });
        break;
      }
    }
  }

  async function start(rows: number, cols: number) {
    toolbox.busy = true;
    try {
      await api.run(rows, cols, received);
    } catch (error) {
      problem = String(error);
      toolbox.busy = false;
    }
  }

  function stop() {
    stops += 1;
    api.stop();
  }

  function close() {
    const done = (finished?.steps ?? []).filter((step) => step.state === 'done').map((step) => step.id);
    toolbox.closeRun(done);
  }

  const ICONS = {
    waiting: '○',
    running: '◐',
    done: '✓',
    failed: '✗',
    interrupted: '■',
    skipped: '–',
    not_run: '–',
  } as const;
</script>

<div class="backdrop">
  <div class="window card" role="dialog" aria-modal="true" aria-labelledby="run-title" use:modal>
    <div class="top">
      <h2 id="run-title" class={finished ? (finished.succeeded ? 'good' : 'bad') : problem ? 'bad' : ''}>
        <Icon name="terminal" />
        {title}
      </h2>
      <div class="progress" role="progressbar" aria-label="Steps ended" aria-valuemin={0} aria-valuemax={total} aria-valuenow={ended}>
        <div class="bar" style:width="{total ? (ended / total) * 100 : 0}%"></div>
      </div>
      <span class="faint count" aria-hidden="true">{ended} of {total} steps</span>
    </div>

    <div class="body">
      <ol class="steps">
        {#each steps as step (step.index)}
          <li class={step.state}>
            <span class="mark">{ICONS[step.state]}</span>
            <span class="name">{step.name}</span>
            {#if step.exit !== undefined}<span class="exit mono">exit {step.exit}</span>{/if}
          </li>
        {/each}
      </ol>
      <div class="terminal-frame">
        <Terminal
          bind:this={terminal}
          onready={start}
          ondata={(data) => api.write(data)}
          onresize={(rows, cols) => api.resize(rows, cols)}
        />
      </div>
    </div>

    {#if warnings.length > 0 || problem || finished}
      <div class="messages" aria-live="polite">
        {#if problem}<div class="bad"><Icon name="warning" size={15} /> {problem}</div>{/if}
        {#each warnings as warning, index (index)}
          <div class="warning"><Icon name="warning" size={15} /> {warning}</div>
        {/each}
        {#if finished}
          <div class={finished.succeeded ? 'good' : 'bad'}>
            <Icon name={finished.succeeded ? 'check' : 'x'} size={15} />
            Result: {finished.description}
          </div>
          {#each finished.checks as check (check.name)}
            <div class={check.passed ? 'faint' : 'bad'}>
              <Icon name={check.later ? 'history' : check.unsure ? 'info' : check.passed ? 'check' : 'x'} size={14} />
              {check.later ? 'After the reboot: ' : check.unsure ? 'Not checked: ' : ''}{check.name}: {check.detail}
            </div>
          {/each}
        {/if}
      </div>
    {/if}

    <div class="buttons">
      {#if toolbox.busy}
        <span class="faint hint">Type into the terminal when a step asks, such as for your password.</span>
        <button class="button danger" onclick={stop}>
          <Icon name="stop" size={14} />
          {stops === 0 ? 'Stop' : 'Force stop'}
        </button>
      {:else}
        <button class="button primary" onclick={close}>Close</button>
      {/if}
    </div>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: var(--backdrop);
    z-index: 20;
  }

  .window {
    display: flex;
    flex-direction: column;
    width: calc(100vw - 64px);
    height: calc(100vh - 64px);
    padding: 18px 20px;
    border-color: var(--selected-line);
    box-shadow: 0 24px 60px var(--shadow);
  }

  .top {
    display: flex;
    align-items: center;
    gap: 16px;
  }

  h2 {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0;
    font-size: 18px;
    color: var(--purple);
    min-width: 150px;
  }

  h2.good {
    color: var(--green);
  }

  h2.bad {
    color: var(--red);
  }

  .progress {
    flex: 1;
    height: 6px;
    border-radius: 6px;
    background: var(--panel-raised);
    overflow: hidden;
  }

  .bar {
    height: 100%;
    background: linear-gradient(90deg, var(--cyan), var(--purple));
    transition: width 0.25s;
  }

  .count {
    font-size: 13px;
  }

  .body {
    display: flex;
    gap: 14px;
    flex: 1;
    min-height: 0;
    margin-top: 16px;
  }

  .steps {
    width: 240px;
    flex-shrink: 0;
    margin: 0;
    padding: 6px;
    overflow-y: auto;
    list-style: none;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
  }

  .steps li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 8px;
    border-radius: var(--radius-small);
    color: var(--muted);
  }

  .mark {
    width: 16px;
    text-align: center;
    font-family: var(--mono);
  }

  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .exit {
    font-size: 11px;
  }

  .running {
    background: var(--selected);
    color: var(--text);
  }

  .running .mark {
    color: var(--cyan);
  }

  .done .mark {
    color: var(--green);
  }

  .failed,
  .interrupted {
    color: var(--red);
  }

  .terminal-frame {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: var(--radius-small);
    background: #0d111a;
  }

  .messages {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 120px;
    margin-top: 12px;
    overflow-y: auto;
    font-size: 13px;
  }

  .messages div {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .good {
    color: var(--green);
  }

  .bad {
    color: var(--red);
  }

  .warning {
    color: var(--yellow);
  }

  .buttons {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 12px;
    margin-top: 14px;
  }

  .hint {
    flex: 1;
    font-size: 13px;
  }
</style>
