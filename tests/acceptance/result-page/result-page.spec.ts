import { test, expect, type APIRequestContext, type Page } from '@playwright/test';
import { mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { resolveEnv } from '../fixtures/env.js';

// Spec v2-result-page requirements 26 and 28 and the Phase 8 scenarios.
// Needs the seeded local stack: LENS_EXTRA="LENS_ADMISSION__CLIENT_BUDGET_PER_HOUR=0 LENS_CACHE__TTL_SECONDS=3600"
// just acceptance-local result-page/

const lens = process.env.LOCAL_LENS_URL ?? resolveEnv().lens;
const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../../..');
const SHOTS = resolve(ROOT, 'target/screenshots/v2-result-page');
const REFUSAL = 'too many checks from your address; try again in';

const WIDTHS = [
  { name: '390', width: 390, height: 844 },
  { name: '1440', width: 1440, height: 900 },
] as const;

async function shot(page: Page, state: string, width: string): Promise<void> {
  mkdirSync(SHOTS, { recursive: true });
  await page.screenshot({ path: resolve(SHOTS, `${state}-${width}.png`), fullPage: true });
}

async function visit(page: Page, domain: string): Promise<void> {
  await page.goto(`${lens}/${domain}`);
  await page.locator('#result').waitFor();
}

async function entryId(page: Page): Promise<string> {
  const text = await page.locator('#run-line').innerText();
  const m = /entry ([A-Za-z0-9_-]+)/.exec(text);
  expect(m, `run line "${text}" names an entry`).not.toBeNull();
  return m![1];
}

for (const w of WIDTHS) {
  test.describe(`result page at ${w.name} px`, () => {
    test.use({ viewport: { width: w.width, height: w.height } });

    test(`idle-recent ${w.name}`, async ({ page }) => {
      for (const d of ['shop.example.com', 'new.example.com', 'stale.example.com']) {
        await visit(page, d);
      }
      await page.goto(`${lens}/`);
      await shot(page, 'idle-recent', w.name);
    });

    test(`shop-fresh ${w.name}`, async ({ page }) => {
      await visit(page, 'shop.example.com');
      await expect(page.locator('#result')).toHaveAttribute('data-state', 'fresh');
      await shot(page, 'shop-fresh', w.name);
    });

    if (w.name === '1440') {
      test('shop-pinned 1440: a trace row click shows that run and marks the row', async ({ page }) => {
        await visit(page, 'shop.example.com');
        const row = page.locator('.trace__row[data-run="1"]');
        await row.click();
        await expect(row).toHaveAttribute('aria-pressed', 'true');
        await expect(page.locator('.spectrum__run[data-run="1"][data-shown]').first()).toBeAttached();
        await shot(page, 'shop-pinned', w.name);
      });
    } else {
      test('shop-stepped 390', async ({ page }) => {
        await visit(page, 'shop.example.com');
        await page.locator('[data-step="1"]').click();
        await shot(page, 'shop-stepped', w.name);
      });
    }

    test(`issues-severity ${w.name}`, async ({ page }) => {
      await visit(page, 'shop.example.com');
      await page.locator('.grouping-switch__option[data-group="severity"]').click();
      await expect(page.locator('.grouping-switch__option[data-group="severity"]')).toHaveAttribute('aria-pressed', 'true');
      await shot(page, 'issues-severity', w.name);
    });

    test(`new-first ${w.name}`, async ({ page }) => {
      await visit(page, 'new.example.com');
      await shot(page, 'new-first', w.name);
    });

    test(`stale-refused ${w.name}`, async ({ page }) => {
      await page.goto(`${lens}/stale.example.com`);
      await expect(page.locator('#result')).toHaveAttribute('data-state', 'stale');
      await expect(page.locator('#run-line')).toContainText(REFUSAL, { timeout: 30_000 });
      await expect(page.locator('#run-line')).toHaveText(/try again in \d+ s/);
      await shot(page, 'stale-refused', w.name);
    });

    test(`stale-nojs ${w.name}`, async ({ browser }) => {
      const context = await browser.newContext({
        javaScriptEnabled: false,
        viewport: { width: w.width, height: w.height },
      });
      try {
        const page = await context.newPage();
        const posts: string[] = [];
        page.on('request', (r) => {
          if (r.method() === 'POST') posts.push(r.url());
        });
        await page.goto(`${lens}/stale.example.com`);
        await expect(page.locator('#result-grade')).toHaveAttribute('data-grade', /.+/);
        await shot(page, 'stale-nojs', w.name);
        expect(posts).toEqual([]);
      } finally {
        await context.close();
      }
    });

    test(`history ${w.name}`, async ({ page }) => {
      await page.goto(`${lens}/shop.example.com/history`);
      await page.waitForLoadState('networkidle');
      await shot(page, 'history', w.name);
    });
  });
}

test.describe('requirement 26 at 390 px', () => {
  test.use({ viewport: { width: 390, height: 844 } });

  test('shop.example.com: no sideways scroll, targets at least 44 px, axis labels apart', async ({ page }) => {
    await visit(page, 'shop.example.com');

    const doc = await page.evaluate(() => ({
      sw: document.documentElement.scrollWidth,
      cw: document.documentElement.clientWidth,
    }));
    expect(doc.sw).toBeLessThanOrEqual(doc.cw);

    for (const sel of ['#result-header', '#diff', '#issues']) {
      const dims = await page.locator(sel).evaluate((e) => ({ sw: e.scrollWidth, cw: e.clientWidth }));
      expect(dims.sw, `${sel} scrollWidth`).toBeLessThanOrEqual(dims.cw);
    }

    const small: string[] = [];
    for (const sel of ['#result-header', '#diff', '#issues', '#spectrum-stepper']) {
      const targets = page.locator(`${sel} a, ${sel} button`);
      const n = await targets.count();
      for (let i = 0; i < n; i++) {
        const t = targets.nth(i);
        if (!(await t.isVisible())) continue;
        const box = await t.boundingBox();
        if (!box) continue;
        if (box.width < 44 || box.height < 44) {
          const label = ((await t.innerText()) || (await t.getAttribute('aria-label')) || '').trim().slice(0, 30);
          small.push(`${sel} "${label}" ${Math.round(box.width)}x${Math.round(box.height)}`);
        }
      }
    }
    expect(small).toEqual([]);

    const labels = page.locator('.spectrum__region-label');
    const boxes: { name: string; x: number; y: number; w: number; h: number }[] = [];
    const n = await labels.count();
    for (let i = 0; i < n; i++) {
      const l = labels.nth(i);
      if (!(await l.isVisible())) continue;
      const b = await l.boundingBox();
      if (b) boxes.push({ name: (await l.innerText()).trim(), x: b.x, y: b.y, w: b.width, h: b.height });
    }
    const overlaps: string[] = [];
    for (let i = 0; i < boxes.length; i++) {
      for (let j = i + 1; j < boxes.length; j++) {
        const a = boxes[i];
        const b = boxes[j];
        if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) {
          overlaps.push(`${a.name} / ${b.name}`);
        }
      }
    }
    expect(overlaps).toEqual([]);
  });
});

test.describe('behaviour', () => {
  test.use({ viewport: { width: 1440, height: 900 } });

  test('data-state is fresh on shop and stale on stale', async ({ page }) => {
    await visit(page, 'shop.example.com');
    await expect(page.locator('#result')).toHaveAttribute('data-state', 'fresh');
    await visit(page, 'stale.example.com');
    await expect(page.locator('#result')).toHaveAttribute('data-state', 'stale');
  });

  test('an issue evidence link opens the inspector view at the referenced block', async ({ page }) => {
    await visit(page, 'shop.example.com');
    const link = page.locator('#issues a.check-row__link[href^="/r/"]').first();
    const href = await link.getAttribute('href');
    expect(href).toMatch(/^\/r\/[^/]+\/[^/#]+#ev-.+/);
    await link.click();
    await page.waitForURL(/\/r\/[^/]+\/[^/#]+#ev-.+/);
    const [, hash] = page.url().split('#');
    const id = decodeURIComponent(hash);
    expect(id.startsWith('ev-')).toBe(true);
    const found = await page.evaluate((i) => document.querySelector(`#${CSS.escape(i)}`) !== null, id);
    expect(found, `element #${id} exists`).toBe(true);
  });
});

test.describe('og card', () => {
  test('og-shop.png is saved from the latest shop entry', async ({ page, request }: { page: Page; request: APIRequestContext }) => {
    await visit(page, 'shop.example.com');
    const id = await entryId(page);
    const res = await request.get(`${lens}/r/${id}/og.png`);
    expect(res.status()).toBe(200);
    mkdirSync(SHOTS, { recursive: true });
    writeFileSync(resolve(SHOTS, 'og-shop.png'), await res.body());
  });
});

test('screenshots: all eight states at both widths and og-shop.png', async () => {
  const states = [
    'idle-recent',
    'shop-fresh',
    'shop-pinned|shop-stepped',
    'issues-severity',
    'new-first',
    'stale-refused',
    'stale-nojs',
    'history',
  ];
  const files = readdirSync(SHOTS);
  const missing: string[] = [];
  for (const s of states) {
    for (const w of WIDTHS) {
      const alts = s.split('|');
      if (!alts.some((a) => files.includes(`${a}-${w.name}.png`))) missing.push(`${s}-${w.name}.png`);
    }
  }
  if (!files.includes('og-shop.png')) missing.push('og-shop.png');
  expect(missing).toEqual([]);
  const stateShots = files.filter((f) => f.endsWith('.png') && f !== 'og-shop.png');
  expect(stateShots).toHaveLength(16);
});
