import { defineConfig } from 'vite'
import solidPlugin from 'vite-plugin-solid'

export default defineConfig({
  plugins: [solidPlugin()],
  server: {
    port: 5175,
    proxy: {
      '/api': 'http://localhost:8082'
    }
  },
  build: {
    target: 'es2020',
    outDir: 'dist'
  }
})
