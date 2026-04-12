import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';

const urls = resolveEnv();
const siteUrl = urls.site;

test('all SuiteNav links in index.html resolve to 200', async ({ request }) => {
  const response = await request.get(`${siteUrl}/`);
  expect(response.status()).toBe(200);
  const html = await response.text();

  // Extract href values from <a> tags with class containing suite-nav__link
  const linkMatches = html.matchAll(/<a[^>]*class="[^"]*suite-nav__link[^"]*"[^>]*href="([^"]+)"/g);
  const hrefs = Array.from(linkMatches, (m: RegExpMatchArray) => m[1]);

  expect(hrefs.length, 'Found SuiteNav links').toBeGreaterThan(0);

  for (const href of hrefs) {
    const url = href.startsWith('http') ? href : `${siteUrl}${href}`;
    const linkResponse = await request.get(url);
    expect(linkResponse.status(), `SuiteNav link ${href} resolves`).toBe(200);
  }
});
