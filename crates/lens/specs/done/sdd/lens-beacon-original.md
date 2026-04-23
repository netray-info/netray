# SDD: Integrate beacon as the Email Backend

Status: Draft
Created: 2026-04-23

## Overview

Add beacon (`email.netray.info`) as the fifth lens backend so `/api/check/{domain}` returns an email-security assessment alongside DNS, TLS, HTTP, and IP. This requires a new SSE-consuming backend, a per-bucket N/A mechanism (sending-identity always applies; receiving checks apply only when MX is present), and a cleanup of lens.dns to stop duplicating email-specific checks that beacon now owns more precisely.

## Context & Constraints

- **Stack**: Rust 2024 edition, Axum 0.8, reqwest with `stream`, SolidJS 1.9 frontend.
- **Existing patterns**: `Backend` trait in `src/backends/mod.rs`; wave-scheduled orchestration in `src/check.rs` (`WAVE1_SECTIONS`, `WAVE2_SECTIONS`); SSE draining in `src/backends/dns.rs:169` (prism); scoring engine in `src/scoring/engine.rs`; config layered via `netray_common::backend::BackendConfig`.
- **CLAUDE.md rules**: scoped commits; no mixed formatting; conventional commit prefixes; no PII / real domains (use `example.com`); **SCORING SYNC RULE** — any change to `src/scoring/` or `profiles/default.toml` must update README.md's Scoring section in the same commit.
- **Engineering**: KISS, YAGNI, DRY (rule of three), fail fast, partial-failure tolerance — one backend down never blocks the others.
- **Cross-repo**: user guide updates live in the netray.info meta repo under `../site/guide/`. Shipped in a separate commit after lens merges.

## Requirements

1. The system shall call beacon's SSE endpoint once per domain check, drain the stream to completion, and produce a single aggregated `email` section result.
2. The system shall run the email backend in wave 1 of the orchestration (concurrent with DNS, TLS, HTTP) because it has no cross-section data dependency.
3. The system shall aggregate beacon's 11 category verdicts plus cross-validation into exactly four buckets: `email_authentication`, `email_infrastructure`, `email_transport`, `email_brand_policy`, using worst-verdict aggregation per bucket.
4. The system shall score `email_authentication` for every domain regardless of MX presence.
5. The system shall mark `email_infrastructure`, `email_transport`, and `email_brand_policy` as not-applicable (per-bucket) when the domain has no MX records, with no penalty to the overall grade.
6. The scoring engine shall represent section status as an explicit `SectionStatus` enum with variants `Scored`, `Errored`, and `NotApplicable { reason }`. `Errored` and `NotApplicable` both cause the section to be excluded from the weighted overall score; `NotApplicable` additionally records its reason on `OverallScore.not_applicable` for UI signaling.
7. The system shall remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, and `mx` from lens.dns lint processing and from `profiles/default.toml` `dns.checks` / `dns.hard_fail`. `dnssec`, `dnskey_algorithm`, and `dnssec_rollover` remain in lens.dns (DNS-infrastructure perspective).
8. The system shall not introduce any hard-fail entries for the `email` section in v1.
9. The system shall accept an optional `dkim_selectors` query parameter on `GET/POST /api/check` and forward it verbatim to beacon. Absent selectors mean beacon falls back to its built-in provider map.
10. The system shall emit a new `email` SSE event (before `summary`) carrying bucket verdicts, beacon's reported grade, per-bucket messages, a `raw_headline`, a `detail_url` to `email.netray.info`, and per-bucket not-applicable reasons where relevant.
11. The system shall enforce a 15-second timeout on the email backend call (safely inside lens's 20-second hard deadline; beacon's own cap is 30 s).
12. The system shall render an email card in the frontend with four bucket rows. N/A buckets display a neutral "Not configured" badge and the message "No MX records — email receiving not configured". The overall grade card shall show a footnote when any email bucket is N/A so users understand why score weight redistributed.
13. The system shall document the sending-vs-receiving scoring model in `README.md` (Scoring section), in the email card's inline copy, and in the lens guide page under `../site/guide/`. Scoring changes and README updates land in the same commit.
14. The system shall lift the existing SSE event reader from `src/backends/dns.rs` into a shared helper at `src/backends/sse.rs`, parameterized on the stream's terminal event name. DNS and Email both call it.
15. The email backend shall return `SectionError::BackendError` when beacon is unreachable or returns a non-success HTTP status, `SectionError::Timeout` when the 15-second wrap times out, and `SectionStatus::NotApplicable { reason: "beacon timeout" }` when beacon itself reports `grade: "Skipped"` (its internal 30 s timeout).

## Architecture

```
+------------------------+            +-----------------+
| lens /api/check/:domain|            | beacon          |
| (Axum + SSE)           |            | (email.netray)  |
+-----------+------------+            +--------+--------+
            |                                  ^
            | wave 1 (concurrent)              |
            |   +------+ +------+ +------+    | SSE drain
            |   | dns  | | tls  | | http |    | (15 s cap)
            |   +------+ +------+ +------+    |
            |                                  |
            |   +---------------------+        |
            +-->| email (NEW)         |--------+
            |   |  - drain SSE        |
            |   |  - parse summary    |
            |   |  - 4 buckets        |
            |   |  - per-bucket N/A   |
            |   +---------------------+
            |
            | wave 2                          +-------+ +------+ +------+
            |   +------+                      | prism | |tlsight| |ifconfig|
            |   |  ip  |                      +-------+ +------+ +------+
            |   +------+
            v
   SSE events: dns -> tls -> http -> email -> ip -> summary -> done
```

Scoring engine state transition after change:

```
SectionInput { checks, status }
                         |
                         +---> Scored       -> score_section returns Some
                         +---> Errored      -> returns None, no reason
                         +---> NotApplicable{reason}
                                              -> returns None,
                                                 reason recorded on
                                                 OverallScore.not_applicable
```

## File & Module Structure

### New

| Path | Purpose |
|---|---|
| `src/backends/email.rs` | `EmailBackend` — SSE drain, summary parsing, bucket aggregation, N/A detection |
| `src/backends/sse.rs` | Shared SSE byte-stream → `Vec<Value>` reader; parameterized terminal event |
| `frontend/src/components/EmailCard.tsx` (or `.ts`) | Email section card with 4-bucket rendering and N/A states |
| `frontend/src/types/email.ts` | Frontend types mirroring the `email` event schema |
| `tests/email_fixtures/*.json` | Golden fixtures for beacon summary event: `mail_domain.json`, `no_mx.json`, `beacon_timeout.json`, `partial_fail.json` |

### Modified

| Path | Change |
|---|---|
| `src/backends/mod.rs` | Add `mod email;` `mod sse;`; extend `BackendExtra` with `Email { ... }` variant |
| `src/backends/dns.rs` | Remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` lint handling; call shared `sse::collect` instead of private `collect_sse`; update `build_headline` |
| `src/check.rs` | Add `"email"` to `WAVE1_SECTIONS` |
| `src/config.rs` | `BackendsConfig` gains `email: Option<BackendConfig>` |
| `src/state.rs` | Conditionally register `EmailBackend` when `email` is configured |
| `src/routes.rs` | Accept `?dkim_selectors=` query param; pipe through to `CheckInput` (or a request-scoped extension) |
| `src/check.rs` — `CheckInput` | Add `dkim_selectors: Option<String>` field |
| `src/scoring/engine.rs` | Replace `SectionInput.errored: bool` with `SectionInput.status: SectionStatus`; extend `OverallScore` with `not_applicable: HashMap<String, String>`; adjust `score_section` and `compute_score` |
| `src/scoring/profile.rs` | (No structural change — new sections load from TOML.) |
| `profiles/default.toml` | Drop `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` from `sections.dns.checks`; clear `sections.dns.hard_fail`; lower `sections.dns.weight` from 30 to 20; add `sections.email` with weight 15 and four bucket weights; drop `sections.tls.weight` to 35 to keep total at 100 |
| `lens.example.toml` | Add `[backends.email]` block with `url = "http://beacon:8085"` placeholder and commented notes |
| `README.md` | Update Scoring section (weights, hard-fails, sending-vs-receiving split); mention email backend + DKIM selector query param; update SSE events table; update `/api/meta` example if relevant |
| `frontend/src/App.tsx` (or equivalent) | Wire `email` SSE event to state; render `EmailCard`; add grade-card N/A footnote |
| `frontend/src/types/api.ts` (or equivalent) | Add `email` event type |
| `CHANGELOG.md` | Entry under `Unreleased`: added email backend via beacon; migrated email-specific checks out of DNS section; introduced `SectionStatus::NotApplicable` |

### Cross-repo (separate commit, netray.info meta repo)

| Path | Change |
|---|---|
| `../site/guide/lens.html` | New/extended section explaining the sending-vs-receiving scoring model |
| `../site/guide/concepts.html` (if exists) | Add N/A section concept; otherwise inline in lens guide |

## Data Models

### Engine changes

```rust
// src/scoring/engine.rs

pub enum SectionStatus {
    Scored,
    Errored,
    NotApplicable { reason: String },
}

pub struct SectionInput {
    pub checks: Vec<CheckResult>,
    pub status: SectionStatus,
}

pub struct OverallScore {
    pub sections: HashMap<String, SectionScore>,
    pub overall_percentage: f64,
    pub grade: String,
    pub hard_fail_triggered: bool,
    pub hard_fail_checks: Vec<String>,
    pub not_applicable: HashMap<String, String>, // section -> reason
}
```

`score_section` returns `None` for both `Errored` and `NotApplicable`. `compute_score` inspects the status variant and, for `NotApplicable`, records the reason in `OverallScore.not_applicable`.

### Email backend extras

```rust
// src/backends/mod.rs (addition to BackendExtra)

Email {
    raw_headline: String,       // e.g. "Auth ✓  Infra -  Transport -  Brand -"
    detail_url: String,         // https://email.netray.info/?domain=...
    grade: Option<String>,      // beacon's own grade: A/B/C/D/F/Skipped
    bucket_na: HashMap<String, String>, // bucket_name -> reason (for UI "Not configured" states)
}
```

### SSE event payload (lens output)

```json
{
  "event": "email",
  "data": {
    "status": "ok",
    "grade": "A",
    "buckets": {
      "email_authentication": { "verdict": "pass", "messages": [] },
      "email_infrastructure": { "verdict": "skip", "messages": ["No MX records — email receiving not configured"], "not_applicable": true },
      "email_transport":      { "verdict": "skip", "messages": ["No MX records — email receiving not configured"], "not_applicable": true },
      "email_brand_policy":   { "verdict": "skip", "messages": ["No MX records — email receiving not configured"], "not_applicable": true }
    },
    "headline": "Auth ✓  Infra -  Transport -  Brand -",
    "detail_url": "https://email.netray.info/?domain=example.com"
  }
}
```

## API Contracts

### Request

```
GET  /api/check/{domain}?dkim_selectors=sel1,sel2
POST /api/check
     { "domain": "example.com", "dkim_selectors": "sel1,sel2" }
```

- `dkim_selectors`: optional comma-separated list forwarded verbatim to beacon. Each selector validated by lens for label shape (letters/digits/hyphens, ≤63 chars, ≤ total 253).
- Absent → beacon uses its built-in provider map.

### Response — sync mode addition

```json
{
  "email": {
    "status": "ok",
    "grade": "A",
    "buckets": { ... },
    "headline": "...",
    "detail_url": "..."
  }
}
```

When email is `NotApplicable` (beacon timeout): `"status": "not_applicable", "reason": "..."`.
When email is errored: `"status": "error", "error": "..."`.

### Summary event — additions

```json
{
  "summary": {
    "overall": "A", "grade": "A", "score": 87.0,
    "hard_fail": false, "hard_fail_reason": null,
    "sections": { "dns": "B", "tls": "A", "http": "A", "email": "A", "ip": "A+" },
    "not_applicable": { "email": "beacon timeout" }   // present only when sections are N/A
  }
}
```

## Configuration

```toml
[backends.email]
url = "http://beacon:8085"
# public_url = "https://email.netray.info"   # optional, used for detail_url
# timeout_secs = 15
```

Environment variables (`LENS_` prefix, `__` separator):

| Variable | Type | Default | Required |
|---|---|---|---|
| `LENS_BACKENDS__EMAIL__URL` | URL | none | No — absent → email backend not registered, behaves like current HTTP-off mode |
| `LENS_BACKENDS__EMAIL__PUBLIC_URL` | URL | `url` value | No |
| `LENS_BACKENDS__EMAIL__TIMEOUT_SECS` | u64 | 15 | No |

Profile weight changes (`profiles/default.toml`):

```toml
[sections.tls]    weight = 35   # was 40
[sections.dns]    weight = 20   # was 30 (loses 6 checks to email)
[sections.http]   weight = 20   # unchanged
[sections.email]  weight = 15   # new
[sections.ip]     weight = 10   # unchanged
# Total: 100

[sections.email]
weight = 15
hard_fail = []

[sections.email.checks]
email_authentication = 10   # always scored
email_infrastructure = 5    # N/A when no MX
email_transport      = 5    # N/A when no MX
email_brand_policy   = 2    # N/A when no MX

[sections.dns]
weight = 20
hard_fail = []              # was ["spf", "dmarc"]

[sections.dns.checks]       # removed: spf, dmarc, mta_sts, tlsrpt, bimi, mx
dnssec            = 5
caa               = 5
ns                = 3
ns_lame           = 5
ns_delegation     = 3
cname_apex        = 5
https_svcb        = 2
ttl               = 2
dnskey_algorithm  = 3
dnssec_rollover   = 2
```

## Error Handling

| Failure | Trigger | Behaviour | User-visible |
|---|---|---|---|
| beacon unreachable | Connection refused / DNS failure | `SectionError::BackendError` | Email section marked `error` in summary; overall score computed without email |
| beacon HTTP non-2xx | Non-success status | `SectionError::BackendError` | Same as above; stderr log with status code |
| lens 15 s timeout wrap fires | reqwest send or SSE read exceeds 15 s | `SectionError::Timeout` | Email section `status: "error"` with timeout reason |
| beacon reports `grade: "Skipped"` | Beacon's own 30 s timeout (rare — lens timeout fires first) | `SectionStatus::NotApplicable { reason: "beacon timeout" }` | Email card shows neutral N/A; footnote explains |
| Domain has no MX | Beacon summary's `mx` category is `Fail` | Three buckets return `CheckVerdict::Skip` with reason "No MX records — email receiving not configured"; `email_authentication` scores normally | Email card shows 3 N/A buckets + 1 scored bucket |
| Invalid `dkim_selectors` query | Malformed label, exceeds 253 chars total | HTTP 400 with field error | Error response before any backend call |
| Beacon returns summary without one of 11 categories | Partial output | Missing categories treated as `Skip`; bucket aggregates to worst of present categories | Bucket renders with partial coverage |
| Email backend not configured | `backends.email.url` unset | `EmailBackend` not registered in `AppState.backends`; no `email` event emitted; profile section silently absent from scoring | No email card rendered |

## Implementation Phases

### Phase 1 — Engine: introduce `SectionStatus`

Replace `SectionInput.errored: bool` with `SectionInput.status: SectionStatus`. Extend `OverallScore` with `not_applicable: HashMap<String, String>`. Update `score_section` and `compute_score`. Update all existing call sites in `src/check.rs::section_input_from_result` and tests. Add unit tests for `NotApplicable` behaviour.

Update `README.md` Scoring section to describe the three section states (SCORING SYNC RULE).

**Phase complete when:** `cargo test` green; `OverallScore.not_applicable` is populated in one new engine test; README reflects the new state machine.

### Phase 2 — Lift SSE reader into shared helper

Move `collect_sse` from `src/backends/dns.rs` into `src/backends/sse.rs`. Parameterize on the terminal event name (`done` for prism, `summary` for beacon). `dns.rs` calls the shared helper. No behaviour change for DNS.

**Phase complete when:** `cargo test` green (DNS backend tests unchanged); `dns.rs` imports from `super::sse`; no duplicated SSE code remains.

### Phase 3 — Prune email-specific checks from lens.dns

Remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` handling from `src/backends/dns.rs::parse_lint_event` path and `build_headline`. Remove corresponding entries from `profiles/default.toml` `sections.dns.checks`. Clear `sections.dns.hard_fail`. Update README Scoring section (weights + removed hard-fails + rationale that email now owns these).

No email backend yet — DNS section simply shrinks. Overall weights temporarily don't sum to 100 (TLS 40 + DNS 20 + HTTP 20 + IP 10 = 90); normalisation handles this, but the total is corrected in Phase 5.

**Phase complete when:** `cargo test` green including scoring regression test; README scoring table reflects the pruned DNS section; scoring regression fixtures updated for known domains.

### Phase 4 — Email backend skeleton + config

Add `BackendConfig` field to `BackendsConfig::email`. Add `src/backends/email.rs` with `EmailBackend` implementing `Backend`. Register conditionally in `AppState`. The backend calls beacon via the shared SSE helper, parses the summary event, produces all four buckets with pass-through verdicts (no N/A detection yet — that's Phase 5). Unit tests against golden fixtures for a happy-path mail domain.

Add `?dkim_selectors=` query param plumbing from routes → `CheckInput` → `EmailBackend::run` → beacon query string.

**Phase complete when:** `make dev` runs with `LENS_BACKENDS__EMAIL__URL` set; `/api/check/example.com` emits an `email` SSE event; fixtures tests green.

### Phase 5 — Per-bucket N/A + scoring weights

In `EmailBackend`, detect "no MX" by inspecting beacon's `mx` category verdict. When no MX: set `email_infrastructure`, `email_transport`, `email_brand_policy` check verdicts to `Skip` with the N/A message; populate `BackendExtra::Email.bucket_na`. Detect beacon's `grade: "Skipped"` and return `SectionStatus::NotApplicable { reason: "beacon timeout" }` at the section level.

Update `profiles/default.toml`: add `sections.email` with bucket weights; set `tls.weight = 35`; set `dns.weight = 20`. Total = 100.

Update `README.md` Scoring section with new weights table, explicit sending-vs-receiving explanation, per-bucket N/A rule. Add email card to SSE events table in README (already listed, revalidate wording).

**Phase complete when:** fixtures for no-MX and beacon-timeout scenarios verify correct bucket N/A and section N/A respectively; scoring regression test includes a no-MX domain; overall score for a no-MX domain reflects weight redistribution across remaining 4 sections.

### Phase 6 — Frontend

Add `EmailCard` component; wire the `email` SSE event; render four buckets with verdict badges; render "Not configured" neutral state for `not_applicable: true` buckets; add a one-line footnote to the overall grade card when any section is N/A (reads from `summary.not_applicable`).

Acceptance criteria:
- Mail domain shows 4 pass buckets.
- Non-mail domain (no MX) shows 1 scored `email_authentication` bucket and 3 "Not configured" buckets.
- Beacon timeout shows section-level "Email check unavailable" card.
- Beacon unreachable shows section-level error card (existing error pattern).

**Phase complete when:** `npm run build` green; manual test against `make dev` for three cases (mail, no-mx, beacon-off) produces correct UI.

### Phase 7 — Documentation (meta repo)

Cross-repo commit in `../site/` updating `site/guide/lens.html` (or equivalent) with the sending-vs-receiving scoring model, worked examples for a mail domain and a non-mail domain, and a call-out that `dkim_selectors` can be passed via URL. Link the guide from the email card if an affordance exists.

**Phase complete when:** guide page preview renders locally without broken links; content reviewed for parity with README prose.

## Test Scenarios

### Engine

GIVEN a `SectionInput { status: SectionStatus::NotApplicable { reason: "no MX" } }`
WHEN `score_section` is called
THEN it returns `None` and `compute_score` records `reason` on `OverallScore.not_applicable`.

GIVEN a `SectionInput { status: Errored }`
WHEN `score_section` is called
THEN it returns `None` and `not_applicable` is unchanged.

GIVEN all sections `Scored`
WHEN `compute_score` is called
THEN `not_applicable` is an empty map and behaviour matches pre-change baseline.

### Email backend

GIVEN a beacon summary with `mx.verdict = Pass` and all 11 categories passing
WHEN `EmailBackend::run` parses the event
THEN all four buckets return `CheckVerdict::Pass`, `BackendExtra::Email.bucket_na` is empty, `grade = Some("A")`.

GIVEN a beacon summary with `mx.verdict = Fail` (no MX records)
WHEN `EmailBackend::run` parses the event
THEN `email_authentication` scores normally, the other three buckets return `CheckVerdict::Skip` with message `"No MX records — email receiving not configured"`, and `BackendExtra::Email.bucket_na` names those three with reason `"no MX records"`.

GIVEN a beacon summary with `grade: "Skipped"` (beacon internal timeout)
WHEN `EmailBackend::run` inspects the grade
THEN it returns a `BackendResult`-less outcome that causes the section status to be `SectionStatus::NotApplicable { reason: "beacon timeout" }`.

GIVEN beacon returns HTTP 503
WHEN `EmailBackend::run` makes the request
THEN it returns `SectionError::BackendError` with a message containing "503".

GIVEN lens-side timeout fires (15 s)
WHEN the SSE drain exceeds the cap
THEN it returns `SectionError::Timeout`.

GIVEN `dkim_selectors = "google,selector1"`
WHEN the backend builds the beacon URL
THEN the outbound query contains `dkim_selectors=google%2Cselector1` (URL-encoded).

### Routes

GIVEN `dkim_selectors = "bad..selector"` on `/api/check`
WHEN the handler validates input
THEN it returns HTTP 400 before calling any backend.

### Orchestration

GIVEN email backend timeout and all other backends succeed
WHEN `run_check` completes
THEN the overall `CheckOutput` contains 4 successful sections plus email errored, and the overall percentage is computed over the 4 successful sections.

GIVEN a no-MX domain and all backends succeed
WHEN `run_check` completes
THEN `summary.not_applicable` is empty (section itself scored — only individual buckets were skipped), `email_authentication` contributed 10 of email's 22 bucket-weight total, and the email section percentage reflects authentication-only scoring.

### Regression

Update `tests/scoring_regression.rs` to include:
- A mail-sending domain (e.g. a known well-configured example): expected A/A+.
- A no-MX domain (e.g. a parked-style example): expected no email penalty, A or A+.
- A domain with bad SPF: expected score drop through `email_authentication`, not through lens.dns anymore.

## Decision Log

### Consume beacon via SSE, not a new JSON endpoint

Beacon exposes SSE as its primary API. Adding a synchronous JSON endpoint to beacon was considered but rejected — prism already streams SSE and lens consumes it, so SSE draining is an established pattern in the codebase. One shared helper covers both.

### Aggregate 11 categories into 4 buckets (not 11 individual checks)

Beacon's own UI groups into the same four buckets (per the user's reference screenshot), so mirroring that mapping is consistent end-to-end. Rendering 11 separate check rows in the lens UI would swamp the user and dilute the grade narrative. The HTTP backend already uses worst-verdict aggregation for the same reason (`http.rs::aggregate_worst`).

### Per-bucket N/A instead of per-section N/A

Initially the design put email as a single section that went N/A when no MX. That was wrong: SPF / DKIM / DMARC are anti-spoofing concerns that apply to every domain, including non-mail domains. Receiving-side checks (MTA-STS, TLS-RPT, DANE, FCrDNS, DNSBL, MX itself) legitimately don't apply without MX. Splitting N/A per-bucket preserves the spoofing signal for parked domains and doesn't hide legitimate gaps.

### Add explicit `SectionStatus::NotApplicable` to the engine

The existing engine silently excludes a section when `possible == 0` — all-Skip checks effectively behave as N/A. Relying on that as the N/A mechanism would work mechanically but buries intent. Per user preference, N/A is modelled as a first-class state: explicit in code, explicit on `OverallScore`, explicit in the UI. Generalises beyond email.

### Per-bucket N/A expressed via `CheckVerdict::Skip` (not a new verdict)

Skip already means "excluded from both earned and possible totals" at the check level — identical semantics to bucket-level N/A. Adding a new verdict for the same concept violates DRY. The per-bucket N/A signal for the UI is carried in `BackendExtra::Email.bucket_na`, not in the verdict — that separation keeps the scoring engine's verdict set minimal and lets the frontend render a distinct badge without the engine caring.

### Move SPF / DMARC / MTA-STS / TLS-RPT / BIMI / MX out of lens.dns

These are pure email concerns and beacon handles them with greater fidelity (recursive SPF expansion, DKIM selector rotation, MTA-STS policy fetch, etc.). Keeping them in lens.dns would double-count against the overall grade and penalise non-mail domains asymmetrically. DNSSEC stays in lens.dns because DNS integrity is a DNS-infrastructure concern independent of mail.

### Drop SPF / DMARC from `dns.hard_fail`

Hard-failing SPF or DMARC in the DNS section predates beacon. With email now a first-class section, hard-fails (if any) belong there. In v1 the email section has **no** hard-fails — real-world email-standard adoption is uneven and a single missing control (e.g. MTA-STS, DANE, strict DMARC policy) shouldn't F-grade an otherwise healthy domain. The bucket scoring already captures these gaps proportionally.

### No email hard-fails in v1

See above. DMARC `p=none` is a valid rollout state. DNSBL listings have false positives. MTA-STS / DANE are aspirational. A hard-fail on any of these would F most real-world domains and destroy score credibility.

### DKIM selectors via URL query (not request body or form field)

Minimal surface: `?dkim_selectors=a,b`. Works for GET and POST. Front-end integration (a selectors input field) is deferred. Power users and automation can pass via URL today.

### 15 s email timeout (not the full 20 s deadline)

Lens's hard deadline is 20 s. Beacon's internal cap is 30 s. If beacon runs close to its own cap, lens must still have margin to emit a summary before its hard deadline fires. 15 s gives 5 s of safety.

### Lift `collect_sse` now (not defer to third use)

DRY's rule of three says two uses is the threshold to *consider* extraction, not commit. The helper is mechanical (≈50 LOC), parameterising the terminal event is trivial, and postponing means the email backend duplicates code that will almost certainly need further changes (e.g. when a third SSE producer appears). Lift now.

### Explicit section-level `NotApplicable` used for beacon's own-timeout case

When beacon internally hits its 30-second cap, it emits `grade: "Skipped"` — the whole email check has no signal. Treating this as `Errored` is wrong (beacon worked fine, it just didn't finish). Treating it as silent exclusion via all-Skip checks would bury intent. Mapping to `SectionStatus::NotApplicable { reason: "beacon timeout" }` is the right fit and exercises the new engine primitive.

### README is updated in the same commit as each scoring change; guide follows in a cross-repo commit

SCORING SYNC RULE from `CLAUDE.md`. The README is the transparency contract — a reader must always be able to cross-check the README against the profile and get identical facts. Guide updates in the meta repo are separate because they live in a different git tree; sequencing is lens first, guide second.

## Open Decisions

### Bucket weights within the email section

Proposed: `email_authentication = 10`, `email_infrastructure = 5`, `email_transport = 5`, `email_brand_policy = 2`. Rationale: authentication weighted highest because it's the only bucket that always scores (anti-spoofing is universal); transport and infrastructure equal (both receiving-dependent, comparable blast radius); brand/policy lowest (BIMI + cross-validation are advisory). Section total stays at 15% of overall.

Alternative: `authentication = 10, infra = 5, transport = 7, brand = 3` — elevating transport because MTA-STS + TLS-RPT + DANE are the strongest levers against MITM on email transit.

Impact: low — worst case ±2 points on the overall percentage for a partially-configured domain.

### Should the sync-mode response include `summary.not_applicable` even when empty?

Options:
1. Always present (possibly empty object) — stable schema for consumers.
2. Present only when non-empty — smaller payload, matches current field-omission pattern.

Impact: client ergonomics only.

### DKIM selectors input on the frontend — now or Step 2?

SDD assumes URL-only input today and UI integration later. Confirm this is Step 2 scope (a small input next to the domain field with a tooltip) and not bundled into Step 1.

Impact: small amount of frontend work; if deferred, users rely on crafting the URL themselves or using the API.

## Out of Scope

- **Full per-check "Explain" feature** — beacon-style expandable affordance per check/bucket. Tracked as a separate SDD covering all lens backends (DNS, TLS, HTTP, IP, Email).
- **DKIM selector UI input field** — URL-only in v1.
- **Historical trending** — no storage of prior check results for the email section.
- **Email-specific hard-fails** — no hard-fail entries added for `sections.email` in v1; revisit after usage data is available.
- **Re-enabling `dns.hard_fail` with non-email entries** — not part of this change.
- **MCP server tool descriptions** — updating `check_domain` tool description to mention email is out of the lens repo (MCP server lives elsewhere); flagged for the MCP repo owner.
- **Deploy / infra changes** — adding beacon to the Compose topology is handled by the ops repo, not lens.
- **Logging/telemetry signature changes** — existing tracing spans per backend cover the email backend; no new conventions.
