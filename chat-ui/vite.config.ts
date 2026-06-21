import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const target = env.VITE_MCP_URL || 'http://127.0.0.1:3010'

  return {
    plugins: [react(), tailwindcss()],
    server: {
      port: 5176,
      strictPort: true,
      proxy: {
        '/api': { target, changeOrigin: true },
        '/health': { target, changeOrigin: true },
        '/mcp': { target, changeOrigin: true },
      },
    },
  }
})
