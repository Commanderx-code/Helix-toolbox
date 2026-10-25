// @vitest-environment jsdom
// The window's state against the sample answers of mock.ts, which api.ts
// uses outside Tauri as `npm run dev` does. Each test has a fresh toolbox
// and a fresh mock; the mock's pretend delays pass on fake timers. In jsdom,
// because loading sets the theme on the document, and so that the runes
// are compiled as the window runs them.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { ClosedView } from './types';

type Toolbox = (typeof import('./state.svelte'))['toolbox'];
type Api = typeof import('./api');

const later = globalThis.setTimeout;

/** Let the mock's pretend delays pass until `work` is done. */
async function settle<T>(work: Promise<T>): Promise<T> {
  let done = false;
  const result = work.finally(() => {
    done = true;
  });
  while (!done) {
    await vi.advanceTimersByTimeAsync(50);
    // A real turn too, for what waits on more than timers, as an import.
    await new Promise((resolve) => later(resolve, 0));
  }
  return result;
}

/** A toolbox loaded from the mock, as the window starts. */
async function start(): Promise<{ toolbox: Toolbox; api: Api }> {
  const { toolbox } = await import('./state.svelte');
  const api = await import('./api');
  await settle(toolbox.load());
  return { toolbox, api };
}

/** Review and confirm `ids` as the Install Plan's Review and run does (or
 * an entry's Run now, when not `fromPlan`), and run them as the run panel
 * does; Stop is pressed while the step at `stopAt` runs, if given. */
async function run(toolbox: Toolbox, api: Api, ids: string[], fromPlan: boolean, stopAt?: number) {
  await settle(toolbox.review(ids, fromPlan));
  toolbox.confirm();
  const stops: Promise<void>[] = [];
  await settle(
    api.run(24, 80, (event) => {
      if (event.event === 'step_started' && event.step.index === stopAt) stops.push(api.stop());
    }),
  );
  await settle(Promise.all(stops));
}

beforeEach(() => {
  vi.resetModules();
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] });
});

afterEach(() => {
  vi.doUnmock('./api');
  vi.useRealTimers();
});

describe('starting', () => {
  it('reads the catalog, the pins and how entries stand', async () => {
    const { toolbox } = await start();
    expect(toolbox.areas.map((area) => area.name)).toEqual(['Applications', 'Configurations', 'Maintenance', 'Profiles']);
    expect(toolbox.favorites).toEqual(['neovim', 'ripgrep']);
    expect(toolbox.statuses.git.label).toBe('Installed');
    expect(toolbox.checking).toBe(false);
    expect(toolbox.error).toBeNull();
    expect(toolbox.entry('vscode')?.name).toBe('VS Code');
    expect(toolbox.entry('nothing-like-it')).toBeUndefined();
  });

  it('places each entry in its area and group', async () => {
    const { toolbox } = await start();
    expect(toolbox.byId.get('firefox')).toMatchObject({ area: 'Applications', group: 'Web Browsers' });
    expect(toolbox.byId.get('dotfiles')).toMatchObject({ area: 'Configurations', group: null });
  });
});

describe('the Install Plan', () => {
  it('queues an entry, and takes it out again', async () => {
    const { toolbox } = await start();
    toolbox.toggleQueued('git');
    toolbox.toggleQueued('vscode');
    expect(toolbox.queue).toEqual(['git', 'vscode']);
    expect(toolbox.isQueued('git')).toBe(true);
    toolbox.toggleQueued('git');
    expect(toolbox.queue).toEqual(['vscode']);
  });

  it('leaves out an entry that runs on its own, and one the catalog does not have', async () => {
    const { toolbox } = await start();
    toolbox.toggleQueued('jetbrains');
    toolbox.toggleQueued('nothing-like-it');
    expect(toolbox.queue).toEqual([]);
  });

  it('adds those not queued yet, in order, and says how many', async () => {
    const { toolbox } = await start();
    toolbox.toggleQueued('neovim');
    expect(toolbox.enqueue(['git', 'neovim', 'kernel-params', 'ripgrep', 'git'])).toBe(2);
    expect(toolbox.queue).toEqual(['neovim', 'git', 'ripgrep']);
  });

  it('loses a deleted profile, in the plan and in the details', async () => {
    const { toolbox } = await start();
    toolbox.enqueue(['git', 'profile-my-workstation']);
    toolbox.selected = 'profile-my-workstation';
    await settle(toolbox.profilesChanged('profile-my-workstation'));
    expect(toolbox.queue).toEqual(['git']);
    expect(toolbox.selected).toBeNull();
  });
});

describe('reviewing a plan', () => {
  it('asks the engine for a plan, and the yes opens the run', async () => {
    const { toolbox } = await start();
    await settle(toolbox.review(['git', 'vscode'], true));
    const prepared = toolbox.confirming?.prepared;
    expect(prepared?.kind).toBe('ready');
    expect(prepared?.kind === 'ready' && prepared.lines.map((line) => line.name)).toEqual(['Git', 'VS Code']);
    expect(toolbox.preparing).toBe(false);
    toolbox.confirm();
    expect(toolbox.confirming).toBeNull();
    expect(toolbox.running).toEqual({ ids: ['git', 'vscode'], fromPlan: true });
  });

  it('asks nothing for an empty plan', async () => {
    const { toolbox } = await start();
    await settle(toolbox.review([], true));
    expect(toolbox.confirming).toBeNull();
  });

  it('forgets a plan the user declines', async () => {
    const { toolbox } = await start();
    await settle(toolbox.review(['git'], true));
    await settle(toolbox.decline());
    expect(toolbox.confirming).toBeNull();
    expect(toolbox.running).toBeNull();
  });

  it('opens no run for a plan that cannot run', async () => {
    const { toolbox } = await start();
    await settle(toolbox.reviewWith(async () => ({ kind: 'blocked', lines: ['Blocked: no network'] })));
    toolbox.confirm();
    expect(toolbox.running).toBeNull();
    expect(toolbox.confirming?.prepared.kind).toBe('blocked');
  });

  it('says why a plan could not be made', async () => {
    const { toolbox } = await start();
    await settle(toolbox.reviewWith(() => Promise.reject(new Error('the catalog changed'))));
    expect(toolbox.error).toBe('Error: the catalog changed');
    expect(toolbox.confirming).toBeNull();
    expect(toolbox.preparing).toBe(false);
  });

  it('ignores a second review while the first is being prepared', async () => {
    const { toolbox } = await start();
    const first = toolbox.review(['git'], true);
    const second = toolbox.review(['vscode'], true);
    await settle(Promise.all([first, second]));
    expect(toolbox.confirming?.ids).toEqual(['git']);
  });
});

describe('the Install Plan after a run', () => {
  it('is empty when everything finished', async () => {
    const { toolbox, api } = await start();
    toolbox.enqueue(['git', 'vscode']);
    await run(toolbox, api, toolbox.queue, true);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual([]);
    expect(toolbox.running).toBeNull();
    expect(toolbox.busy).toBe(false);
  });

  it('keeps what did not finish, in order', async () => {
    const { toolbox, api } = await start();
    toolbox.enqueue(['git', 'vscode', 'zed']);
    await run(toolbox, api, toolbox.queue, true, 1);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['vscode', 'zed']);
  });

  it('stays as it was when the run never started', async () => {
    const { toolbox } = await start();
    toolbox.enqueue(['git', 'vscode']);
    await settle(toolbox.review(toolbox.queue, true));
    toolbox.confirm();
    // The run panel's api.run failed, so nothing ran.
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['git', 'vscode']);
  });

  it("has a profile's unfinished entries in its place", async () => {
    const { toolbox, api } = await start();
    // Developer basics runs Git, Neovim, Ripgrep and fzf, after VS Code.
    toolbox.enqueue(['vscode', 'developer-basics']);
    await run(toolbox, api, toolbox.queue, true, 2);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['neovim', 'ripgrep', 'fzf']);
  });

  it('is left alone by a run that did not come from it', async () => {
    const { toolbox, api } = await start();
    toolbox.enqueue(['vscode', 'zed']);
    await run(toolbox, api, ['zed'], false);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['vscode', 'zed']);
  });

  it('loses entries the catalog no longer has, and says why the run was not recorded', async () => {
    vi.doMock('./api', async (original) => ({
      ...(await original<Api>()),
      closeRun: async (): Promise<ClosedView> => ({
        remaining: ['gone', 'zed'],
        read_again: ['history'],
        not_recorded: 'The run was not recorded in the history: the disk is full',
      }),
    }));
    const { toolbox, api } = await start();
    toolbox.enqueue(['vscode', 'zed']);
    await run(toolbox, api, toolbox.queue, true, 0);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['zed']);
    expect(toolbox.error).toBe('The run was not recorded in the history: the disk is full');
  });

  it('is kept, and everything read again, when the run could not be closed', async () => {
    vi.doMock('./api', async (original) => ({
      ...(await original<Api>()),
      closeRun: () => Promise.reject('the run session is gone'),
    }));
    const { toolbox, api } = await start();
    toolbox.enqueue(['git', 'vscode']);
    await run(toolbox, api, toolbox.queue, true);
    await settle(toolbox.closeRun());
    expect(toolbox.queue).toEqual(['git', 'vscode']);
    expect(toolbox.error).toBe('the run session is gone');
    expect(toolbox.readAgain).toEqual({ statuses: 1, updates: 1, tools: 1, history: 1 });
  });
});

describe('reading again after a run', () => {
  it('has Updates, History and the tools read again after each run', async () => {
    const { toolbox, api } = await start();
    expect(toolbox.readAgain).toEqual({ statuses: 0, updates: 0, tools: 0, history: 0 });
    await run(toolbox, api, ['git'], false);
    await settle(toolbox.closeRun());
    expect(toolbox.readAgain).toEqual({ statuses: 1, updates: 1, tools: 1, history: 1 });
    await run(toolbox, api, ['zed'], false, 0);
    await settle(toolbox.closeRun());
    expect(toolbox.readAgain).toEqual({ statuses: 2, updates: 2, tools: 2, history: 2 });
  });

  it('reads how entries stand again', async () => {
    const { toolbox, api } = await start();
    await run(toolbox, api, ['git'], false);
    toolbox.statuses = {};
    await settle(toolbox.closeRun());
    expect(toolbox.statuses.git.label).toBe('Installed');
    expect(toolbox.checking).toBe(false);
  });

  it('reads nothing again when the run never started', async () => {
    const { toolbox } = await start();
    await settle(toolbox.review(['git'], false));
    toolbox.confirm();
    toolbox.statuses = {};
    await settle(toolbox.closeRun());
    expect(toolbox.readAgain).toEqual({ statuses: 0, updates: 0, tools: 0, history: 0 });
    expect(toolbox.statuses).toEqual({});
  });

  it('reads only what the engine names', async () => {
    vi.doMock('./api', async (original) => ({
      ...(await original<Api>()),
      closeRun: async (): Promise<ClosedView> => ({ remaining: [], read_again: ['history'], not_recorded: null }),
    }));
    const { toolbox } = await start();
    toolbox.statuses = {};
    await settle(toolbox.closeRun());
    expect(toolbox.readAgain).toEqual({ statuses: 0, updates: 0, tools: 0, history: 1 });
    expect(toolbox.statuses).toEqual({});
  });
});

describe('screens and entries', () => {
  it('shows an entry in its area, selected', async () => {
    const { toolbox } = await start();
    toolbox.search = 'fish';
    toolbox.palette = true;
    toolbox.show('fish');
    expect(toolbox.screen).toEqual({ kind: 'area', index: 1 });
    expect(toolbox.selected).toBe('fish');
    expect(toolbox.search).toBe('');
    expect(toolbox.palette).toBe(false);
    expect(toolbox.section).toBe('Catalog');
    toolbox.show('nothing-like-it');
    expect(toolbox.selected).toBe('fish');
  });

  it('names where a screen sits in the sidebar', async () => {
    const { toolbox } = await start();
    toolbox.open({ kind: 'restic' });
    expect(toolbox.section).toBe('Safekeeping');
    toolbox.open({ kind: 'plan' });
    expect(toolbox.section).toBe('Main menu');
  });

  it('finds entries in every area by all the words searched', async () => {
    const { toolbox } = await start();
    const ids = (text: string) => toolbox.matches(text).map((place) => place.entry.id);
    expect(ids('editor')).toEqual(['neovim', 'zed']);
    expect(ids('code EDITING')).toEqual(['vscode']);
    // A group's name counts.
    expect(ids('browsers')).toEqual(['firefox', 'brave', 'chromium']);
    expect(ids('  ')).toEqual([]);
    expect(ids('editor nothing-like-it')).toEqual([]);
  });

  it('pins and unpins an entry', async () => {
    const { toolbox } = await start();
    await settle(toolbox.togglePin('git'));
    expect(toolbox.favorites).toEqual(['neovim', 'ripgrep', 'git']);
    await settle(toolbox.togglePin('neovim'));
    expect(toolbox.favorites).toEqual(['ripgrep', 'git']);
  });

  it('asks a question and gives back the answer', async () => {
    const { toolbox } = await start();
    const answer = toolbox.ask('Save as profile', 'Name the profile.', 'My workstation');
    expect(toolbox.prompt?.title).toBe('Save as profile');
    toolbox.prompt?.resolve('Laptop');
    expect(await answer).toBe('Laptop');
    expect(toolbox.prompt).toBeNull();
  });

  it('tells what a command said, or its problem', async () => {
    const { toolbox } = await start();
    expect(await toolbox.tell('Trust', async () => 'Signed by the catalog.')).toBe(true);
    expect(toolbox.notice).toEqual({ title: 'Trust', text: 'Signed by the catalog.', mono: false });
    expect(await toolbox.tell('Trust', () => Promise.reject('no such entry'))).toBe(false);
    expect(toolbox.notice).toEqual({ title: 'Trust', text: 'no such entry' });
  });
});
