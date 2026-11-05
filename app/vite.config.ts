import react from '@vitejs/plugin-react';
import { loadEnv, type Plugin } from 'vite';
import { defineConfig } from 'vitest/config';

// The start chunk budget in kB before gzip. The build fails above it, well
// before Vite's 700 kB warning, so a form or dialog imported statically by
// mistake shows up at once.
const START_CHUNK_BUDGET_KB = 620;

function startChunkBudget(): Plugin {
  return {
    name: 'start-chunk-budget',
    apply: 'build',
    enforce: 'post',
    writeBundle(_options, bundle) {
      for (const chunk of Object.values(bundle)) {
        if (chunk.type !== 'chunk' || !chunk.isEntry) continue;
        const kb = Buffer.byteLength(chunk.code) / 1000;
        if (kb > START_CHUNK_BUDGET_KB) {
          this.error(
            `start chunk ${chunk.fileName} is ${kb.toFixed(1)} kB, over the ${START_CHUNK_BUDGET_KB} kB budget: ` +
              'something that should load lazily is imported statically (see FRONTEND.md "Start bundle")',
          );
        }
      }
    },
  };
}

// The Tauri dev server has to be reachable on a fixed port, because the shell
// points the web view at it while the app starts. VITE_PORT and
// CARVE_DEVHOST_PORT let a second browser session run beside the first.
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, '.', '');
  const port = Number(env.VITE_PORT ?? 1420);
  const devhostPort = Number(env.CARVE_DEVHOST_PORT ?? 1421);
  return {
    plugins: [react(), startChunkBudget()],
    clearScreen: false,
    server: {
      port,
      strictPort: true,
      watch: { ignored: ['**/src-tauri/**'] },
      // `pnpm dev:kernel` talks to the real kernel through carve-devhost.
      proxy: { '/kernel': `http://127.0.0.1:${devhostPort}` },
    },
    build: {
      target: 'safari16',
      outDir: 'dist',
      emptyOutDir: true,
      sourcemap: true,
      // The start chunk stays under this; three.js and the forms load later.
      chunkSizeWarningLimit: 700,
      rollupOptions: {
        output: {
          // three.js gets a chunk of its own, loaded with the 3D view.
          manualChunks: (id: string) => (id.includes('/node_modules/three/') ? 'three' : undefined),
        },
      },
    },
    test: {
      environment: 'node',
      include: ['src/**/*.test.ts'],
    },
  };
});
