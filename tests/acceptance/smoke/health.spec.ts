import { test, expect } from '@playwright/test';
import { resolveEnv, toolOrigins } from '../fixtures/env.js';

// Static site (nginx) has no /health endpoint; test only tool origins
// Additionally test that the static site root returns 200
test('site: GET / returns 200', async ({ request }) => {
  const urls = resolveEnv();
  const response = await request.get(`${urls.site}/`);
  expect(response.status()).toBe(200);
});

for (const { name, url } of toolOrigins()) {
  test(`${name}: GET /health returns 200 with status ok`, async ({ request }) => {
    test.fixme(name === 'email', 'beacon not yet deployed behind Traefik');
    const response = await request.get(`${url}/health`);
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body).toEqual({ status: 'ok' });
  });
}
