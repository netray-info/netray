import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';

const urls = resolveEnv();

test.describe.configure({ mode: 'serial' });

test('rapid requests trigger 429 with Retry-After header', async ({ request }) => {
  test.slow();
  test.setTimeout(30_000);

  // Use ifconfig /ip/json as the target - lightweight endpoint
  const target = `${urls.ip}/ip/json`;
  let got429 = false;

  for (let i = 0; i < 200; i++) {
    const response = await request.get(target);
    if (response.status() === 429) {
      const retryAfter = response.headers()['retry-after'];
      expect(retryAfter, 'Retry-After header present on 429').toBeTruthy();
      const retryValue = parseInt(retryAfter!, 10);
      expect(retryValue, 'Retry-After is a positive number').toBeGreaterThan(0);
      got429 = true;
      break;
    }
  }

  expect(got429, 'Should have received a 429 response').toBe(true);
});
