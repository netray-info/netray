import { test, expect, chromium } from '@playwright/test';
import { toolOrigins } from '../fixtures/env.js';

for (const { name, url } of toolOrigins()) {
  test(`${name}: GET / returns 200 text/html (browser context)`, async () => {
    test.fixme(name === 'email', 'beacon not yet deployed behind Traefik');
    const browser = await chromium.launch();
    const context = await browser.newContext({
      ignoreHTTPSErrors: true,
      userAgent: 'netray-acceptance-tests/1.0',
    });

    try {
      const page = await context.newPage();
      const response = await page.goto(`${url}/`, { waitUntil: 'commit' });
      expect(response).not.toBeNull();
      expect(response!.status()).toBe(200);
      expect(response!.headers()['content-type']).toContain('text/html');
      const body = await page.content();
      expect(body.length).toBeGreaterThan(0);
    } finally {
      await context.close();
      await browser.close();
    }
  });
}
