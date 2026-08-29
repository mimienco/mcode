import { resolve } from 'node:path'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

export default defineConfig({
  root: resolve('src/renderer'),
  base: './',
  resolve: {
    alias: {
      '@renderer': resolve('src/renderer/src'),
      '@': resolve('src/renderer/src')
    }
  },
  plugins: [react(), tailwindcss()],
  define: {
    MCODE_BUILD_IDENTITY: 'null',
    MCODE_POSTHOG_WRITE_KEY: 'null',
    MCODE_DIAGNOSTICS_TOKEN_URL: 'null'
  },
  worker: {
    format: 'es'
  },
  build: {
    outDir: resolve('out/tauri-renderer'),
    emptyOutDir: true,
    manifest: true,
    modulePreload: { polyfill: true },
    target: 'es2020',
    rollupOptions: {
      preserveEntrySignatures: 'strict',
      input: {
        index: resolve('src/renderer/index.html'),
        lifecycleSmoke: resolve('src/renderer/lifecycle-smoke.html'),
        popout: resolve('src/renderer/popout.html'),
        web: resolve('src/renderer/web-index.html')
      }
    }
  }
})
