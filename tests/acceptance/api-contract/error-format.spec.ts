import { test, expect } from '@playwright/test';
import { apex, hostHeaders } from '../fixtures/env.js';

test('apex: an invalid domain returns 400 with a structured error', async ({ request }) => {
  const origin = apex();
  const response = await request.get(`${origin.url}/api/v1/domains/not..valid`, {
    headers: hostHeaders(origin),
  });
  expect(response.status()).toBe(400);
  const body = await response.json();
  expect(typeof body.error.code).toBe('string');
  expect(typeof body.error.message).toBe('string');
});
