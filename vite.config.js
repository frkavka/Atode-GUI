import { defineConfig } from 'vite'

export default defineConfig({
  root: '.',
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        main: 'index.html'
      }
    }
  },
  server: {
    port: 1420,
    strictPort: true
  },
  // Deno import mapをViteでも使えるようにする
  resolve: {
    alias: {
      '@tauri-apps/api': 'https://esm.sh/@tauri-apps/api@1.5.0'
    }
  }
})