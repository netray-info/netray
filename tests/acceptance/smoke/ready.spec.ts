import { test, expect } from '@playwright/test';
import { toolOrigins } from '../fixtures/env.js';

const origins = toolOrigins();

for (const { name, url } of origins) {
  test(`${name}: GET /ready returns 200 with status ok or ready`, async ({ request }) => {
    const response = await request.get(`${url}/ready`);
    expect(response.status()).toBe(200);
    const body = await response.json();
    expect(body).toHaveProperty('status');
    expect(['ok', 'ready']).toContain(body.status);
  });
}
