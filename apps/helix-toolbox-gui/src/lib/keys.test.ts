// @vitest-environment jsdom
// The keyboard helpers on a document of their own: arrow keys through a
// list, a dialog that keeps the focus, and scrolling regions in the Tab
// order. jsdom lays nothing out: an element counts as shown unless it is
// inside a `hidden` one, and the sizes a region scrolls by are given by hand.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { arrows, keepScrollingReachable, modal } from './keys';

/** A key pressed on the focused element; whether something took it. */
function press(key: string, modifiers: KeyboardEventInit = {}): boolean {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...modifiers });
  (document.activeElement ?? document.body).dispatchEvent(event);
  return event.defaultPrevented;
}

/** Let MutationObserver callbacks and queued microtasks run. */
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

const focused = () => (document.activeElement as HTMLElement | null)?.dataset.name;

const offsetParent = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetParent')!;

beforeEach(() => {
  // jsdom has no scrolling, and gives every element a null offsetParent.
  Element.prototype.scrollIntoView = () => {};
  Object.defineProperty(HTMLElement.prototype, 'offsetParent', {
    configurable: true,
    get(this: HTMLElement) {
      return this.closest('[hidden]') ? null : document.body;
    },
  });
});

afterEach(() => {
  Object.defineProperty(HTMLElement.prototype, 'offsetParent', offsetParent);
  document.body.innerHTML = '';
  vi.useRealTimers();
});

describe('arrows through a list', () => {
  function list(html: string, options: { items: string; follow?: string }) {
    document.body.innerHTML = `<div id="list">${html}</div>`;
    const node = document.getElementById('list')!;
    const clicked: string[] = [];
    node.addEventListener('click', (event) => clicked.push((event.target as HTMLElement).dataset.name!));
    const action = arrows(node, options);
    const item = (name: string) => node.querySelector<HTMLElement>(`[data-name="${name}"]`)!;
    const stops = () => [...node.querySelectorAll<HTMLElement>(options.items)].map((item) => item.tabIndex);
    return { node, clicked, action, item, stops };
  }

  const ROWS =
    '<button class="row" data-name="a">A</button><button class="row selected" data-name="b">B</button><button class="row" data-name="c">C</button>';

  it('is one Tab stop: the selected item', () => {
    const { stops } = list(ROWS, { items: '.row' });
    expect(stops()).toEqual([-1, 0, -1]);
  });

  it('moves with Up, Down, Home and End, and stays at the ends', () => {
    const { item } = list(ROWS, { items: '.row' });
    item('b').focus();
    expect(press('ArrowDown')).toBe(true);
    expect(focused()).toBe('c');
    press('ArrowDown');
    expect(focused()).toBe('c');
    press('Home');
    expect(focused()).toBe('a');
    press('ArrowUp');
    expect(focused()).toBe('a');
    press('End');
    expect(focused()).toBe('c');
    press('ArrowUp');
    expect(focused()).toBe('b');
  });

  it('selects the item reached, as the terminal interface does', () => {
    const { item, clicked } = list(ROWS, { items: '.row', follow: '.row' });
    item('a').focus();
    press('ArrowDown');
    press('ArrowDown');
    expect(clicked).toEqual(['b', 'c']);
  });

  it('selects only the items that follow the focus', () => {
    const html =
      '<button class="row pick" data-name="a">A</button><button class="row" data-name="b">B</button><button class="row pick" data-name="c">C</button>';
    const { item, clicked } = list(html, { items: '.row', follow: '.pick' });
    item('a').focus();
    press('ArrowDown');
    press('ArrowDown');
    expect(clicked).toEqual(['c']);
  });

  it('makes the item last reached the Tab stop', () => {
    const { item, stops } = list(ROWS, { items: '.row' });
    item('b').focus();
    press('ArrowDown');
    expect(stops()).toEqual([-1, -1, 0]);
  });

  it('skips disabled and hidden items', () => {
    const html =
      '<button class="row" data-name="a">A</button><button class="row" data-name="b" disabled>B</button><div hidden><button class="row" data-name="h">H</button></div><button class="row" data-name="c">C</button>';
    const { item } = list(html, { items: '.row' });
    item('a').focus();
    press('ArrowDown');
    expect(focused()).toBe('c');
  });

  it('leaves Alt and Ctrl with the arrows, and other keys, alone', () => {
    const { item } = list(ROWS, { items: '.row' });
    item('a').focus();
    expect(press('ArrowDown', { altKey: true })).toBe(false);
    expect(press('ArrowDown', { ctrlKey: true })).toBe(false);
    expect(press('x')).toBe(false);
    expect(focused()).toBe('a');
  });

  it('keeps one Tab stop as rows come and go', async () => {
    const { node, stops } = list(ROWS, { items: '.row' });
    node.querySelector('.selected')!.remove();
    node.insertAdjacentHTML('beforeend', '<button class="row" data-name="d">D</button>');
    await flush();
    expect(stops()).toEqual([0, -1, -1]);
  });

  it('stops listening when destroyed', () => {
    const { item, action } = list(ROWS, { items: '.row' });
    action.destroy();
    item('a').focus();
    expect(press('ArrowDown')).toBe(false);
    expect(focused()).toBe('a');
  });
});

describe('a dialog', () => {
  function dialog(html: string) {
    document.body.innerHTML = `<button data-name="before">Open</button><div id="dialog">${html}</div>`;
    document.querySelector<HTMLElement>('[data-name="before"]')!.focus();
    const node = document.getElementById('dialog')!;
    return { node, action: modal(node) };
  }

  const CONTROLS =
    '<button data-name="first">Cancel</button><input data-name="middle" data-autofocus><button data-name="last">Run</button>';

  it('takes the focus to its autofocus control', async () => {
    dialog(CONTROLS);
    await flush();
    expect(focused()).toBe('middle');
  });

  it('else to its first control', async () => {
    dialog('<button data-name="first">Cancel</button><button data-name="last">Run</button>');
    await flush();
    expect(focused()).toBe('first');
  });

  it('keeps Tab and Shift-Tab inside it', async () => {
    dialog(CONTROLS);
    await flush();
    expect(press('Tab')).toBe(false);
    document.querySelector<HTMLElement>('[data-name="last"]')!.focus();
    expect(press('Tab')).toBe(true);
    expect(focused()).toBe('first');
    expect(press('Tab', { shiftKey: true })).toBe(true);
    expect(focused()).toBe('last');
  });

  it('holds the focus when it has no control', async () => {
    const { node } = dialog('<p>Reading…</p>');
    await flush();
    expect(document.activeElement).toBe(node);
    expect(press('Tab')).toBe(true);
    expect(document.activeElement).toBe(node);
  });

  it("leaves Tab to the run's terminal, and Ctrl-Tab leaves it", async () => {
    dialog(
      '<button data-name="stop">Stop</button><div class="xterm"><textarea data-name="terminal" data-autofocus></textarea></div><button data-name="close">Close</button>',
    );
    await flush();
    expect(focused()).toBe('terminal');
    expect(press('Tab')).toBe(false);
    expect(focused()).toBe('terminal');
    expect(press('Tab', { ctrlKey: true })).toBe(true);
    expect(focused()).toBe('stop');
    document.querySelector<HTMLElement>('[data-name="terminal"]')!.focus();
    press('Tab', { ctrlKey: true, shiftKey: true });
    expect(focused()).toBe('close');
  });

  it('gives the focus back when it closes', async () => {
    const { action } = dialog(CONTROLS);
    await flush();
    action.destroy();
    expect(focused()).toBe('before');
  });

  it('takes the focus back when its focused control goes away', async () => {
    const { node } = dialog(CONTROLS);
    await flush();
    node.querySelector('[data-name="middle"]')!.remove();
    (document.activeElement as HTMLElement | null)?.blur();
    node.insertAdjacentHTML('beforeend', '<button data-name="next">Next</button>');
    await flush();
    expect(node.contains(document.activeElement)).toBe(true);
  });
});

describe('a region that scrolls', () => {
  /** A region `overflow: auto` of 100 pixels holding `height` of content. */
  function region(html: string, height: number) {
    const node = document.createElement('div');
    node.style.overflowY = 'auto';
    node.innerHTML = html;
    Object.defineProperty(node, 'clientHeight', { configurable: true, value: 100 });
    Object.defineProperty(node, 'scrollHeight', { configurable: true, get: () => height });
    return {
      node,
      resize(next: number) {
        height = next;
      },
    };
  }

  it('joins the Tab order while it scrolls, and leaves when it fits', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout'] });
    const root = document.createElement('main');
    document.body.append(root);
    const text = region('<p>A long report</p>', 400);
    const buttons = region('<button>Restore…</button>', 400);
    const terminal = region('', 400);
    terminal.node.classList.add('xterm');
    root.append(text.node, buttons.node, terminal.node);
    keepScrollingReachable(root);
    await vi.advanceTimersByTimeAsync(300);
    expect(text.node.tabIndex).toBe(0);
    expect(text.node.hasAttribute('data-scroll-stop')).toBe(true);
    // A control inside already reaches it, and the terminal scrolls itself.
    expect(buttons.node.hasAttribute('tabindex')).toBe(false);
    expect(terminal.node.hasAttribute('tabindex')).toBe(false);

    text.resize(50);
    text.node.append(document.createElement('br'));
    await vi.advanceTimersByTimeAsync(300);
    expect(text.node.hasAttribute('tabindex')).toBe(false);
    expect(text.node.hasAttribute('data-scroll-stop')).toBe(false);
  });

  it('keeps a tabindex it did not give', async () => {
    vi.useFakeTimers({ toFake: ['setTimeout'] });
    const root = document.createElement('main');
    document.body.append(root);
    const text = region('<p>A long report</p>', 50);
    text.node.tabIndex = 0;
    root.append(text.node);
    keepScrollingReachable(root);
    await vi.advanceTimersByTimeAsync(300);
    expect(text.node.tabIndex).toBe(0);
  });
});
