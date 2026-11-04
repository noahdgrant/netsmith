import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

// The bundle is embedded in the netsmith binary and served from its root.
export default defineConfig({
  plugins: [preact()],
  build: { outDir: 'dist', emptyOutDir: true, sourcemap: false, target: 'es2022' },
  // The dev server asks an editor that is already running: NETSMITH_EDITOR names it.
  server: { proxy: { '/api': { target: process.env.NETSMITH_EDITOR ?? 'http://127.0.0.1:8787', ws: true } } },
  test: { include: ['src/**/*.test.ts'] },
});
