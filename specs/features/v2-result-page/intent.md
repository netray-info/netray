# Intent: v2-result-page

## Problem

`netray.info/<domain>` has no result page. On the apex the lens SPA answers every path
(`crates/lens/src/lib.rs:152`, `spa::handler`). It reads only `?d=` and starts a check on load
(`crates/lens/frontend/src/App.tsx:62-64`). It never shows what was stored, what changed since the
last run, or one prioritised list of what to fix. The history store (`netray-store`, built by
`v2-history`) records every complete run and its diff, but no page reads it. The SDD's result page
(`specs/sdd/v2.md` §5) is the user-facing half of V2, and SDD Phase 4 has not built it yet:
V4.5, V4.6 and V4.13 are open.

## Proposed outcome

`GET /<domain>` renders the latest stored entry server-side in the order of §5.7. When the entry
is stale and JavaScript is on, the page streams a fresh run section by section through the own
`stream` island (S25), with grade and diff arriving last. When admission refuses the run, the page
shows S26's line with the reason and a countdown. The `spectrum` island switches the trace,
replay, pin and run-stepper states that the server computed in advance. The history table at
`/<domain>/history` lists every entry. Open issues show each warn and fail once, grouped by owner,
severity or protocol, and the `issues` island remembers the choice in browser storage. `/` is the
idle page, and the `recent` island keeps recent domains in the browser only. At 390 px the header,
the diff and the issues need no sideways scroll and keep 44 px targets. Screenshots at 390 px and
on desktop prove all of this to the operator before the feature finishes.

## Affected users and systems

- Visitors of `netray.info`: the apex's `/` and `/<domain>` stop serving the lens SPA.
- `crates/web` (pages, fragments, islands, the OG plate), `crates/lens` (mounts the routes ahead
  of the SPA fallback, implements the run seam, draws the OG plate),
  `tests/acceptance` and the root `justfile` (seeded local stack, screenshots).
- Built after `v2-history`, `v2-admission`, `v2-entries`, `v2-api`, `v2-spectrum`, `v2-pages` and
  v2-inspectors' Phase 2.
  The Host switch and the SPA deletion stay with `v2-cutover`.

## Constraints

- Autonomous build towards a local `0.30.0` with no release in between (operator, 2026-10-17).
  Nothing deploys before the cutover, so replacing the SPA at `/` early hurts nobody.
- Strict CSP: no inline script, no `unsafe-eval` (S25, V4.9). Islands compute no layout and hold
  no verdict (§8.4). They share the bundle's 30 KB gzipped cap (V4.10).
- The design-system gate (`crates/web/src/gate.rs`) covers every new template, stylesheet and
  island. The components exist once (§8.3).
- Test data uses only `example.com`, its subdomains and `netray.info` (§9.1).
- The operator reviews this feature's UI from its own screenshots before `finish`, whenever they
  exist; the midday review on 2026-10-18 covers lane B and precedes them.

## Open questions

None. Resolved by the operator on 2026-10-17: evidence references on stored runs come from
`v2-api` (X2); evidence is linked into v2-inspectors' views; the copy is approved from this feature's
screenshots.
