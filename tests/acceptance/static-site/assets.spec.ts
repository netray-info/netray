import { test, expect } from '@playwright/test';
import { resolveEnv, isProduction } from '../fixtures/env.js';

const urls = resolveEnv();
const siteUrl = urls.site;

const ASSETS = [
  '/robots.txt',
  '/favicon.svg',
  '/bimi-logo.svg',
];

for (const path of ASSETS) {
  test(`static site: ${path} returns 200 with non-empty body`, async ({ request }) => {
    const response = await request.get(`${siteUrl}${path}`);
    expect(response.status(), `${path} returns 200`).toBe(200);
    const body = await response.text();
    expect(body.length, `${path} body non-empty`).toBeGreaterThan(0);
  });
}

test('mta-sts: policy served with STSv1 enforce', async ({ request }) => {
  // Production: dedicated host. Local: `netray site` selects the policy by a Host header starting with `mta-sts.`.
  const response = isProduction()
    ? await request.get('https://mta-sts.netray.info/.well-known/mta-sts.txt')
    : await request.get(`${siteUrl}/.well-known/mta-sts.txt`, { headers: { Host: 'mta-sts.localhost' } });
  expect(response.status(), 'mta-sts.txt returns 200').toBe(200);
  const body = await response.text();
  expect(body).toContain('version: STSv1');
  expect(body).toContain('mode: enforce');
});
