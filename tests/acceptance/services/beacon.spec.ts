import { test, expect } from '@playwright/test';
import { resolveEnv } from '../fixtures/env.js';
import { consumeSSE } from '../fixtures/sse.js';
import { INTER_TEST_DELAY_MS } from '../playwright.config.js';

const urls = resolveEnv();
const base = urls.email;

test.afterEach(async () => {
  await new Promise(r => setTimeout(r, INTER_TEST_DELAY_MS));
});

test('GET /inspect/example.com SSE stream contains mx, spf, dmarc and summary', async () => {
  test.fixme(true, 'beacon not yet deployed behind Traefik');
  const events = await consumeSSE(
    `${base}/inspect/example.com`,
    { 'User-Agent': 'netray-acceptance-tests/1.0' },
    30_000,
  );

  expect(events.length).toBeGreaterThan(0);

  // Check for category events
  const categoryEvents = events.filter(e => e.event === 'category');
  const categories = categoryEvents.map(e => {
    try { return JSON.parse(e.data); } catch { return null; }
  }).filter(Boolean);

  const categoryNames = categories.map((c: any) => c.category ?? c.name);
  expect(categoryNames, 'should include mx category').toContain('mx');
  expect(categoryNames, 'should include spf category').toContain('spf');
  expect(categoryNames, 'should include dmarc category').toContain('dmarc');

  // Check for summary event with grade
  const summaryEvents = events.filter(e => e.event === 'summary');
  expect(summaryEvents.length, 'should have a summary event').toBeGreaterThan(0);
  const summary = JSON.parse(summaryEvents[0].data);
  expect(summary).toHaveProperty('grade');
  expect(summary.grade).toMatch(/^[A-F]$/);
});
