# Intent: v2-entries

## Problem

`v2-history` records every complete lens run in `history.db`, but nothing reads an entry back: `/r/<id>` serves only V1 snapshots (`crates/lens/src/routes.rs:1567-1621`), an entry has no page and no OG card of its own, and a swept entry's ID answers what any unknown ID answers today (404, "expired or unknown"). R15 promises that an entry is reachable by its ID for its whole life and answers `410` without naming the domain afterwards; R16 promises a privacy page that says what the history keeps, and none exists (no `privacy` file in the monorepo; the policy stub is `specs/netray/sdd/privacy-policy.md` in netray-ops, private).

## Proposed outcome

SDD Phase 2, third of three features (operator, 2026-10-15): V2.7 `/r/<id>` renders a stored entry as a minimal server-side page on the Phase 4a components and serves the entry's own OG card at `/r/<id>/og.png`; a well-formed ID that is no entry and no V1 snapshot answers `410` with the idle input and no domain (§4.6). V1 snapshot IDs keep rendering their V1 page until V5.6 removes `snapshots.db` at the cutover. V2.8 `/privacy` carries §4.7's text, and the private `privacy-policy.md` replaces its "nothing per request; aggregates only" line for the history.

## Affected users and systems

`netray-web` (a minimal `base.html`, three page templates, the entry mapping and the `/r/` and `/privacy` routers, SDD §3.1, operator X1), lens (mounts those routers ahead of its SPA fallback, hands in its V1 snapshot lookup and V1 card renderer), `specs/sdd/v2.md` (§4.6, Phase 2 table), and netray-ops `specs/netray/sdd/privacy-policy.md` (a separate commit in that repository). No wire, golden, cache, badge or snapshot behaviour changes.

## Constraints

Builds on the unmerged `v2-history` (`netray-store`, `HistoryStore::get`, `AppState::history_store`); `v2-admission` may land first and touches neither `get` nor `/r/`. No release of its own: the program builds to a local `0.30.0`, the only release (operator, 2026-10-17). `tests/repo/test_web_crate.sh` lifts its no-Router and no-axum guards under `ADLC-Test-Change` if still in place; the binary still does not depend on `netray-web` directly. Runs parallel to `v2-pages`; the second to land merges `base.html`, the router allow-list and the single `/privacy` route. The full result page, evidence views, API v1 and admission are other features.

## Open questions

None. Resolved 2026-10-17 (operator): the privacy page carries §4.7's text only, the whole policy is a separate feature after this program; the overnight run commits the netray-ops `privacy-policy.md` change and the operator pushes.
