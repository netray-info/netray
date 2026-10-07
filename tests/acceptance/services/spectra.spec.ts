import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.http;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /api/inspect?url=https://example.com returns quality verdict', async ({ request }) => {
  const response = await request.get(`${base}/api/inspect?url=https://example.com`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toHaveProperty('quality');
  expect(body.quality).toHaveProperty('verdict');
});

test('GET /api/inspect?url=http://localhost rejects SSRF target', async ({ request }) => {
  const response = await request.get(`${base}/api/inspect?url=http://localhost`);
  expect(response.status()).toBeGreaterThanOrEqual(400);
  expect(response.status()).toBeLessThan(500);
  const body = await response.json();
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
});
