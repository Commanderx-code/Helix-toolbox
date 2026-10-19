import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri serves the built files itself, and in development loads the dev
// server on a fixed port.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // The emblem comes from the terminal interface's copy.
    fs: { allow: ['.', '../helix-toolbox/src/tui'] },
  },
  build: {
    target: 'safari16',
    outDir: 'dist',
    emptyOutDir: true,
  },
});
