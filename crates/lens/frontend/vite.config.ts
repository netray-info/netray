import { defineConfig } from 'vite'
import solidPlugin from 'vite-plugin-solid'

const DEV_SITE_TITLE = "netray.info — your domain's health grade, in under a second"
const DEV_SITE_DESCRIPTION =
  'Type a domain, get an A+ to F grade across DNS, TLS, HTTP, and email security. No account, no ads, open source.'
const DEV_SITE_OG_SITE_NAME = 'netray.info'

export default defineConfig({
  plugins: [
    solidPlugin(),
    {
      // Replace Axum server-side template vars during `vite dev`.
      // The build output retains the placeholders so Axum can substitute
      // them from the runtime [site] config.
      name: 'template-vars',
      apply: 'serve',
      transformIndexHtml(html: string): string {
        return html
          .replace(/.*\{\{site_og_image\}\}.*\n?/g, '')
          .replaceAll('{{site_title}}', DEV_SITE_TITLE)
          .replaceAll('{{site_description}}', DEV_SITE_DESCRIPTION)
          .replaceAll('{{site_og_site_name}}', DEV_SITE_OG_SITE_NAME)
      },
    },
  ],
  server: {
    port: 5178,
    proxy: {
      '/api': 'http://localhost:8085',
      '/badge': 'http://localhost:8085',
      '/og': 'http://localhost:8085',
      '/r': 'http://localhost:8085',
    },
  },
  build: {
    target: 'es2020',
    outDir: 'dist',
  },
})
