import { test, expect } from '@playwright/test';
import { hostHeaders, origins } from '../fixtures/env.js';

for (const origin of origins()) {
  test(`${origin.name}: GET /health returns 200 with status ok`, async ({ request }) => {
    const response = await request.get(`${origin.url}/health`, { headers: hostHeaders(origin) });
    expect(response.status()).toBe(200);
    expect(await response.json()).toEqual({ status: 'ok' });
  });
}

// A V1 container answers /health on these hosts too; only the one 0.30.0 process redirects their
// /docs to the apex's API v1 docs (requirement 13h), so this tells the hosts reached it.
for (const origin of origins().filter((o) => o.name !== 'apex')) {
  test(`${origin.name}: served by the one process (/docs redirects to the apex)`, async ({ request }) => {
    const response = await request.get(`${origin.url}/docs`, {
      headers: hostHeaders(origin),
      maxRedirects: 0,
    });
    expect(response.status()).toBe(301);
    expect(response.headers()['location']).toMatch(/^https:\/\/[^/]+\/api\/v1\/docs$/);
  });
}
