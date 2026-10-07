import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.dns;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /api/query?q=example.com+A&stream=false returns events with A record', async ({ request }) => {
  const response = await request.get(`${base}/api/query?q=example.com+A&stream=false`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toHaveProperty('events');
  expect(Array.isArray(body.events)).toBe(true);

  // record_type is at the event.data level for batch events
  const batchEvents = body.events.filter((e: any) => e.type === 'batch');
  const hasARecord = batchEvents.some((e: any) => e.data?.record_type === 'A');
  expect(hasARecord, 'events should contain at least one batch with record_type A').toBe(true);
});

test('GET /api/servers returns JSON array', async ({ request }) => {
  const response = await request.get(`${base}/api/servers`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(Array.isArray(body)).toBe(true);
});

test('GET /api/query?q=&stream=false (empty) returns 4xx with structured error', async ({ request }) => {
  const response = await request.get(`${base}/api/query?q=&stream=false`);
  expect(response.status()).toBeGreaterThanOrEqual(400);
  expect(response.status()).toBeLessThan(500);
  const body = await response.json();
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
});
