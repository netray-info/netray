import { defineConfig } from '@playwright/test';

const INTER_TEST_DELAY_MS = 500;

export { INTER_TEST_DELAY_MS };

export default defineConfig({
  workers: 1,
  fullyParallel: false,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  timeout: 60_000,
  use: {
    ignoreHTTPSErrors: true,
    extraHTTPHeaders: {
      'User-Agent': 'netray-acceptance-tests/1.0',
    },
  },
  projects: [
    { name: 'smoke', testDir: './smoke' },
    { name: 'static-site', testDir: './static-site', dependencies: ['smoke'] },
    { name: 'api-contract', testDir: './api-contract', dependencies: ['smoke'] },
    { name: 'services', testDir: './services', dependencies: ['smoke'] },
    { name: 'integration', testDir: './integration', dependencies: ['services'] },
    // Production gets only this subset: the full suite probes unknown paths and crawls the
    // sitemap, which the production host's fail2ban botsearch jail bans (2026-10-08).
    {
      name: 'prod',
      testDir: '.',
      testMatch: [
        'smoke/health.spec.ts',
        'smoke/ready.spec.ts',
        'smoke/security-headers.spec.ts',
        'smoke/tls-certs.spec.ts',
        'static-site/assets.spec.ts',
        'api-contract/meta-shape.spec.ts',
        'api-contract/openapi.spec.ts',
      ],
    },
  ],
});
