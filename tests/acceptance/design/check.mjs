// The design check's runner (specs/features/v2-design-gate): `just design-check` starts the
// stack, then calls `node tests/acceptance/design/check.mjs <manifest> <out dir>`.
// Exit 0: every board has a state; 1: a board has none; 2: manifest or setup error.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { loadManifest, ManifestError } from './manifest.mjs';
import { measurePage } from './page.mjs';

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

const baseUrls = {};
for (const host of new Set(manifest.boards.map((b) => b.host))) {
  const url = process.env[`LOCAL_${host.toUpperCase()}_URL`];
  if (!url) setupError(`no LOCAL_${host.toUpperCase()}_URL for host ${host}`);
  baseUrls[host] = url;
}

try {
  mkdirSync(outDir, { recursive: true });
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

const pages = [];
const noState = [];
try {
  for (const board of manifest.boards) {
    const r = await measurePage(browser, board, baseUrls, outDir);
    pages.push(r);
    // A page that failed to answer, or a result page that would start a live run, has no state.
    if (r.status === null || r.status >= 400) noState.push(`${board.title}: no state (HTTP ${r.status})`);
    else if (r.state !== null && r.state !== 'fresh') noState.push(`${board.title}: no state (data-state=${r.state})`);
  }
} catch (e) {
  await browser.close();
  setupError(`page side failed: ${e.message}`);
}
await browser.close();

try {
  writeFileSync(join(outDir, 'page.json'), JSON.stringify(pages, null, 2) + '\n');
} catch (e) {
  setupError(`cannot write ${outDir}/page.json: ${e.message}`);
}
for (const line of noState) console.log(line);
process.exit(noState.length > 0 ? 1 : 0);
