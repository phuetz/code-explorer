import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Prevent vite from obscuring Rust errors
  clearScreen: false,
  build: {
    // The lazy C++ Shiki grammar is large by design; other oversized chunks
    // should still surface above this threshold.
    chunkSizeWarningLimit: 700,
  },
  server: {
    port: 1420,
    strictPort: true,
  },
})
