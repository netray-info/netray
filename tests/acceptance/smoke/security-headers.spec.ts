import { test, expect } from '@playwright/test';
import { apex, hostHeaders, origins } from '../fixtures/env.js';
import {
  assertAppHeaders,
  assertCorsPreflight,
  assertSecureHeaders,
  assertSiteHeaders,
} from '../fixtures/security-headers.js';

for (const origin of origins()) {
  test(`${origin.name}: security headers on GET /health`, async ({ request }) => {
    const response = await request.get(`${origin.url}/health`, { headers: hostHeaders(origin) });
    assertSecureHeaders(response);
  });

  test(`${origin.name}: CORS preflight on /health`, async ({ request }) => {
    const response = await request.fetch(`${origin.url}/health`, {
      method: 'OPTIONS',
      headers: {
        ...hostHeaders(origin),
        Origin: 'https://example.com',
        'Access-Control-Request-Method': 'POST',
        'Access-Control-Request-Headers': 'content-type',
      },
    });
    assertCorsPreflight(response);
  });

  test(`${origin.name}: X-Request-Id header present`, async ({ request }) => {
    const response = await request.get(`${origin.url}/health`, { headers: hostHeaders(origin) });
    assertAppHeaders(response);
  });
}

test('apex: security headers on GET /guide/', async ({ request }) => {
  const origin = apex();
  const response = await request.get(`${origin.url}/guide/`, { headers: hostHeaders(origin) });
  expect(response.status()).toBe(200);
  assertSiteHeaders(response);
});
