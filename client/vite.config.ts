import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    host: '127.0.0.1',
    allowedHosts: ['woghub.site'],
    proxy: {
      '^/api/': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
        // The support chat WebSocket lives under /api/v1/support/ws.
        ws: true,
      },
      '^/gateway/': {
        target: 'http://127.0.0.1:3000',
        changeOrigin: true,
      },
    },
  },
})

