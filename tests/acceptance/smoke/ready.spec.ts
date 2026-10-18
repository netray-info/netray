import { test, expect } from '@playwright/test';
import { hostHeaders, origins } from '../fixtures/env.js';

for (const origin of origins()) {
  test(`${origin.name}: GET /ready returns 200 with status ok or ready`, async ({ request }) => {
    const response = await request.get(`${origin.url}/ready`, { headers: hostHeaders(origin) });
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body).toHaveProperty('status');
    expect(['ok', 'ready']).toContain(body.status);
  });
}
