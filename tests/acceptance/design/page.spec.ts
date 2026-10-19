import { test, expect } from '@playwright/test';
import { createServer, type Server } from 'node:http';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Spec v2-design-gate requirement 4 (C3, C8): the page side of the design gate.
// Run by hand: cd tests/acceptance && npx playwright test --project design

const FIXTURE = `<!doctype html>
<html><head><meta charset="utf-8"><style>
  body { margin: 0; height: 2000px; }
  h1 { color: rgb(10, 20, 30); }
  @media (prefers-color-scheme: dark) { h1 { color: rgb(200, 210, 220); } }
  [data-theme=dark] h1 { color: rgb(200, 210, 220); }
  [data-theme=light] h1 { color: rgb(10, 20, 30); }
</style>
<script>
  var t = localStorage.getItem('theme');
  if (t === 'light' || t === 'dark') document.documentElement.setAttribute('data-theme', t);
</script></head>
<body><h1 id="title">Fixture</h1></body></html>`;

let server: Server;
let baseUrls: Record<string, string>;
const outDir = mkdtempSync(join(tmpdir(), 'design-page-'));

test.beforeAll(async () => {
  server = createServer((_req, res) => {
    res.setHeader('content-type', 'text/html');
    res.end(FIXTURE);
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  const addr = server.address();
  if (addr === null || typeof addr === 'string') throw new Error('no port');
  baseUrls = { lens: `http://127.0.0.1:${addr.port}` };
});

test.afterAll(async () => {
  await new Promise((ok) => server.close(ok));
});

function board(width: number, theme: 'light' | 'dark') {
  return {
    title: `Fixture ${theme} ${width}`,
    host: 'lens',
    path: '/',
    width,
    theme,
    elements: [
      { name: 'heading', board: 'Heading', page: '#title', class: 'text', properties: ['color'] },
      { name: 'ghost', board: 'Ghost', page: '#does-not-exist', class: 'text', properties: ['color'] },
    ],
  };
}

function pngWidth(path: string): number {
  return readFileSync(path).readUInt32BE(16);
}

test('page side captures a dark 390 board at its width and theme', async ({ browser }) => {
  const { measurePage } = await import('./page.mjs');
  const { slug } = await import('./manifest.mjs');
  const b = board(390, 'dark');
  const r = await measurePage(browser, b, baseUrls, outDir);

  expect(r.screenshot).toBe(join(outDir, `${slug(b.title)}-page.png`));
  expect(existsSync(r.screenshot)).toBe(true);
  expect(pngWidth(r.screenshot)).toBe(390);
  expect(r.colorScheme).toBe('dark');
  expect(r.theme).toBe('dark');
  expect(r.values.heading).toEqual({ color: 'rgb(200, 210, 220)' });
  expect(r.values.ghost).toBeNull();
});

test('page side captures a light 1280 board', async ({ browser }) => {
  const { measurePage } = await import('./page.mjs');
  const b = board(1280, 'light');
  const r = await measurePage(browser, b, baseUrls, outDir);

  expect(pngWidth(r.screenshot)).toBe(1280);
  expect(r.colorScheme).toBe('light');
  expect(r.theme).toBe('light');
  expect(r.values.heading).toEqual({ color: 'rgb(10, 20, 30)' });
});
