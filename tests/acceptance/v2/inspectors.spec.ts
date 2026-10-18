import { test, expect, type APIRequestContext, type Page } from '@playwright/test';
import { mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveEnv } from '../fixtures/env.js';

// Spec v2-inspectors requirement 24 and the Phase 8 scenarios C7-C10.
// Needs the local stack with the seeded `example.com` entry (`just acceptance-local`).

const lens = resolveEnv().lens;
const SHOTS = resolve(dirname(fileURLToPath(import.meta.url)), '../test-results/v2-inspectors');
const DOMAIN = 'example.com';

const PROTOCOLS = ['dns', 'tls', 'http', 'email', 'ip'] as const;
type Protocol = (typeof PROTOCOLS)[number];

const INSPECTORS = [
  { tool: 'dns', field: 'q' },
  { tool: 'tls', field: 'h' },
  { tool: 'http', field: 'url' },
  { tool: 'ip', field: 'ip' },
] as const;

const WIDTHS = [
  { name: '1280', width: 1280, height: 900 },
  { name: '390', width: 390, height: 844 },
] as const;
const THEMES = ['light', 'dark'] as const;
type Theme = (typeof THEMES)[number];

// Explore-live derivation (crates/web/src/explore.rs), written out as literals.
const VIEW_LINK: Record<Protocol, string> = {
  dns: '/inspect/dns?q=example.com',
  tls: '/inspect/tls?h=example.com:443',
  http: '/inspect/http?url=https%3A%2F%2Fexample.com%2F',
  email: '/inspect/dns?q=example.com%20MX',
  ip: '/inspect/dns?q=example.com%20A',
};
const ROW_LINKS: Partial<Record<Protocol, string[]>> = {
  tls: [
    '/inspect/tls?h=192.0.2.10:443',
    '/inspect/tls?h=198.51.100.10:443',
    '/inspect/tls?h=%5B2001:db8::10%5D:443',
    '/inspect/tls?h=%5B2001:db8::11%5D:443',
  ],
  email: ['/inspect/tls?h=mx1.example.com:25'],
  ip: [
    '/inspect/ip?ip=192.0.2.10',
    '/inspect/ip?ip=198.51.100.10',
    '/inspect/ip?ip=2001:db8::10',
    '/inspect/ip?ip=2001:db8::11',
  ],
};

interface Spec {
  id: string;
  protocol: string;
  short: string;
}

// API v1 limits each client per minute: read the catalogue and the seeded history once per worker.
let catalogue: Spec[] | undefined;
let seeded: string[] | undefined;

async function catalogueShorts(request: APIRequestContext, protocol: Protocol): Promise<string[]> {
  if (!catalogue) {
    const res = await request.get(`${lens}/api/v1/catalogue`);
    expect(res.status(), 'GET /api/v1/catalogue').toBe(200);
    catalogue = ((await res.json()) as { checks: Spec[] }).checks;
  }
  return catalogue.filter((c) => c.protocol === protocol).map((c) => c.short);
}

async function seededEntryIds(request: APIRequestContext): Promise<string[]> {
  seeded ??= await entryIds(request, DOMAIN);
  return seeded;
}

async function entryIds(request: APIRequestContext, domain: string): Promise<string[]> {
  const res = await request.get(`${lens}/api/v1/domains/${domain}/history`);
  if (res.status() === 404) return [];
  expect(res.status(), `history of ${domain}`).toBe(200);
  const body = (await res.json()) as { entries: { id: string }[] };
  return body.entries.map((e) => e.id);
}

async function prepare(page: Page, width: (typeof WIDTHS)[number], theme: Theme): Promise<void> {
  await page.setViewportSize({ width: width.width, height: width.height });
  await page.emulateMedia({ colorScheme: theme });
  await page.addInitScript((t) => {
    try {
      globalThis.localStorage.setItem('theme', t);
    } catch {
      // storage unavailable: prefers-color-scheme still applies
    }
  }, theme);
}

async function open(page: Page, path: string, theme: Theme): Promise<void> {
  const res = await page.goto(`${lens}${path}`, { waitUntil: 'load' });
  expect(res?.status(), `GET ${path}`).toBe(200);
  await expect(page.locator('html')).toHaveAttribute('data-theme', theme);
}

async function expectNoPageScroll(page: Page): Promise<void> {
  const [scrollWidth, clientWidth] = await page.evaluate(() => [
    document.documentElement.scrollWidth,
    document.documentElement.clientWidth,
  ]);
  expect(scrollWidth, 'document scrollWidth <= clientWidth').toBeLessThanOrEqual(clientWidth);
}

async function shot(page: Page, view: string, width: string, theme: Theme): Promise<void> {
  mkdirSync(SHOTS, { recursive: true });
  await page.screenshot({ path: `${SHOTS}/${view}-${width}-${theme}.png`, fullPage: true });
}

async function hrefs(page: Page, selector: string): Promise<string[]> {
  return page.locator(selector).evaluateAll((els) => els.map((e) => e.getAttribute('href') ?? ''));
}

for (const protocol of PROTOCOLS) {
  for (const width of WIDTHS) {
    for (const theme of THEMES) {
      test(`evidence view ${DOMAIN}/${protocol} at ${width.name} px ${theme}`, async ({ page, request }) => {
        const ids = await seededEntryIds(request);
        expect(ids.length, 'the seeded entry exists').toBeGreaterThan(0);
        const entryId = ids[0];
        const shorts = await catalogueShorts(request, protocol);
        expect(shorts.length).toBeGreaterThan(0);

        await prepare(page, width, theme);
        await open(page, `/${DOMAIN}/${protocol}`, theme);

        // run line
        const runLine = page.locator('.run-line').first();
        await expect(runLine).toContainText(/run of \d{4}-\d{2}-\d{2} \d{2}:\d{2} UTC/);
        await expect(runLine).toContainText(`entry ${entryId}`);

        // check rows in catalogue order
        const codes = await page.locator('.check-row__code').allTextContents();
        expect(codes.map((c) => c.trim())).toEqual(shorts);

        // a shared block is a link line, never rendered in this view
        const shared = await hrefs(page, '.evidence-section__shared a');
        expect(new Set(shared).size, 'each shared block once').toBe(shared.length);
        for (const href of shared) {
          expect(href).toMatch(new RegExp(`^/r/${entryId}/(dns|tls|http|email|ip)#ev-[a-z]+\\.[a-z0-9_]+$`));
          expect(href).not.toContain(`/${protocol}#`);
        }
        const ownIds = await page
          .locator('.evidence-block')
          .evaluateAll((els) => els.map((e) => e.id));
        for (const id of ownIds) expect(id.startsWith(`ev-${protocol}.`), id).toBe(true);
        if (protocol === 'tls') {
          expect(shared).toContain(`/r/${entryId}/dns#ev-dns.https`);
        }

        // explore-live links
        const explore = await hrefs(page, '.evidence-view__explore');
        expect(explore[0]).toBe(VIEW_LINK[protocol]);
        for (const row of ROW_LINKS[protocol] ?? []) expect(explore).toContain(row);

        if (width.name === '390') await expectNoPageScroll(page);
        await shot(page, `${DOMAIN}-${protocol}`, width.name, theme);
      });
    }
  }
}

for (const { tool, field } of INSPECTORS) {
  for (const width of WIDTHS) {
    for (const theme of THEMES) {
      test(`inspector page /inspect/${tool} at ${width.name} px ${theme}`, async ({ page }) => {
        await prepare(page, width, theme);
        await open(page, `/inspect/${tool}`, theme);

        const form = page.locator('form.query-input');
        await expect(form).toBeVisible();
        await expect(form).toHaveAttribute('data-island', 'stream');
        await expect(form).toHaveAttribute('data-stream-url', `/inspect/${tool}/run`);
        await expect(form.locator(`input[name="${field}"]`)).toBeVisible();
        await expect(page.locator('#inspect-result')).toBeEmpty();
        await expect(page.locator('.grade')).toHaveCount(0);

        if (width.name === '390') await expectNoPageScroll(page);
        await shot(page, `inspect-${tool}`, width.name, theme);
      });
    }
  }
}

test('/inspect/email answers 404', async ({ request }) => {
  const res = await request.get(`${lens}/inspect/email`);
  expect(res.status()).toBe(404);
});

// C7
test('C7: /example.com/tls at 390 px dark names the entry, no page scroll, screenshot saved', async ({ page, request }) => {
  const [entryId] = await seededEntryIds(request);
  expect(entryId, 'the seeded entry exists').toBeTruthy();
  await prepare(page, WIDTHS[1], 'dark');
  await open(page, `/${DOMAIN}/tls`, 'dark');
  await expect(page.locator('.run-line').first()).toContainText(`entry ${entryId}`);
  await expectNoPageScroll(page);
  await shot(page, `${DOMAIN}-tls`, '390', 'dark');
});

// C8
for (const { tool } of INSPECTORS) {
  test(`C8: /inspect/${tool} at 1280 px light renders the form, screenshot saved`, async ({ page }) => {
    await prepare(page, WIDTHS[0], 'light');
    await open(page, `/inspect/${tool}`, 'light');
    await expect(page.locator('form.query-input')).toBeVisible();
    await shot(page, `inspect-${tool}`, '1280', 'light');
  });
}

// C9
test('C9: the email.mx row links to /inspect/tls?h=mx1.example.com:25', async ({ page }) => {
  await page.goto(`${lens}/${DOMAIN}/email`);
  const explore = await hrefs(page, '.evidence-view__explore');
  expect(explore).toContain('/inspect/tls?h=mx1.example.com:25');
});

// C10 and the live part of R24
const LIVE_BAND = '#inspect-band .spectrum';

async function expectLiveRun(page: Page): Promise<void> {
  await expect(page.locator('#inspect-result .run-line')).toContainText(/live\s*·\s*not recorded/, {
    timeout: 90_000,
  });
  await expect(page.locator(LIVE_BAND)).toBeVisible({ timeout: 90_000 });
  await expect(page.locator('#inspect-result .grade, .grade')).toHaveCount(0);
}

test('C10: /inspect/tls?h=netray.info streams, is not recorded and leaves the store unchanged @live', async ({ page, request }) => {
  const before = [await entryIds(request, DOMAIN), await entryIds(request, 'netray.info')];
  await page.goto(`${lens}/inspect/tls?h=netray.info`, { waitUntil: 'load' });
  await expectLiveRun(page);
  const after = [await entryIds(request, DOMAIN), await entryIds(request, 'netray.info')];
  expect(after).toEqual(before);
});

const LIVE_JOBS = [
  { tool: 'dns', query: 'q=example.com%20A' },
  { tool: 'tls', query: 'h=netray.info' },
  { tool: 'http', query: 'url=https%3A%2F%2Fexample.com%2F' },
  { tool: 'ip', query: 'ip=1.1.1.1' },
] as const;

for (const { tool, query } of LIVE_JOBS) {
  test(`live job /inspect/${tool}: live · not recorded, no grade, a band @live`, async ({ page }) => {
    await page.goto(`${lens}/inspect/${tool}?${query}`, { waitUntil: 'load' });
    await expectLiveRun(page);
  });
}
