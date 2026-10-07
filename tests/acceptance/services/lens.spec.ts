import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.lens;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /api/check/example.com?stream=false returns dns, tls, summary with grade', async ({ request }) => {
  const response = await request.get(`${base}/api/check/example.com?stream=false`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toHaveProperty('dns');
  expect(body).toHaveProperty('tls');
  expect(body).toHaveProperty('summary');
  expect(body.summary).toHaveProperty('grade');
  expect(body.summary.grade).toMatch(/^[A-F]$/);
});
