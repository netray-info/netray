import { test, expect } from '@playwright/test';
import { resolveEnv, toolOrigins } from '../fixtures/env.js';
import {
  assertToolHeaders,
  assertSiteHeaders,
  assertCorsPreflight,
  assertAppHeaders,
} from '../fixtures/security-headers.js';

for (const { name, url } of toolOrigins()) {
  for (const path of ['/', '/health']) {
    test(`${name}: security headers on GET ${path}`, async ({ request }) => {
      const response = await request.get(`${url}${path}`);
      assertToolHeaders(response);
    });
  }

  test(`${name}: CORS preflight on /health`, async ({ request }) => {
    const response = await request.fetch(`${url}/health`, {
      method: 'OPTIONS',
      headers: {
        Origin: 'https://example.com',
        'Access-Control-Request-Method': 'POST',
        'Access-Control-Request-Headers': 'content-type',
      },
    });
    assertCorsPreflight(response);
  });

  test(`${name}: X-Request-Id header present`, async ({ request }) => {
    const response = await request.get(`${url}/health`);
    assertAppHeaders(response);
  });
}

test('site: security headers on GET /guide/', async ({ request }) => {
  const response = await request.get(`${resolveEnv().site}/guide/`);
  expect(response.status()).toBe(200);
  assertSiteHeaders(response);
});
