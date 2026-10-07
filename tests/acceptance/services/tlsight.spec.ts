import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.tls;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /api/inspect?h=example.com returns ports and summary', async ({ request }) => {
  const response = await request.get(`${base}/api/inspect?h=example.com`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toHaveProperty('ports');
  expect(body).toHaveProperty('summary');
  expect(body.summary).toHaveProperty('checks');
});

test('GET /api/inspect?h=192.168.1.1 rejects RFC1918 target', async ({ request }) => {
  const response = await request.get(`${base}/api/inspect?h=192.168.1.1`);
  expect(response.status()).toBeGreaterThanOrEqual(400);
  expect(response.status()).toBeLessThan(500);
  const body = await response.json();
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
});
