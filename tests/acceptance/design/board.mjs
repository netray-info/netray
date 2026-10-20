// Board side of the design gate: opens the design export, grows one board's iframe
// to its content, screenshots it and reads the computed values of the board's elements.
import { mkdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { boardSlug } from './manifest.mjs';

export async function openExport(browser, exportPath) {
  const context = await browser.newContext({
    viewport: { width: 1600, height: 3200 },
    deviceScaleFactor: 1,
  });
  const page = await context.newPage();
  await page.goto(pathToFileURL(exportPath).href, { waitUntil: 'load' });
  // The export serves its boards as sandboxed blob: iframes the parent cannot read, so each
  // board's readiness is awaited inside its own frame.
  for (const handle of await page.$$('iframe')) {
    const frame = await handle.contentFrame();
    if (frame === null) throw new Error(`an iframe of the export has no frame (${await handle.getAttribute('title')})`);
    await frame.waitForFunction(() => (document.body?.children.length ?? 0) > 0, null, { timeout: 30000 });
    await frame.evaluate(() => document.fonts.ready);
  }
  await page.evaluate(() => document.fonts.ready);
  await page.waitForTimeout(500);
  return page;
}

export async function measureBoard(exportPage, board, outDir) {
  let handle = null;
  for (const candidate of await exportPage.$$('iframe')) {
    if ((await candidate.getAttribute('title')) === board.title) {
      handle = candidate;
      break;
    }
  }
  if (handle === null) throw new Error(`board "${board.title}" not found in the export`);

  const frame = await handle.contentFrame();
  const height = await frame.evaluate(() => document.documentElement.scrollHeight);
  await handle.evaluate((f, h) => {
    f.style.border = '0';
    f.style.height =`${h}px`;
    f.setAttribute('height', String(h));
  }, height);
  await exportPage.evaluate(
    () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
  );

  mkdirSync(outDir, { recursive: true });
  const screenshot = join(outDir, `${boardSlug(board)}-board.png`);
  await handle.screenshot({ path: screenshot });

  const values = {};
  for (const el of board.elements) {
    const loc = frame.locator(el.board);
    if ((await loc.count()) === 0) {
      values[el.name] = null;
      continue;
    }
    values[el.name] = await loc.first().evaluate((n, props) => {
      const cs = getComputedStyle(n);
      return Object.fromEntries(props.map((p) => [p, cs.getPropertyValue(p)]));
    }, el.properties);
  }
  return { screenshot, values };
}

export async function sideBySide(browser, boardPng, pagePng, outPath) {
  const src = (p) => `data:image/png;base64,${readFileSync(p).toString('base64')}`;
  const context = await browser.newContext({
    viewport: { width: 800, height: 600 },
    deviceScaleFactor: 1,
  });
  try {
    const page = await context.newPage();
    await page.setContent(
      `<!doctype html><html><body style="margin:0;display:flex;align-items:flex-start;gap:16px;background:#888">` +
        `<img style="display:block" src="${src(boardPng)}">` +
        `<img style="display:block" src="${src(pagePng)}"></body></html>`,
    );
    await page.waitForFunction(() => [...document.images].every((i) => i.complete));
    await page.screenshot({ path: outPath, fullPage: true });
    return outPath;
  } finally {
    await context.close();
  }
}
