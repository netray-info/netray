# Report: v2-unmeasured

## Phase 1 — Engine, lens, frontend

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: a weighted `unmeasured` check makes its section incomplete and the run letterless; N/A and not_tested unchanged | green | `crates/engine/tests/unmeasured_scoring.rs` |
| C2 | R2: lens carries `Unmeasured` to scoring and wire verdict `"unmeasured"` (SSE, snapshots, MCP); stored `"skip"` reads as before; `site/api/lens.html` lists the value | green | `crates/lens/tests/unmeasured_verdict.rs` |
| C3 | R3: frontend shows "could not check" for `unmeasured`; incomplete note appears for an unmeasured-incomplete section | green | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
| C4 | spf pass + dmarc unmeasured → section incomplete, grade incomplete | green | `crates/engine/tests/unmeasured_scoring.rs` |
| C5 | spf pass + dmarc not_applicable → complete, possible = spf weight, letter | already_implemented | `crates/engine/tests/unmeasured_scoring.rs` |
| C6 | spf pass + dmarc not_tested → complete | already_implemented | `crates/engine/tests/unmeasured_scoring.rs` |
| C7 | engine run with email.dnsbl unmeasured → SSE verdict "unmeasured", grade incomplete | green | `crates/lens/tests/unmeasured_verdict.rs` |
| C8 | same with email.dnsbl not_applicable → verdict "skip" | already_implemented | `crates/lens/tests/unmeasured_verdict.rs` |
| C9 | stored snapshot with "skip" loads unchanged | already_implemented | `crates/lens/tests/unmeasured_verdict.rs` |
| C10 | frontend: verdict unmeasured → "could not check" + incomplete note | green | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
| C11 | frontend: verdict skip → "–", no incomplete note on its account | already_implemented | `crates/lens/frontend/src/components/Unmeasured.test.tsx` |
### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| G1 engine scoring | 1 | sonnet | 23093 | 26 |
| G2 lens wire | 1 (the tls step of GROUP_CMD lacked `--features testing`; rerun by me green) | sonnet | 37566 | 67 |
| G3 frontend | 1 | sonnet | 25776 | 28 |

Test-side fixes by the orchestrator: the C2/C7 stub emitted an unweighted `email.email_authentication`, so the C8 row could never get a letter; it now emits `email.spf` (b61bed4).

### Reader (phase 1)

| Class | Where | Finding | Action |
|---|---|---|---|
| BLOCKER | `crates/lens/frontend/src/components/Summary.tsx:196` | Every incomplete grade gets the headline "Grade unavailable — all backends failed", even when every backend answered | fixed (C14) |
| BLOCKER | `crates/lens/src/routes.rs:840` | A section made incomplete by an unmeasured check is "pass" in its event and "error" in `summary.sections` | fixed: new section status "incomplete" (operator, C13) |
| BLOCKER | `crates/lens/frontend/src/components/SectionHeadline.tsx:27` | The card header draws an unmeasured check as "–" | fixed (C15) |
| AMENDMENT | `crates/lens/frontend/src/components/ValidationChips.tsx:14` | The chip row does not count unmeasured checks | repaired in phase 1 (C16) |
| AMENDMENT | `crates/engine/src/scoring.rs:130` | Incomplete outranks a measured hard fail | operator 2026-10-12: a measured hard fail gives F even on an incomplete run; repaired in phase 1 (C12) |
| AMENDMENT | `crates/lens/README.md:235` | The SCORING SYNC RULE (`crates/lens/CLAUDE.md:5`) wants the README Scoring section changed in the same commit as the engine rule; requirement 17 deferred it | repaired in phase 1 |
| AMENDMENT | `site/api/lens.html:182` | The page documents neither grade `incomplete`, `summary.complete`, nor an incomplete section status | repaired in phase 1 |
| NIT | `crates/lens/src/routes.rs:189`, `crates/lens/src/score.rs:62` | The `complete` docs list only errored and possible-0 sections | fixed |
| NIT | `site/api/lens.html:181` | `skip` also carries `not_tested` | fixed |
| NIT | `crates/lens/src/routes.rs:1433` | The snapshot "incomplete" branch is unreachable under v3, because incomplete runs are not snapshotted | kept: it labels an unprofiled section correctly |

Further criteria from the reading (operator decisions of 2026-10-12):

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C12 | A measured hard fail gives F even when the run is incomplete; without one, incomplete stays letterless | green | `crates/engine/tests/hard_fail_precedence.rs` |
| C13 | A section made incomplete by a weighted unmeasured check is "incomplete" in its event and in `summary.sections` | green | `crates/lens/tests/unmeasured_section_status.rs` |
| C14 | The summary for grade incomplete with answering backends does not say "all backends failed" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C15 | The section headline draws an unmeasured check with "?" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C16 | The chip row counts unmeasured checks ("could not check") | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
| C17 | A section value "incomplete" is not rendered as "err" | green | `crates/lens/frontend/src/components/UnmeasuredSummary.test.tsx` |
### Runs (reader round)

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| C12 engine precedence | 0 (orchestrator, `scoring.rs` grade order) | orchestrator | 0 | 0 |
| C13 lens section status + docs | 2 | sonnet | 46624 | 133 |
| C14–C17 frontend | 2 | sonnet | 31197 | 38 |

### Behavioural verification

skipped: no module emits `unmeasured` before Phase 2. The SSE route is covered in process by `crates/lens/tests/unmeasured_verdict.rs` and `unmeasured_section_status.rs`, which POST to `/api/check` and read the `email` and `summary` events.

### Deferred

- Badge and OG responses for an F on an incomplete run keep the long `Cache-Control` (3600 s): `is_unscored_grade` matches only `error` and `incomplete`. The server-side cache and snapshots refuse incomplete results.
- A grade `incomplete` with an empty `sections` map still shows "all backends failed". `sdd_lens-badges_p2_c4_summary-badge-btn.test.tsx` pins this, and the backend does not send that combination.
- `crates/lens/README.md` changed in the phase commit, not in the prose commit: the SCORING SYNC RULE (`crates/lens/CLAUDE.md:5`) requires it.
