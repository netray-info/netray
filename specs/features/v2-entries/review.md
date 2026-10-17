# Review: v2-entries

## a721964..e9eb3fb

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

No wrong result in a721964..e9eb3fb. Traced: `/r/{id}` precedence (malformed or rejected path 404 `NOT_FOUND_HTML`; entry; V1 lookup with its 500; 410 gone; a failed history lookup 500 with a warn, V1 never called); `/r/{id}/og.png` (410 empty for swept, never-issued and live V1 IDs; 304 on a matching ETag without rendering; render failure passed through as lens's response); the mount in `lens::app` (entries router with history, else snapshot router, else none; privacy unconditional; both before the layers beside the SPA fallback); byte-identity of `snapshot_answer`/`v1_lookup` with the old `snapshot_handler`, of `svg_for_grade` through `card_svg`, of the moved `format_utc_timestamp`; the entry-to-view mapping (fail then warn in catalogue order, unknown IDs, counts, every change class word and tone, summary and evidence texts, escaping); the repo guards. Checked and not reported: exact `If-None-Match` comparison (as the V1 OG handler), relative `og:image`/`og:url` (as the spec and the V1 page), the entry card served with `[og_cards] enabled = false` (the spec makes it unconditional), pre-rebase `ADLC-Baseline` SHAs (informational). Ran `cargo test --workspace --exclude netray-ip`, the `tests/repo` scripts that start no binary, `cargo metadata --locked --offline`: all pass.

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=21 partial=0 missed=0 contradicted=0

All 21 requirements of Phases 1–4 checked against the code at e9eb3fb; the entries, mount, OG-card, privacy and `snapshot_routes` tests and `test_web_crate.sh`, `test_entries_sdd.sh`, `test_spectrum_deps.sh` green. The SDD §4.6/§4.7 and Phase 2 table edits sit in the commits that build requirements 10, 15 and 19. No behaviour the spec does not name.

### Refuted

None (no BLOCKER or MAJOR to refute).

### Calibration

No finding was answered by a refuter.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | the entries code uses `netray_model::{Grade, Status}`, declares no status enum; no codename in the templates |
| P10 | convergence | `status_tone` sends unmeasured, not tested and none to untested, never pass; `Grade::Incomplete` untested |
| P12 | absence | no outbound call or module run added |
| P13 | divergence (weak) | `/r/` and `/privacy` are attached to no per-consumer limiter; the V1 `snapshot_router` they replace had none either |
| P18 | convergence | no `reqwest::Client`, no config struct added |
| P26 | absence | no check ID or emitter added |
| P35 | convergence | the Router allow-list arrives with `router_offenders` and its self-test; SDD §4.6/§4.7 held by `test_entries_sdd.sh` |
| P36 | absence | no generated artefact changed |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/0/0, spec reader 0/0/0 (REQUIREMENTS met=21 partial=0 missed=0 contradicted=0). After refutation: the same. verified 0, held 0. Roll call: 9 answered, 4 convergence, 1 divergence, 4 absence.
