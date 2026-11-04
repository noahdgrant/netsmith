import { defineConfig } from 'vite';
import preact from '@preact/preset-vite';

// The bundle is embedded in the netsmith binary and served from its root. Nothing is inlined into
// the page: the icons it names are files of their own (the binary serves each by its path).
export default defineConfig({
  plugins: [preact()],
  build: { outDir: 'dist', emptyOutDir: true, sourcemap: false, target: 'es2022', assetsInlineLimit: 0 },
  // The dev server asks an editor that is already running: NETSMITH_EDITOR names it. The editor
  // answers only to its own host and origin, so the proxy speaks as a page of that editor.
  server: {
    proxy: {
      '/api': {
        target: process.env.NETSMITH_EDITOR ?? 'http://127.0.0.1:8787',
        ws: true,
        changeOrigin: true,
        headers: { Origin: process.env.NETSMITH_EDITOR ?? 'http://127.0.0.1:8787' },
      },
    },
  },
  test: { include: ['src/**/*.test.ts'] },
});
