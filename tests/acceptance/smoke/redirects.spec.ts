import { test, expect } from '@playwright/test';
import { isProduction } from '../fixtures/env.js';

const PRODUCTION_HOSTS = [
  'netray.info',
  'ip.netray.info',
  'dns.netray.info',
  'tls.netray.info',
  'http.netray.info',
  'email.netray.info',
  'lens.netray.info',
];

for (const host of PRODUCTION_HOSTS) {
  test(`${host}: HTTP redirects to HTTPS`, async () => {
    test.skip(!isProduction(), 'Redirect checks only in production');

    const response = await fetch(`http://${host}/`, { redirect: 'manual' });
    expect([301, 308], `Expected 301 or 308 but got ${response.status}`).toContain(response.status);
    const location = response.headers.get('location') ?? '';
    expect(location).toContain(`https://${host}`);
  });
}
