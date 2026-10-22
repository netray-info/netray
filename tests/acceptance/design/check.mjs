// The design check's runner (specs/features/v2-design-gate, specs/rules/design-rules.md):
// `just design-check` starts the stack, then calls
// `node tests/acceptance/design/check.mjs <manifest> <out dir>`.
// Exit 0: every measured property within tolerance or accepted and every board in a state;
// 1: open deviations or stale accepted entries; 2: manifest or setup error.
import { existsSync, mkdirSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { boardName, boardSlug, loadManifest, ManifestError, resolveExport } from './manifest.mjs';
import { measurePage } from './page.mjs';
import { measureBoard, openExport, sideBySide } from './board.mjs';
import {
  applyAccepted,
  deviations,
  liveState,
  noStateDeviation,
  reportMarkdown,
  summary,
} from './compare.mjs';

function setupError(msg) {
  console.error(`design-check: ${msg}`);
  process.exit(2);
}

const [manifestPath, outDir] = process.argv.slice(2);
if (!manifestPath || !outDir) setupError('usage: check.mjs <manifest> <out dir>');

let manifest;
try {
  manifest = loadManifest(manifestPath);
} catch (e) {
  setupError(e instanceof ManifestError ? e.message : `cannot read ${manifestPath}: ${e.message}`);
}

const exportPath = resolveExport(manifest, manifestPath);
if (!existsSync(exportPath)) setupError(`export not found: ${exportPath}`);

const baseUrls = {};
for (const host of new Set(manifest.boards.map((b) => b.host))) {
  const url = process.env[`LOCAL_${host.toUpperCase()}_URL`];
  if (!url) setupError(`no LOCAL_${host.toUpperCase()}_URL for host ${host}`);
  baseUrls[host] = url;
}

try {
  mkdirSync(outDir, { recursive: true });
  // Drop this check's files of an earlier run, so no image or report outlives its board.
  for (const f of readdirSync(outDir)) {
    if (/-(board|page|sbs)\.png$|^page\.json$|^report\.md$/.test(f)) rmSync(join(outDir, f));
  }
} catch (e) {
  setupError(`cannot create ${outDir}: ${e.message}`);
}

let browser;
try {
  // Imported here so a missing tests/acceptance/node_modules is a setup error (exit 2).
  const { chromium } = await import('@playwright/test');
  browser = await chromium.launch();
} catch (e) {
  setupError(`cannot launch chromium: ${e.message}`);
}

const found = [];
const sideBySides = [];
const pages = [];
try {
  const exportPage = await openExport(browser, exportPath);
  for (const board of manifest.boards) {
    const page = await measurePage(browser, board, baseUrls, outDir);
    pages.push(page);
    const measured = await measureBoard(exportPage, board, outDir);
    // A page that failed to answer, or a result page that would start a live run, has no state:
    // one `no state` row instead of its measured rows; its images are written all the same.
    if (page.status === null || page.status >= 400) {
      found.push(noStateDeviation(board, `HTTP ${page.status}`));
    } else if (page.state !== null && page.state !== 'fresh') {
      found.push(noStateDeviation(board, `data-state=${page.state}`));
    } else {
      const reason = liveState(board, page.values, page.waited);
      if (reason) found.push(noStateDeviation(board, reason));
      else found.push(...deviations(board, measured.values, page.values));
    }
    const sbs = join(outDir, `${boardSlug(board)}-sbs.png`);
    await sideBySide(browser, measured.screenshot, page.screenshot, sbs);
    sideBySides.push(sbs);
  }
} catch (e) {
  await browser.close();
  setupError(e.message);
}
await browser.close();

const { open, accepted, stale } = applyAccepted(found, manifest.accepted ?? []);
try {
  writeFileSync(join(outDir, 'page.json'), JSON.stringify(pages, null, 2) + '\n');
  writeFileSync(join(outDir, 'report.md'), reportMarkdown({ open, accepted, stale, sideBySides }));
} catch (e) {
  setupError(`cannot write the report into ${outDir}: ${e.message}`);
}
const result = summary({ open, stale, titles: manifest.boards.map(boardName) });
for (const line of result.lines) console.log(line);
console.log(`report: ${join(outDir, 'report.md')}`);
process.exit(result.status);
