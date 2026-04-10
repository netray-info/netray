import { defineConfig } from 'vite'
import solidPlugin from 'vite-plugin-solid'

export default defineConfig({
  plugins: [solidPlugin()],
  server: {
    port: 5178,
    proxy: {
      '/api': 'http://localhost:8085'
    }
  },
  build: {
    target: 'es2020',
    outDir: 'dist'
  }
})
