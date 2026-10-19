<script lang="ts">
  import { onMount } from 'svelte';
  import { Terminal } from '@xterm/xterm';
  import { FitAddon } from '@xterm/addon-fit';
  import '@xterm/xterm/css/xterm.css';

  let {
    onready,
    ondata,
    onresize,
  }: {
    /** The terminal is open, at this size. */
    onready: (rows: number, cols: number) => void;
    /** What the user typed. */
    ondata: (data: string) => void;
    onresize: (rows: number, cols: number) => void;
  } = $props();

  let element: HTMLDivElement;
  let terminal: Terminal | undefined;

  /** Draw output from the run. */
  export function write(bytes: Uint8Array) {
    terminal?.write(bytes);
  }

  export function focus() {
    terminal?.focus();
  }

  onMount(() => {
    const css = getComputedStyle(document.documentElement);
    const token = (name: string) => css.getPropertyValue(name).trim();
    terminal = new Terminal({
      fontFamily: token('--mono'),
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 10000,
      convertEol: false,
      theme: {
        background: token('--bg'),
        foreground: token('--text'),
        cursor: token('--cyan'),
        cursorAccent: token('--bg'),
        selectionBackground: token('--selected-line'),
        black: '#1a2232',
        red: token('--red'),
        green: token('--green'),
        yellow: token('--yellow'),
        blue: '#6ea8ff',
        magenta: token('--purple'),
        cyan: token('--cyan'),
        white: token('--text'),
        brightBlack: token('--faint'),
        brightRed: '#ff9aaa',
        brightGreen: '#9ef0bd',
        brightYellow: '#ffdd8a',
        brightBlue: '#97c0ff',
        brightMagenta: '#c9adff',
        brightCyan: '#9cf2fb',
        brightWhite: '#ffffff',
      },
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(element);
    fit.fit();
    terminal.onData(ondata);
    terminal.onResize(({ rows, cols }) => onresize(rows, cols));
    const observer = new ResizeObserver(() => fit.fit());
    observer.observe(element);
    terminal.focus();
    onready(terminal.rows, terminal.cols);
    return () => {
      observer.disconnect();
      terminal?.dispose();
    };
  });
</script>

<div class="terminal" bind:this={element}></div>

<style>
  .terminal {
    width: 100%;
    height: 100%;
    padding: 8px 0 0 10px;
    background: var(--bg);
  }

  .terminal :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
