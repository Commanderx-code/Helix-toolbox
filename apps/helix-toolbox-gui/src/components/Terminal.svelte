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

  // The dark theme's colors in either theme: programs in a terminal expect
  // a dark background.
  const DARK = {
    background: '#0d111a',
    foreground: '#e7ecfa',
    cursor: '#68e8f6',
    cursorAccent: '#0d111a',
    selectionBackground: '#4e6e94',
    black: '#1a2232',
    red: '#ff788c',
    green: '#78e6a0',
    yellow: '#facc66',
    blue: '#6ea8ff',
    magenta: '#b18bff',
    cyan: '#68e8f6',
    white: '#e7ecfa',
    brightBlack: '#6e7c91',
    brightRed: '#ff9aaa',
    brightGreen: '#9ef0bd',
    brightYellow: '#ffdd8a',
    brightBlue: '#97c0ff',
    brightMagenta: '#c9adff',
    brightCyan: '#9cf2fb',
    brightWhite: '#ffffff',
  };

  onMount(() => {
    const mono = getComputedStyle(document.documentElement).getPropertyValue('--mono').trim();
    terminal = new Terminal({
      fontFamily: mono,
      fontSize: 13,
      lineHeight: 1.15,
      cursorBlink: true,
      scrollback: 10000,
      convertEol: false,
      theme: DARK,
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
    background: #0d111a;
  }

  .terminal :global(.xterm-viewport) {
    background: transparent !important;
  }
</style>
