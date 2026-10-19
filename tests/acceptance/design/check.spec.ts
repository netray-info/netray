import { test, expect } from '@playwright/test';
import { spawn } from 'node:child_process';
import { createServer, type Server } from 'node:http';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

// Spec v2-design-gate requirements 3, 6 and 7 end to end: check.mjs against a fixture export and
// a fixture page server; its exit status follows the comparison, and a board without a state
// still gets its board image and side-by-side.
// Run by hand: cd tests/acceptance && npx playwright test --project design

const CHECK = join(dirname(fileURLToPath(import.meta.url)), 'check.mjs');

const BOARD_DOC = `<!doctype html><html><body style="margin:0">
<h1 style="font-size:40px;font-weight:650">example.com</h1></body></html>`;

const EXPORT = `<!doctype html><html><head><meta charset="utf-8"></head><body>
<iframe title="Board one" width="390" style="width:390px;height:200px" src="about:blank#a1"></iframe>
<script>
  document.querySelectorAll('iframe').forEach((f) => {
    f.contentDocument.open();
    f.contentDocument.write(${JSON.stringify(BOARD_DOC)});
    f.contentDocument.close();
  });
</script></body></html>`;

const page = (size: number) => `<!doctype html><html><body style="margin:0">
<h1 id="d" style="font-size:${size}px;font-weight:650">shop.example.com</h1></body></html>`;

let server: Server;
let base: string;
const dir = mkdtempSync(join(tmpdir(), 'design-check-'));
writeFileSync(join(dir, 'canvas.html'), EXPORT);

test.beforeAll(async () => {
  server = createServer((req, res) => {
    res.setHeader('content-type', 'text/html');
    if (req.url === '/match') res.end(page(40));
    else if (req.url === '/off') res.end(page(44));
    else {
      res.statusCode = 404;
      res.end('<!doctype html><p>not found</p>');
    }
  });
  await new Promise<void>((ok) => server.listen(0, '127.0.0.1', ok));
  const addr = server.address();
  if (addr === null || typeof addr === 'string') throw new Error('no port');
  base = `http://127.0.0.1:${addr.port}`;
});

test.afterAll(async () => {
  await new Promise((ok) => server.close(ok));
});

// Async: the fixture server answers from this process, so a synchronous spawn would starve it.
async function run(path: string) {
  const manifest = join(dir, `manifest${path.replace('/', '-')}.json`);
  writeFileSync(
    manifest,
    JSON.stringify({
      export: 'canvas.html',
      boards: [
        {
          title: 'Board one',
          host: 'lens',
          path,
          width: 390,
          theme: 'light',
          elements: [{ name: 'h1', board: 'h1', page: '#d', class: 'typography', properties: ['font-size'] }],
        },
      ],
    }),
  );
  const out = join(dir, `out${path.replace('/', '-')}`);
  const child = spawn(process.execPath, [CHECK, manifest, out], {
    env: { ...process.env, LOCAL_LENS_URL: base },
  });
  let stdout = '';
  let stderr = '';
  child.stdout.on('data', (d) => (stdout += d));
  child.stderr.on('data', (d) => (stderr += d));
  const status = await new Promise<number | null>((ok) => child.on('close', ok));
  return { status, stdout, stderr, out };
}

test('a page within tolerance exits 0', async () => {
  const r = await run('/match');
  expect(r.status, r.stderr).toBe(0);
  expect(readFileSync(join(r.out, 'report.md'), 'utf8')).toContain('None.');
});

test('a page outside tolerance exits 1 with its row', async () => {
  const r = await run('/off');
  expect(r.status, r.stderr).toBe(1);
  expect(r.stdout).toContain('Board one: 1 deviation');
  expect(readFileSync(join(r.out, 'report.md'), 'utf8')).toContain('h1 font-size board 40px page 44px (typography)');
});

test('a board without a state exits 1 and still gets its board image and side-by-side', async () => {
  const r = await run('/gone');
  expect(r.status, r.stderr).toBe(1);
  expect(readFileSync(join(r.out, 'report.md'), 'utf8')).toContain('no state (HTTP 404)');
  expect(existsSync(join(r.out, 'board-one-board.png'))).toBe(true);
  expect(existsSync(join(r.out, 'board-one-sbs.png'))).toBe(true);
});
