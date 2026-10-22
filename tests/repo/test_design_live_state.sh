#!/usr/bin/env bash
# v2-ui-polish R1 (C1/C3-C7): a manifest element may carry `"shell": true` (boolean only); a live
# board (one with `wait`) reports ONE `no state` row when its wait selector was not reached or when
# its page holds no element without `shell`; a board without `wait` is measured as before.
set -u
source "$(dirname "$0")/lib/netray.sh"

loader="$REPO_ROOT/tests/acceptance/design/manifest.mjs"
mod="$REPO_ROOT/tests/acceptance/design/compare.mjs"
tmp=$(mktemp -d "${TMPDIR:-/tmp}/design_live_state.XXXXXX")
trap 'rm -rf "$tmp"; _netray_cleanup' EXIT

failures=0
fail() { echo "FAIL: $*" >&2; failures=$((failures + 1)); }

[ -f "$mod" ] || { echo "FAIL: compare module missing" >&2; exit 1; }

# Manifest table: shell accepted as boolean, refused otherwise (exit 2, message names the element).
cat >"$tmp/base.json" <<'JSON'
{
  "export": "export/canvas.html",
  "boards": [
    {
      "title": "Live board",
      "host": "lens",
      "path": "/{target}",
      "width": 1280,
      "theme": "light",
      "target": "shop.example.com",
      "wait": "form[data-stream-state=done]",
      "elements": [
        {"name": "header bar", "board": "header", "page": "header", "class": "spacing", "properties": ["padding-top"]},
        {"name": "card gap", "board": ".card", "page": ".card", "class": "spacing", "properties": ["padding-top"]}
      ]
    }
  ]
}
JSON

mutate() {
    node -e '
        const fs = require("fs");
        const m = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
        eval(process.argv[2]);
        fs.writeFileSync(process.argv[3], JSON.stringify(m));
    ' "$tmp/base.json" "$1" "$tmp/m.json"
}

table=(
    "C3 shell true|m.boards[0].elements[0].shell = true|0|"
    "C3 shell false|m.boards[0].elements[0].shell = false|0|"
    "C3 shell string|m.boards[0].elements[0].shell = \"yes\"|2|header bar;;shell"
    "C3 shell number|m.boards[0].elements[1].shell = 1|2|card gap;;shell"
)
for row in "${table[@]}"; do
    IFS='|' read -r name mutation want_rc substrings <<<"$row"
    mutate "$mutation" || { fail "$name: could not build manifest"; continue; }
    err=$(node "$loader" "$tmp/m.json" 2>&1 >/dev/null </dev/null)
    rc=$?
    [ "$rc" -eq "$want_rc" ] || fail "$name: exit $rc, want $want_rc (stderr: $err)"
    if [ -n "$substrings" ]; then
        while IFS= read -r want; do
            case "$err" in
                *"$want"*) ;;
                *) fail "$name: stderr lacks '$want' (stderr: $err)" ;;
            esac
        done < <(printf '%s\n' "${substrings//;;/$'\n'}")
    fi
done

COMPARE_URL="file://$mod" node --input-type=module <<'JS'
import test from 'node:test';
import assert from 'node:assert/strict';

const m = await import(process.env.COMPARE_URL);

const el = (name, shell) => ({ name, board: '.a', page: '.a', class: 'spacing', properties: ['gap'], ...(shell ? { shell: true } : {}) });
const live = {
  title: 'Live board', host: 'lens', path: '/{target}', width: 1280, theme: 'light',
  wait: 'form[data-stream-state=done]',
  elements: [el('shell bar', true), el('card', false)],
};
const plain = { ...live, wait: undefined };
const here = { gap: '4px' };

const rows = [
  ['C4 only shell elements on the page', live, { 'shell bar': here, card: null }, true, 'no board element on the page'],
  ['C5 wait selector not reached', live, { 'shell bar': here, card: here }, false, 'wait form[data-stream-state=done] not reached'],
  ['C5 not reached wins over empty page', live, { 'shell bar': null, card: null }, false, 'wait form[data-stream-state=done] not reached'],
  ['C6 one non-shell element on the page', live, { 'shell bar': here, card: here }, true, null],
  ['C6 non-shell only, shell missing', live, { 'shell bar': null, card: here }, true, null],
  ['C7 board without wait, nothing on the page', plain, { 'shell bar': null, card: null }, null, null],
];
for (const [name, board, page, waited, want] of rows) {
  test(name, () => {
    assert.equal(typeof m.liveState, 'function', 'liveState export missing');
    assert.equal(m.liveState(board, page, waited), want);
  });
}

test('C4 criterion renders one no state row', () => {
  const reason = m.liveState(live, { 'shell bar': here, card: null }, true);
  const d = m.noStateDeviation(live, reason);
  assert.equal(m.criterion(d), 'Live board 1280/light: no state (no board element on the page)');
});

test('C5 criterion renders the selector', () => {
  const reason = m.liveState(live, { 'shell bar': here, card: here }, false);
  const d = m.noStateDeviation(live, reason);
  assert.equal(m.criterion(d), 'Live board 1280/light: no state (wait form[data-stream-state=done] not reached)');
});

test('C7 board without wait stays missing on page', () => {
  const devs = m.deviations(plain, { 'shell bar': here, card: here }, { 'shell bar': null, card: null });
  assert.deepEqual(devs.map((d) => d.kind), ['missing on page', 'missing on page']);
});

test('C6 live board with a non-shell element reports measured rows', () => {
  const devs = m.deviations(live, { 'shell bar': { gap: '4px' }, card: { gap: '4px' } }, { 'shell bar': { gap: '4px' }, card: { gap: '40px' } });
  assert.equal(devs.length, 1);
  assert.equal(devs[0].kind, 'value');
});
JS
rc=$?
[ "$rc" -eq 0 ] || fail "live state unit tests exited $rc"

[ "$failures" -eq 0 ] || { echo "$failures failure(s)" >&2; exit 1; }
echo "ok: design live state"
