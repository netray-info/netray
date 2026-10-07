import { test, expect } from '@playwright/test';
import { toolOrigins } from '../fixtures/env.js';
import { assertValidOpenApi } from '../fixtures/openapi.js';

for (const { name, url } of toolOrigins()) {
  test(`${name}: /api-docs/openapi.json is valid OpenAPI 3.x`, async ({ request }) => {
    test.fixme(name === 'email', 'beacon not yet deployed behind Traefik');
    const response = await request.get(`${url}/api-docs/openapi.json`);
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('application/json');
    const json = await response.json();
    assertValidOpenApi(json);
  });

  test(`${name}: /docs returns text/html`, async ({ request }) => {
    test.fixme(name === 'email', 'beacon not yet deployed behind Traefik');
    const response = await request.get(`${url}/docs`);
    expect(response.status()).toBe(200);
    expect(response.headers()['content-type']).toContain('text/html');
  });
}
