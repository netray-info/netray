# Report: v2 modules

## Phase 0 — Baseline

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: lens's full sync output (durations and IDs removed) committed per lens fixture and for beacon-timeout, beacon-partial, beacon-sending-no-dkim as `lens-full-*.json`, written by `UPDATE_GOLDEN=1`; a test compares | green | crates/lens/tests/lens_full_output.rs |
| C2 | each fixture plus the three beacon-only goldens: `UPDATE_GOLDEN=1` writes, without it the test compares and passes | green | crates/lens/tests/lens_full_output.rs |
| C3 | a one-word headline change makes the test fail naming the fixture and field | green | crates/lens/tests/lens_full_output.rs |

RED: `lens_full_output_matches_goldens` failed on the ten missing goldens; the comparator and stripper unit tests passed (they are the C3 tooling).

### Behavioural verification

`UPDATE_GOLDEN=1 cargo test -p lens --test lens_full_output` wrote ten `lens-full-*.json` (79 kB) from the 0.23.1 code; two further runs without it passed; no stub port or timestamp remains (`grep -E '127\.0\.0\.1:[0-9]+|20[0-9]{2}-..-..T'` empty). Sections carry checks with fix hints, headlines, detail URLs and the HTTP and IP extras.
