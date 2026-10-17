# Intent: v2-history

## Problem

V2 keeps every domain's results as a history (R9–R13, R19): entries that change only when the domain changes, a diff against the previous entry, and a 90-day retention. Today lens holds a moka cache and `snapshots.db`; neither knows a predecessor, a diff or a volatile path, and nothing of `netray-store` exists.

## Proposed outcome

SDD Phase 2, first of three features (operator, 2026-10-15): V2.1 `history.db` with the schema of §4.1 in a new crate `netray-store` via portable sqlx migrations; V2.2 identical-run detection by fingerprint (§4.2); V2.3 the diff of §4.3 as a pure function in `netray-model`, table-driven over every R12 class, with the profile/domain split of R13; V2.4 the hourly sweep of §4.5; V2.5 incomplete runs never stored, never a cache hit, never badged, diffed against the last entry.

## Affected users and systems

Library work plus whatever wiring the spec decides for lens. `snapshots.db` stays untouched until the cutover (S2, SC15). Admission (V2.6, `v2-admission`) and `/r/<id>` plus the privacy page (V2.7–V2.8, `v2-entries`) follow as their own features.

## Constraints

Ships with 0.27.0 together with `v2-admission` and `v2-entries`; no release of its own. Types portable (no SQLite-only types or functions). Recording a run is one transaction (P41). Every released profile stays loadable (R6) for the R13 split.

## Open questions

- Does lens write to `history.db` in this feature, or is the store a library nothing calls until `v2-admission` (history-as-cache and run join, §4.4)?
- If lens writes: does the history replace the moka cache here, or with `v2-admission`?
- "Never badged" (V2.5): wired to the badge route here, or a store-level guarantee the badge inherits later?
- Where `history.db` is configured and whether a missing or unwritable path is a startup reject.
