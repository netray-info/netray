// Design check comparison: pure functions that compare board values with page values by
// element class, match accepted deviations, and render report rows and a summary.
// No dependencies; nothing here touches a browser, the network or the file system.

import { basename } from 'node:path';
import { boardName } from './manifest.mjs';

const hex2 = (n) => Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0');

export function normalizeColour(v) {
  if (typeof v !== 'string') return null;
  const s = v.trim().toLowerCase();
  if (s === 'transparent') return '#00000000';
  let m = /^#([0-9a-f]{3,4})$/.exec(s);
  if (m) {
    const [r, g, b, a = 'f'] = m[1];
    return `#${r}${r}${g}${g}${b}${b}${a}${a}`;
  }
  m = /^#([0-9a-f]{6})([0-9a-f]{2})?$/.exec(s);
  if (m) return `#${m[1]}${m[2] ?? 'ff'}`;
  m = /^rgba?\(\s*([^)]*)\)$/.exec(s);
  if (!m) return null;
  const parts = m[1].split(/\s*[,/]\s*|\s+/).filter((p) => p !== '');
  if (parts.length < 3 || parts.length > 4) return null;
  const chan = (p) => (p.endsWith('%') ? (parseFloat(p) * 255) / 100 : parseFloat(p));
  const rgb = parts.slice(0, 3).map(chan);
  let a = 1;
  if (parts.length === 4) a = parts[3].endsWith('%') ? parseFloat(parts[3]) / 100 : parseFloat(parts[3]);
  if (rgb.some(Number.isNaN) || Number.isNaN(a)) return null;
  return `#${rgb.map(hex2).join('')}${hex2(a * 255)}`;
}

const PX = /^(-?\d+(?:\.\d+)?)px$/;

// Box shorthands (padding, margin, gap, border-width) read back with 1 to 4 tokens depending on
// which sides are equal; expand both sides to four (top, right, bottom, left) before comparing.
function expandBox(tokens) {
  const [t, r = t, b = t, l = r] = tokens;
  return [t, r, b, l];
}

function pxWithin(b, p, tol) {
  let bs = String(b).trim().split(/\s+/);
  let ps = String(p).trim().split(/\s+/);
  if (bs.length !== ps.length && bs.length <= 4 && ps.length <= 4) {
    bs = expandBox(bs);
    ps = expandBox(ps);
  }
  if (bs.length !== ps.length) return false;
  for (let i = 0; i < bs.length; i++) {
    const bm = PX.exec(bs[i]);
    const pm = PX.exec(ps[i]);
    // A px token is compared within the tolerance, any other token (`normal`, `auto`) exactly.
    if (!bm || !pm) {
      if (bs[i] !== ps[i]) return false;
      continue;
    }
    // "At most" the tolerance: the epsilon absorbs float error (17.6 - 13.6 = 4.000000000000002).
    if (Math.abs(parseFloat(bm[1]) - parseFloat(pm[1])) > tol + 1e-6) return false;
  }
  return true;
}

const firstFamily = (v) =>
  String(v).split(',')[0].trim().replace(/^["']|["']$/g, '').trim().toLowerCase();

const zeroNormal = (v) => (String(v).trim() === 'normal' ? '0px' : v);

function within(cls, prop, b, p) {
  // An empty value is a property the browser cannot read (a misspelt name): never a match.
  if (b == null || p == null || String(b).trim() === '' || String(p).trim() === '') return false;
  if (cls === 'spacing') return pxWithin(b, p, 4);
  if (cls === 'typography') {
    if (prop === 'font-weight') return String(b).trim() === String(p).trim();
    if (prop === 'font-family') return firstFamily(b) === firstFamily(p);
    // Chromium reads letter-spacing 0 back as `normal`.
    if (prop === 'letter-spacing') return pxWithin(zeroNormal(b), zeroNormal(p), 1);
    return pxWithin(b, p, 1);
  }
  if (cls === 'colour') {
    const nb = normalizeColour(b);
    const np = normalizeColour(p);
    if (nb !== null && np !== null) return nb === np;
    return String(b) === String(p);
  }
  return String(b) === String(p);
}

const absent = (v) => v === null || v === undefined;

export function deviations(board, boardValues, pageValues) {
  const out = [];
  for (const element of board.elements) {
    const base = {
      board: boardName(board),
      width: board.width,
      theme: board.theme,
      element: element.name,
      class: element.class,
    };
    const bv = boardValues?.[element.name];
    const pv = pageValues?.[element.name];
    if (absent(bv)) {
      out.push({ ...base, property: null, boardValue: null, pageValue: null, kind: 'missing on board' });
      continue;
    }
    if (absent(pv)) {
      out.push({ ...base, property: null, boardValue: null, pageValue: null, kind: 'missing on page' });
      continue;
    }
    for (const property of element.properties) {
      if (!within(element.class, property, bv[property], pv[property])) {
        out.push({
          ...base,
          property,
          boardValue: bv[property] ?? null,
          pageValue: pv[property] ?? null,
          kind: 'value',
        });
      }
    }
  }
  return out;
}

export function noStateDeviation(board, reason) {
  return {
    board: boardName(board),
    width: board.width,
    theme: board.theme,
    element: null,
    property: null,
    boardValue: null,
    pageValue: null,
    class: null,
    kind: 'no state',
    reason,
  };
}

// A live board (one with `wait`) has no state when its wait selector was not reached or when its
// page holds only shell elements (`shell: true`, present on every page); null otherwise.
export function liveState(board, pageValues, waited) {
  if (!board.wait) return null;
  if (waited === false) return `wait ${board.wait} not reached`;
  const onPage = board.elements.some((el) => !el.shell && pageValues[el.name] != null);
  return onPage ? null : 'no board element on the page';
}

export function criterion(dev) {
  const head = `${dev.board} ${dev.width}/${dev.theme}:`;
  if (dev.kind === 'value') {
    return `${head} ${dev.element} ${dev.property} board ${dev.boardValue} page ${dev.pageValue} (${dev.class})`;
  }
  if (dev.kind === 'no state') return `${head} no state (${dev.reason})`;
  const side = dev.kind === 'missing on board' ? 'board' : 'page';
  return `${head} ${dev.element} element missing on ${side} (class 1)`;
}

export function applyAccepted(devs, accepted = []) {
  // A board without a state was not measured: its entries are neither matched nor stale.
  const unmeasured = new Set(devs.filter((d) => d.kind === 'no state').map((d) => d.board));
  const used = new Set();
  const open = [];
  const taken = [];
  for (const dev of devs) {
    const i =
      dev.kind === 'value'
        ? accepted.findIndex(
            (a) => a.board === dev.board && a.element === dev.element && a.property === dev.property,
          )
        : -1;
    if (i < 0) {
      open.push(dev);
    } else {
      used.add(i);
      taken.push({ deviation: dev, reason: accepted[i].reason });
    }
  }
  return {
    open,
    accepted: taken,
    stale: accepted.filter((a, i) => !used.has(i) && !unmeasured.has(a.board)),
  };
}

const cell = (s) => String(s).replaceAll('|', '/');

export function reportRows(open) {
  return open.map((d, i) => `| D${i + 1} | ${cell(criterion(d))} | open | design-check |`);
}

export function reportMarkdown({ open, accepted, stale, sideBySides = [] }) {
  const lines = ['# Design check', '', '## Deviations', ''];
  if (open.length === 0) {
    lines.push('None.');
  } else {
    lines.push('| id | criterion | status | test file |', '|---|---|---|---|', ...reportRows(open));
  }
  lines.push('', '## Accepted', '');
  if (accepted.length === 0) lines.push('None.');
  else for (const a of accepted) lines.push(`- ${criterion(a.deviation)} — ${a.reason}`);
  if (stale.length > 0) {
    lines.push('', '## Stale accepted entries', '');
    for (const s of stale) lines.push(`- ${s.board} / ${s.element} / ${s.property}`);
  }
  lines.push('', '## Side-by-side', '');
  if (sideBySides.length === 0) lines.push('None.');
  else for (const p of sideBySides) lines.push(`- ${basename(p)}`);
  return `${lines.join('\n')}\n`;
}

export function summary({ open, stale, titles }) {
  const lines = [];
  for (const title of titles) {
    const n = open.filter((d) => d.board === title).length;
    if (n > 0) lines.push(`${title}: ${n} deviation(s)`);
  }
  for (const s of stale) lines.push(`stale accepted: ${s.board} / ${s.element} / ${s.property}`);
  lines.push(
    `${open.length} open deviation(s), ${stale.length} stale accepted ${stale.length === 1 ? 'entry' : 'entries'}`,
  );
  return { status: open.length === 0 && stale.length === 0 ? 0 : 1, lines };
}
