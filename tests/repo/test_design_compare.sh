#!/usr/bin/env bash
# v2-design-gate R5/R6/R7: tests/acceptance/design/compare.mjs compares board values with page
# values by class tolerance, matches accepted deviations, and renders report.md rows and a summary.
set -u
source "$(dirname "$0")/lib/netray.sh"

trap '_netray_cleanup' EXIT

mod="$REPO_ROOT/tests/acceptance/design/compare.mjs"
[ -f "$mod" ] || { echo "FAIL: compare module missing: tests/acceptance/design/compare.mjs" >&2; exit 1; }

COMPARE_URL="file://$mod" node --input-type=module <<'JS'
import test from 'node:test';
import assert from 'node:assert/strict';

const m = await import(process.env.COMPARE_URL);

const board = { title: 'Result page, light', host: 'lens', path: '/{target}', width: 1280, theme: 'light', elements: [] };
const el = (name, cls, props) => ({ name, board: '.a', page: '.a', class: cls, properties: props });

// Compare one element, one property, one class.
const one = (cls, prop, b, p) => {
  const bd = { ...board, elements: [el('e', cls, [prop])] };
  return m.deviations(bd, { e: { [prop]: b } }, { e: { [prop]: p } });
};

const rows = [
  ['typography size within tolerance', 'typography', 'font-size', '40px', '41px', 0],
  ['typography size beyond tolerance', 'typography', 'font-size', '40px', '44px', 1],
  ['typography weight differs', 'typography', 'font-weight', '650', '700', 1],
  ['typography family first, case, quotes', 'typography', 'font-family', '"Inter", sans-serif', 'inter, system-ui', 0],
  ['spacing gap within tolerance', 'spacing', 'gap', '36px', '40px', 0],
  ['spacing gap beyond tolerance', 'spacing', 'gap', '36px', '41px', 1],
  ['spacing multi-value within', 'spacing', 'padding', '0px 18px', '0px 20px', 0],
  ['spacing multi-value beyond', 'spacing', 'padding', '0px 18px', '0px 23px', 1],
  ['colour rgb vs hex', 'colour', 'color', 'rgb(255, 255, 255)', '#ffffff', 0],
  ['spacing shorthand of three against two values', 'spacing', 'padding', '0px 18px', '0px 18px 2px', 0],
  ['spacing gap of one against two values', 'spacing', 'gap', '36px', '36px 40px', 0],
  ['spacing shorthand beyond after expansion', 'spacing', 'padding', '0px 18px', '0px 18px 6px', 1],
  ['letter-spacing normal reads as zero', 'typography', 'letter-spacing', '-0.5px', 'normal', 0],
  ['letter-spacing normal beyond tolerance', 'typography', 'letter-spacing', '-1.5px', 'normal', 1],
  ['unreadable property on both sides', 'spacing', 'margin-botom', '', '', 1],
  ['unreadable colour on both sides', 'colour', 'colr', '', '', 1],
];
for (const [name, cls, prop, b, p, n] of rows) {
  test(name, () => assert.equal(one(cls, prop, b, p).length, n));
}

test('deviation criterion names both values', () => {
  const [d] = one('typography', 'font-size', '40px', '44px');
  assert.equal(d.kind, 'value');
  const c = m.criterion(d);
  assert.ok(c.includes('40px') && c.includes('44px'), c);
  assert.equal(c, 'Result page, light 1280/light: e font-size board 40px page 44px (typography)');
});

test('normalizeColour', () => {
  assert.equal(m.normalizeColour('rgb(255, 255, 255)'), '#ffffffff');
  assert.equal(m.normalizeColour('rgba(0, 0, 0, 0.5)'), '#00000080');
});

test('page element null is missing on page', () => {
  const bd = { ...board, elements: [el('e', 'spacing', ['gap'])] };
  const devs = m.deviations(bd, { e: { gap: '4px' } }, { e: null });
  assert.equal(devs.length, 1);
  assert.equal(devs[0].kind, 'missing on page');
  assert.ok(m.criterion(devs[0]).includes('element missing on page'));
});

const dev = () => one('typography', 'font-size', '40px', '44px')[0];

test('accepted deviation moves out of open', () => {
  const acc = [{ board: board.title, element: 'e', property: 'font-size', reason: 'export rounds' }];
  const r = m.applyAccepted([dev()], acc);
  assert.equal(r.open.length, 0);
  assert.equal(r.accepted.length, 1);
  assert.equal(r.accepted[0].reason, 'export rounds');
  assert.equal(r.stale.length, 0);
  assert.equal(m.summary({ open: r.open, stale: r.stale, titles: [board.title] }).status, 0);
});

test('stale accepted entry fails the summary', () => {
  const acc = [{ board: board.title, element: 'gone', property: 'gap', reason: 'old' }];
  const r = m.applyAccepted([], acc);
  assert.equal(r.stale.length, 1);
  const s = m.summary({ open: r.open, stale: r.stale, titles: [board.title] });
  assert.equal(s.status, 1);
  assert.ok(s.lines.some((l) => l.includes('stale')), s.lines.join('\n'));
});

test('summary counts deviations per board', () => {
  const s = m.summary({ open: [dev()], stale: [], titles: [board.title] });
  assert.equal(s.status, 1);
  assert.ok(s.lines.some((l) => l.includes('Result page, light: 1 deviation')), s.lines.join('\n'));
});

test('accepted entry of a board without a state is not stale', () => {
  const acc = [{ board: board.title, element: 'e', property: 'font-size', reason: 'kept' }];
  const r = m.applyAccepted([m.noStateDeviation(board, 'HTTP 500')], acc);
  assert.equal(r.stale.length, 0);
  assert.equal(r.open.length, 1);
});

test('noStateDeviation criterion', () => {
  const d = m.noStateDeviation(board, 'no seed');
  assert.equal(d.kind, 'no state');
  assert.equal(m.criterion(d), 'Result page, light 1280/light: no state (no seed)');
});

test('report rows have four cells and no stray pipe', () => {
  const bd = { ...board, elements: [el('e', 'typography', ['font-family'])] };
  const devs = [
    ...m.deviations(bd, { e: { 'font-family': 'a|b' } }, { e: { 'font-family': 'c|d' } }),
    ...m.deviations(bd, { e: { 'font-family': 'x' } }, { e: null }),
  ];
  assert.ok(devs.length >= 2);
  const out = m.reportRows(devs);
  assert.equal(out.length, devs.length);
  out.forEach((row, i) => {
    assert.ok(row.startsWith(`| D${i + 1} |`) && row.endsWith('| open | design-check |'), row);
    const inner = row.trim().replace(/^\|/, '').replace(/\|$/, '');
    const cells = inner.split('|');
    assert.equal(cells.length, 4, row);
  });
});
JS
rc=$?
[ "$rc" -eq 0 ] || { echo "FAIL: compare unit tests exited $rc" >&2; exit 1; }
echo "ok: design compare"
