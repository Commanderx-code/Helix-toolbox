/// <reference types="vitest/config" />
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
    // The header's emblem is the menu icon, in the repository's assets.
    fs: { allow: ['.', '../../assets'] },
  },
  build: {
    target: 'safari16',
    outDir: 'dist',
    emptyOutDir: true,
  },
  // `npm test`: the tests beside the modules in src/lib, in Node. A test
  // file that needs a document says `@vitest-environment jsdom` on its
  // first line; there the Svelte plugin also compiles runes as the window
  // runs them, not as a server renders them.
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
