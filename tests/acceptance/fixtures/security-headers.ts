import { expect, type APIResponse } from '@playwright/test';

export const PERMISSIONS_POLICY = 'camera=(), microphone=(), geolocation=(), payment=()';

export const TOOL_CSP =
  "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; " +
  "font-src 'self'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; " +
  "base-uri 'self'; object-src 'none'; form-action 'self'";

/** The `secure-headers` set (D3): every response of every origin. */
export function assertSecureHeaders(response: APIResponse): void {
  const h = response.headers();
  const hsts = h['strict-transport-security'] ?? '';
  expect(hsts, 'HSTS present').toBeTruthy();
  const maxAgeMatch = hsts.match(/max-age=(\d+)/);
  expect(maxAgeMatch, 'HSTS has max-age').toBeTruthy();
  expect(parseInt(maxAgeMatch![1], 10), 'HSTS max-age >= 31536000').toBeGreaterThanOrEqual(31536000);
  expect(hsts, 'HSTS includeSubDomains').toContain('includeSubDomains');
  expect(hsts, 'HSTS preload').toContain('preload');

  expect(h['x-frame-options'], 'x-frame-options').toBe('DENY');
  expect(h['x-content-type-options'], 'x-content-type-options').toBe('nosniff');
  expect(h['referrer-policy'], 'referrer-policy').toBe('strict-origin-when-cross-origin');
  expect(h['permissions-policy'], 'permissions-policy').toBe(PERMISSIONS_POLICY);
  expect(h['cross-origin-opener-policy'], 'cross-origin-opener-policy').toBe('same-origin');
}

/** Headers of the five tools and lens: secure-headers, csp-tool-spa, cors-public-api, no Server. */
export function assertToolHeaders(response: APIResponse): void {
  assertSecureHeaders(response);
  const h = response.headers();
  expect(h['content-security-policy'], 'content-security-policy').toBe(TOOL_CSP);
  expect(h['access-control-allow-origin'], 'access-control-allow-origin').toBe('*');
  expect(h['cross-origin-resource-policy'], 'cross-origin-resource-policy').toBe('cross-origin');
  expect(h['server'], 'no server header').toBeUndefined();
}

/** Headers of `netray site` (D4): secure-headers plus csp-netray-web. */
export function assertSiteHeaders(response: APIResponse): void {
  assertSecureHeaders(response);
  const h = response.headers();
  expect(h['content-security-policy'], 'content-security-policy').toContain(
    "connect-src 'self' https://stats.uptimerobot.com",
  );
  expect(h['cross-origin-resource-policy'], 'cross-origin-resource-policy').toBe('same-origin');
}

export function assertCorsPreflight(response: APIResponse): void {
  expect(response.status(), 'preflight 2xx').toBeGreaterThanOrEqual(200);
  expect(response.status(), 'preflight 2xx').toBeLessThan(300);
  const h = response.headers();
  expect(h['access-control-allow-origin'], 'preflight ACAO').toBe('*');
  const methods = (h['access-control-allow-methods'] ?? '').split(',').map((m) => m.trim().toUpperCase());
  for (const m of ['GET', 'POST', 'OPTIONS']) {
    expect(methods, `allow-methods includes ${m}`).toContain(m);
  }
  expect(h['access-control-max-age'], 'preflight max-age').toBe('600');
}

export function assertAppHeaders(response: APIResponse): void {
  const requestId = response.headers()['x-request-id'];
  expect(requestId, 'X-Request-Id present').toBeTruthy();
  expect(requestId!.length, 'X-Request-Id non-empty').toBeGreaterThan(0);
}
