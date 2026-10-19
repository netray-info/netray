import { test, expect } from '@playwright/test';
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// Spec v2-design-gate requirement 3 and 6 (side-by-side): the board side of the design gate.
// Run by hand: cd tests/acceptance && npx playwright test --project design

const BOARD_DOC = `<!doctype html><html><body style="margin:0">
<h1 style="font-size:40px;font-weight:650;color:rgb(15, 18, 22)" data-dc-tpl="domain">example.com</h1>
<div style="height:2000px"></div></body></html>`;
const OTHER_DOC = `<!doctype html><html><body style="margin:0"><p style="margin:0">other</p></body></html>`;

const FIXTURE = `<!doctype html><html><head><meta charset="utf-8"></head><body>
<iframe title="Fixture board — light" width="390" style="width:390px;height:300px" src="about:blank#a1"></iframe>
<iframe title="Other board" width="1280" style="width:1280px;height:300px" src="about:blank#b2"></iframe>
<script>
  const docs = ${JSON.stringify([BOARD_DOC, OTHER_DOC])};
  document.querySelectorAll('iframe').forEach((f, i) => {
    f.contentDocument.open();
    f.contentDocument.write(docs[i]);
    f.contentDocument.close();
  });
</script></body></html>`;

const outDir = mkdtempSync(join(tmpdir(), 'design-board-'));
const exportPath = join(outDir, 'export.html');
writeFileSync(exportPath, FIXTURE);

function board(elements: object[]) {
  return { title: 'Fixture board — light', width: 390, theme: 'light', elements };
}

const el = (name: string, boardSel: string) => ({
  name,
  board: boardSel,
  page: '#x',
  class: 'text',
  properties: ['font-size', 'font-weight', 'color'],
});

function png(path: string) {
  const b = readFileSync(path);
  return { width: b.readUInt32BE(16), height: b.readUInt32BE(20) };
}

test('board side grows the iframe to its content and reads computed values', async ({ browser }) => {
  const { openExport, measureBoard } = await import('./board.mjs');
  const { slug } = await import('./manifest.mjs');
  const page = await openExport(browser, exportPath);
  const b = board([el('h1 domain', 'h1'), { ...el('ghost', 'h2.missing'), properties: ['color'] }]);
  const r = await measureBoard(page, b, outDir);

  expect(r.screenshot).toBe(join(outDir, `${slug(b.title)}-board.png`));
  expect(existsSync(r.screenshot)).toBe(true);
  const dims = png(r.screenshot);
  expect(dims.width).toBe(390);
  expect(dims.height).toBeGreaterThanOrEqual(2000);
  expect(r.values['h1 domain']).toEqual({
    'font-size': '40px',
    'font-weight': '650',
    color: 'rgb(15, 18, 22)',
  });
  expect(r.values.ghost).toBeNull();
});

test('board side rejects a title absent from the export, naming it', async ({ browser }) => {
  const { openExport, measureBoard } = await import('./board.mjs');
  const page = await openExport(browser, exportPath);
  const b = { ...board([]), title: 'No such board' };
  await expect(measureBoard(page, b, outDir)).rejects.toThrow(/No such board/);
});

test('sideBySide puts both images next to each other at natural size', async ({ browser }) => {
  const { openExport, measureBoard, sideBySide } = await import('./board.mjs');
  const page = await openExport(browser, exportPath);
  const r = await measureBoard(page, board([]), outDir);
  const one = png(r.screenshot);
  const out = join(outDir, 'side.png');
  await sideBySide(browser, r.screenshot, r.screenshot, out);

  expect(existsSync(out)).toBe(true);
  const dims = png(out);
  expect(dims.width).toBeGreaterThanOrEqual(one.width * 2);
  expect(dims.height).toBe(one.height);
});
