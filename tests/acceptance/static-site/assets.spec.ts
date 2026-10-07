import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';

const urls = resolveEnv();
const siteUrl = urls.site;

const ASSETS = [
  '/robots.txt',
  '/favicon.svg',
  '/bimi-logo.svg',
  // TODO: mta-sts.txt is not served from the static site origin
];

for (const path of ASSETS) {
  test(`static site: ${path} returns 200 with non-empty body`, async ({ request }) => {
    const response = await request.get(`${siteUrl}${path}`);
    expect(response.status(), `${path} returns 200`).toBe(200);
    const body = await response.text();
    expect(body.length, `${path} body non-empty`).toBeGreaterThan(0);
  });
}
