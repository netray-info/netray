import { defineConfig } from 'vite';
import solidPlugin from 'vite-plugin-solid';
import { readFileSync } from 'node:fs';

function cargoVersion(): string {
  try {
    const cargo = readFileSync('../Cargo.toml', 'utf-8');
    const m = cargo.match(/^version\s*=\s*"([^"]+)"/m);
    return m ? m[1] : '0.0.0';
  } catch {
    return '0.0.0';
  }
}

export default defineConfig({
  plugins: [solidPlugin()],
  define: {
    __APP_VERSION__: JSON.stringify(cargoVersion()),
  },
  server: {
    port: 5176,
    proxy: {
      '/inspect': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/api': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/api-docs': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/docs': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/health': { target: 'http://127.0.0.1:3000', changeOrigin: true },
      '/ready': { target: 'http://127.0.0.1:3000', changeOrigin: true },
    },
  },
  build: {
    target: 'esnext',
    outDir: 'dist',
  },
});
