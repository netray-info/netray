import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.ip;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /ip/json returns JSON with addr', async ({ request }) => {
  const response = await request.get(`${base}/ip/json`);
  expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toContain('application/json');
  const body = await response.json();
  expect(body).toHaveProperty('addr');
});

test('GET /ip returns YAML via Accept header', async ({ request }) => {
  const response = await request.get(`${base}/ip/yaml`);
  expect(response.status()).toBe(200);
  const ct = response.headers()['content-type'];
  expect(ct).toMatch(/yaml/);
});

test('GET /ip returns text/plain via Accept header', async ({ request }) => {
  const response = await request.fetch(`${base}/ip`, {
    headers: { Accept: 'text/plain' },
  });
  expect(response.status()).toBe(200);
  expect(response.headers()['content-type']).toContain('text/plain');
});

test('GET /all/json?ip=8.8.8.8 returns JSON with network info', async ({ request }) => {
  const response = await request.get(`${base}/all/json?ip=8.8.8.8`);
  expect(response.status()).toBe(200);
  const body = await response.json();
  expect(body).toHaveProperty('network');
  expect(body.network).toHaveProperty('org');
});

test('GET /all/json?ip=10.0.0.1 rejects private IP', async ({ request }) => {
  const response = await request.get(`${base}/all/json?ip=10.0.0.1`);
  expect(response.status()).toBeGreaterThanOrEqual(400);
  expect(response.status()).toBeLessThan(500);
  const body = await response.json();
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
});

test('GET /all/json?ip=127.0.0.1 rejects loopback IP', async ({ request }) => {
  const response = await request.get(`${base}/all/json?ip=127.0.0.1`);
  expect(response.status()).toBeGreaterThanOrEqual(400);
  expect(response.status()).toBeLessThan(500);
  const body = await response.json();
  expect(body).toHaveProperty('error');
  expect(body.error).toHaveProperty('code');
  expect(body.error).toHaveProperty('message');
});
