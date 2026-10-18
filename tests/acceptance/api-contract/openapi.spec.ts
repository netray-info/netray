import { test, expect } from '@playwright/test';
import { apex, hostHeaders } from '../fixtures/env.js';
import { assertValidOpenApi } from '../fixtures/openapi.js';

const origin = apex();
const headers = hostHeaders(origin);

test('apex: /api/v1/openapi.json is valid OpenAPI 3.x', async ({ request }) => {
  const response = await request.get(`${origin.url}/api/v1/openapi.json`, { headers });
  expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toContain('application/json');
  assertValidOpenApi(await response.json());
});

test('apex: /api/v1/docs returns text/html', async ({ request }) => {
  const response = await request.get(`${origin.url}/api/v1/docs`, { headers });
  expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toContain('text/html');
});
