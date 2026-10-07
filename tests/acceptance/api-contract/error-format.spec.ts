import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';

const urls = resolveEnv();

function assertStructuredError(body: any): void {
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
  expect(typeof body.error.code).toBe('string');
  expect(typeof body.error.message).toBe('string');
}

const MALFORMED_REQUESTS: { name: string; url: string }[] = [
  { name: 'ifconfig: private IP', url: `${urls.ip}/all/json?ip=10.0.0.1` },
  { name: 'prism: empty query', url: `${urls.dns}/api/query?q=&stream=false` },
  { name: 'tlsight: empty host', url: `${urls.tls}/api/inspect?h=` },
  { name: 'spectra: missing url param', url: `${urls.http}/api/inspect` },
  { name: 'beacon: invalid domain', url: `${urls.email}/inspect/not..valid` },
  { name: 'lens: invalid domain', url: `${urls.lens}/api/check/not..valid?stream=false` },
];

for (const { name, url } of MALFORMED_REQUESTS) {
  test(`${name}: malformed request returns 4xx with structured error`, async ({ request }) => {
    test.fixme(name.startsWith('beacon'), 'beacon not yet deployed behind Traefik');
    const response = await request.get(url);
    expect(response.status()).toBeGreaterThanOrEqual(400);
    expect(response.status()).toBeLessThan(500);
    const body = await response.json();
    assertStructuredError(body);
  });
}
