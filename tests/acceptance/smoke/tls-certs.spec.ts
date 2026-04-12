import { test, expect } from '@playwright/test';
import * as tls from 'node:tls';
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

const MIN_DAYS_REMAINING = 14;

function getCertExpiry(host: string): Promise<Date> {
  return new Promise((resolve, reject) => {
    const socket = tls.connect(443, host, { servername: host }, () => {
      const cert = socket.getPeerCertificate();
      socket.destroy();
      if (!cert || !cert.valid_to) {
        reject(new Error(`No certificate returned for ${host}`));
        return;
      }
      resolve(new Date(cert.valid_to));
    });
    socket.on('error', reject);
    socket.setTimeout(10_000, () => {
      socket.destroy();
      reject(new Error(`TLS connection to ${host} timed out`));
    });
  });
}

for (const host of PRODUCTION_HOSTS) {
  test(`${host}: TLS certificate not expiring within ${MIN_DAYS_REMAINING} days`, async () => {
    test.skip(!isProduction(), 'TLS cert checks only in production');

    const expiry = await getCertExpiry(host);
    const daysRemaining = (expiry.getTime() - Date.now()) / (1000 * 60 * 60 * 24);
    expect(daysRemaining, `Certificate for ${host} expires in ${Math.floor(daysRemaining)} days`).toBeGreaterThan(MIN_DAYS_REMAINING);
  });
}
