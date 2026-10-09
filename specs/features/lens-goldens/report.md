# Report: lens result goldens and backend results tables

## Phase 1 — lens result goldens

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: stubs serve the committed backend goldens; lens with `lens.production.toml` settings runs `POST /api/check` in-process; projection compared with `tests/fixtures/contracts/lens-<fixture>.json`, rewritten under `UPDATE_GOLDEN=1` | already_implemented | crates/lens/tests/lens_golden.rs |
| C2 | R2: projection = summary fields + per-section status, grade, checks as name+verdict; no messages, durations, cache flag, snapshot id; sorted maps, byte-stable | already_implemented | crates/lens/tests/lens_golden.rs |
| C3 | R3: fixtures `healthy`, `no-mx`, `mx-cname`, each one table entry | already_implemented | crates/lens/tests/lens_golden.rs |
| C4 | R8: no production change beyond a behaviour-neutral exposure | already_implemented | — |
| C5 | `healthy` → projection equals `lens-healthy.json` | already_implemented | crates/lens/tests/lens_golden.rs |
| C6 | `no-mx` → equals `lens-no-mx.json` (amended: today the mail buckets are `skip`, `not_applicable` is empty) | already_implemented | crates/lens/tests/lens_golden.rs |
| C7 | `mx-cname` → equals `lens-mx-cname.json` | already_implemented | crates/lens/tests/lens_golden.rs |
| C8 | two runs write byte-identical files | already_implemented | crates/lens/tests/lens_golden.rs |
| C9 | `UPDATE_GOLDEN` unset and a differing golden → fails and prints the differing fields | already_implemented | crates/lens/tests/lens_golden.rs |

Every test is a pinning test and passed at its baseline commit (`ef55064`, baseline `ca9716a`); C4 holds because the phase changed no production file. Goldens: `lens-healthy.json` C (71.2), `lens-no-mx.json` C (65.0), `lens-mx-cname.json` B (76.6); no section errors in any fixture; two runs are byte-identical.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| — | 0 | — | 0 | 0 |

No coder ran: nothing was red. The test writer took 49678 tokens, 191 s.

### Review

- AMENDMENT | spec Phase 1 scenario 2 said `not_applicable` names the no-MX mail buckets; today lens marks `email_infrastructure`, `email_transport` and `email_brand_policy` `skip` and leaves `not_applicable` empty. The golden pins today's state; the test asserts a `skip` bucket instead | affected_phase: 1 | repaired_in_phase: yes (scenario reworded). Whether no-MX should become N/A is R4.3's to decide, which moves this row with `ADLC-Test-Change`.
- AMENDMENT | requirement 2 recorded `not_applicable` as a map; the projection keeps the section names only, the reason being prose like messages | affected_phase: 1 | repaired_in_phase: yes
- DEFERRED | `lens-mx-cname.json`: the email section event carries grade F (beacon's own grade, passed through) while `summary.section_grades.email` is B (lens's bucket score). Two grades for one section is today's behaviour, now pinned; the V2 status-word mapping (V1.1) should name which one is the section grade.
- DEFERRED | `cargo clippy -p lens --tests -- -D warnings` fails on three existing errors in `crates/lens/tests/og_routes.rs`; the gate runs clippy without `--tests`, so it is green. Not touched here.

### Behavioural verification

skipped: no entry point changes; `POST /api/check` is driven in-process by the test itself.
