import { expect, type APIResponse } from '@playwright/test';

export function assertTraefikHeaders(response: APIResponse): void {
  const hsts = response.headers()['strict-transport-security'] ?? '';
  expect(hsts, 'HSTS present').toBeTruthy();
  // Production uses max-age=31536000 (1 year)
  const maxAgeMatch = hsts.match(/max-age=(\d+)/);
  expect(maxAgeMatch, 'HSTS has max-age').toBeTruthy();
  expect(parseInt(maxAgeMatch![1], 10), 'HSTS max-age >= 31536000').toBeGreaterThanOrEqual(31536000);
  expect(hsts, 'HSTS includeSubDomains').toContain('includeSubDomains');
  expect(hsts, 'HSTS preload').toContain('preload');

  expect(response.headers()['x-content-type-options']).toBe('nosniff');
  expect(response.headers()['x-frame-options']).toBe('DENY');
  expect(response.headers()['referrer-policy']).toBe('strict-origin-when-cross-origin');
}

export function assertAppHeaders(response: APIResponse): void {
  const requestId = response.headers()['x-request-id'];
  expect(requestId, 'X-Request-Id present').toBeTruthy();
  expect(requestId!.length, 'X-Request-Id non-empty').toBeGreaterThan(0);
}
