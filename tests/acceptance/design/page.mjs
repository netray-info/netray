// Page side of the design gate: loads one board's page at its width and theme,
// screenshots it and reads the computed values of the board's elements.
import { mkdirSync } from 'node:fs';
import { join } from 'node:path';
import { boardName, boardSlug } from './manifest.mjs';

export async function measurePage(browser, board, baseUrls, outDir) {
  const base = baseUrls[board.host];
  if (base === undefined) throw new Error(`no base URL for host "${board.host}"`);
  // The loader holds `{target}` in the path and `target` together (both or neither).
  const path = board.target ? board.path.replaceAll('{target}', board.target) : board.path;
  const url = base + path;

  mkdirSync(outDir, { recursive: true });
  const context = await browser.newContext({
    viewport: { width: board.width, height: 900 },
    deviceScaleFactor: 1,
    colorScheme: board.theme,
  });
  try {
    await context.addInitScript((t) => {
      try {
        localStorage.setItem('theme', t);
      } catch {}
    }, board.theme);
    const page = await context.newPage();
    const response = await page.goto(url, { waitUntil: 'load' });
    const status = response?.status() ?? null;
    // A board whose page fills in after load (an inspector's live run) names the selector that
    // marks the finished state; a page that never reaches it is measured as it stands.
    let waited = null;
    if (board.wait) {
      waited = await page
        .waitForSelector(board.wait, { timeout: 60000 })
        .then(() => true)
        .catch(() => false);
    }
    // A page state behind a control (an open menu) is opened before it is measured.
    if (board.click?.page) await page.locator(board.click.page).first().click();
    await page.evaluate(() =>
      document.fonts.ready.then(
        () => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))),
      ),
    );

    const { colorScheme, theme, state } = await page.evaluate(() => ({
      colorScheme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light',
      theme: document.documentElement.getAttribute('data-theme'),
      state: document.querySelector('#result')?.getAttribute('data-state') ?? null,
    }));

    const values = {};
    for (const el of board.elements) {
      const loc = page.locator(el.page);
      if ((await loc.count()) === 0) {
        values[el.name] = null;
        continue;
      }
      values[el.name] = await loc.first().evaluate((n, props) => {
        const cs = getComputedStyle(n);
        return Object.fromEntries(props.map((p) => [p, cs.getPropertyValue(p)]));
      }, el.properties);
    }

    const screenshot = join(outDir, `${boardSlug(board)}-page.png`);
    await page.screenshot({ path: screenshot, fullPage: true });
    return { board: boardName(board), url, status, screenshot, colorScheme, theme, state, waited, values };
  } finally {
    await context.close();
  }
}
