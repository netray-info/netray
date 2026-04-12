import { test } from '@playwright/test';
import { allOrigins, toolOrigins, isProduction } from '../fixtures/env.js';
import { assertTraefikHeaders, assertAppHeaders } from '../fixtures/security-headers.js';

for (const { name, url } of allOrigins()) {
  test(`${name}: Traefik security headers present`, async ({ request }) => {
    test.skip(!isProduction(), 'Traefik headers only in production');
    // Static site (nginx) has no /health endpoint; probe / instead
    const probe = name === 'site' ? '/' : '/health';
    const response = await request.get(`${url}${probe}`);
    assertTraefikHeaders(response);
  });
}

for (const { name, url } of toolOrigins()) {
  test(`${name}: X-Request-Id header present`, async ({ request }) => {
    const response = await request.get(`${url}/health`);
    assertAppHeaders(response);
  });
}
