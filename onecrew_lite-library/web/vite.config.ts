import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defaultClientConditions, defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';
import { readRuntime, verifyRuntime } from 'onecrew/runtime';

export default defineConfig(async ({ command }) => {
  let apiTarget: string | undefined;
  if (command === 'serve') {
    process.env.ONECREW_DATA_DIR ??= fileURLToPath(new URL('../.data', import.meta.url));
    const runtime = await readRuntime();
    if (!runtime) throw new Error('Start the OneCrew backend for this data profile before Vite.');
    await verifyRuntime(runtime);
    apiTarget = runtime.url;
  }
  return {
    base: process.env.VITE_ONECREW_UNIFIED === '1' ? '/library/' : '/',
    plugins: [react(), tailwindcss()],
    resolve: {
      alias: { '@': path.resolve(process.cwd(), 'src') },
      conditions:
        command === 'serve'
          ? ['onecrew:source', ...defaultClientConditions]
          : defaultClientConditions,
    },
    build: { outDir: 'dist' },
    server: {
      proxy: apiTarget
        ? Object.fromEntries(
            ['/api', '/creator-sync/pairing-callback', '/mcp', '/healthz'].map((route) => [
              route,
              // Keep the browser origin and Host aligned for the local origin checks.
              { target: apiTarget, changeOrigin: false },
            ]),
          )
        : undefined,
    },
  };
});
