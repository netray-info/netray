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
  ],
});
