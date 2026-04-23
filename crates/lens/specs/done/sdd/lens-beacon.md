# SDD: Integrate beacon as the Email Backend

Status: Ready for Implementation
Original: /Users/lukas/Documents/src/netray.info/lens/specs/sdd/lens-beacon.md
Refined: 2026-04-23

---

## Overview

Add beacon (`email.netray.info`) as the fifth lens backend so `/api/check/{domain}` returns an email-security assessment alongside DNS, TLS, HTTP, and IP. The email backend drains beacon's SSE stream, aggregates 11 beacon categories into four scored buckets, and marks three of those buckets not-applicable when no MX records exist. Simultaneously, the six email-specific checks currently in `lens.dns` are removed and ownership transferred to the email section.

---

## Context & Constraints

- Rust 2024 edition, Axum 0.8, reqwest with `stream` feature, SolidJS 1.9 frontend.
- `Backend` trait in `src/backends/mod.rs`; wave scheduling (`WAVE1_SECTIONS`, `WAVE2_SECTIONS`) in `src/check.rs`; SSE draining in `src/backends/dns.rs::collect_sse`; scoring engine in `src/scoring/engine.rs`.
- Config layered via `netray_common::backend::BackendConfig`. `http` backend is `Option<BackendConfig>` — the email backend follows the same optional pattern.
- `percent_encode` helper already exists in `src/backends/mod.rs`.
- `SectionInput.errored: bool` drives section exclusion today. This changes to `SectionInput.status: SectionStatus` in Phase 1.
- SCORING SYNC RULE (CLAUDE.md): any commit touching `src/scoring/`, `profiles/default.toml`, or scoring weights MUST update `README.md` Scoring section in the same commit.
- `compute_score` uses relative weights — the denominator is the sum of present sections' weights, so weights need not sum to any fixed constant. Stating that weights "must sum to 100" is a documentation convention only.
- Partial failure is required: one backend down never blocks the others.
- Cross-repo guide updates go to `../site/guide/` in the netray.info meta repo, committed after lens merges.

---

## Architecture

```
+------------------------+            +-----------------+
| lens /api/check/:domain|            | beacon          |
| (Axum + SSE)           |            | (email.netray)  |
+-----------+------------+            +--------+--------+
            |                                  ^
            | wave 1 (concurrent)              |
            |   +------+ +------+ +------+    | SSE drain
            |   | dns  | | tls  | | http |    | (15 s timeout)
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
            | wave 2
            |   +------+
            |   |  ip  |
            |   +------+
            v
   SSE events: dns -> tls -> http -> email -> ip -> summary -> done
```

Scoring engine state after Phase 1:

```
SectionStatus::Scored           -> score_section returns Some(SectionScore)
SectionStatus::Errored          -> score_section returns None; section absent from OverallScore.sections
SectionStatus::NotApplicable    -> score_section returns None; reason recorded in OverallScore.not_applicable
```

---

## Requirements

1. The system shall call beacon's SSE endpoint once per domain check, drain the stream to completion, and produce a single aggregated `email` section result.
2. The system shall run the email backend in wave 1 of the orchestration (concurrent with DNS, TLS, HTTP); the email backend has no cross-section data dependency.
3. The system shall aggregate beacon's 11 category verdicts into exactly four buckets using worst-verdict aggregation per bucket:
   - `email_authentication`: `spf`, `dkim`, `dmarc`
   - `email_infrastructure`: `mx`, `fcrdns`, `dnsbl`
   - `email_transport`: `mta_sts`, `tlsrpt`, `dane`
   - `email_brand_policy`: `bimi`, `dmarc_policy`
   Missing categories are treated as `Skip` for aggregation purposes.
4. The system shall score `email_authentication` for every domain regardless of MX presence.
5. The system shall mark `email_infrastructure`, `email_transport`, and `email_brand_policy` as not-applicable when the domain has no MX records (detected by `mx.verdict == Fail` in beacon's summary), using `CheckVerdict::Skip` for each bucket's check and recording the reason in `BackendExtra::Email.bucket_na`. These buckets contribute 0 to both `earned` and `possible` for the email section, so no penalty applies.
6. The system shall replace `SectionInput.errored: bool` with `SectionInput.status: SectionStatus` in `src/scoring/engine.rs`, where `SectionStatus` is:
   ```rust
   pub enum SectionStatus {
       Scored,
       Errored,
       NotApplicable { reason: String },
   }
   ```
   `score_section` returns `None` for both `Errored` and `NotApplicable`. `compute_score` records the reason string in `OverallScore.not_applicable: HashMap<String, String>` only for `NotApplicable`. `Errored` does not populate `not_applicable`. `SectionStatus` is `pub` in `engine.rs` and imported directly by `check.rs` and `email.rs`.
7. The system shall extend `OverallScore` with `pub not_applicable: HashMap<String, String>` (section name → reason). This field is always serialized, even when empty (stable schema for consumers).
8. The system shall remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, and `mx` from `src/backends/dns.rs` lint processing, from `profiles/default.toml` `[sections.dns.checks]`, and from `[sections.dns.hard_fail]`. `dnssec`, `dnskey_algorithm`, and `dnssec_rollover` remain.
9. The system shall not add any hard-fail entries for `[sections.email]` in v1.
10. The system shall accept an optional `dkim_selectors` query parameter on `GET /api/check/{domain}` and an optional `dkim_selectors` field (string or array) in the `POST /api/check` body. On the lens external boundary, accept comma-separated strings for human ergonomics. Lens validates each selector after splitting on `,` and trimming: characters in `[a-zA-Z0-9-]`, length 1–63 chars each, total count 1–10 selectors, no empty token. Invalid input returns HTTP 400 before any backend call. The validated selectors are forwarded to beacon as a JSON array in the POST body (see Req 10a).

10a. The system shall forward validated DKIM selectors to beacon by calling `POST {email_url}/inspect` with body `{"domain": <domain>, "dkim_selectors": [<sel1>, <sel2>, ...]}`. The `dkim_selectors` key is omitted from the body when no selectors are provided. Beacon's actual endpoint path is `/inspect` (not `/api/check`); the body field `dkim_selectors` is a JSON array of strings (not a comma-separated string). Beacon enforces its own per-selector validation (`validate_dkim_selector` in `beacon/src/input.rs`) and a max of 10 selectors; lens's validation is a strict subset, so valid lens input is always accepted by beacon.
11. The system shall emit a new `email` SSE event before `ip` carrying: bucket verdicts, per-bucket messages (worst-verdict check messages, max 5 per bucket), per-bucket `not_applicable` flag, beacon's reported grade, a `raw_headline`, and a `detail_url` to `https://email.netray.info/?domain=<encoded-domain>` (or the configured `public_url`).
12. The system shall enforce a 15-second timeout on the email backend call. The timeout wraps both the initial HTTP connect and the full SSE drain. On timeout: return `Err(SectionError::Timeout)`.
13. The email backend shall lift the SSE reader from `src/backends/dns.rs` into `src/backends/sse.rs` as a public function `collect` parameterized on the terminal event name. Both DNS and email call this shared function.
14. The email backend shall map beacon's `grade: "Skipped"` (beacon's own internal timeout sentinel) to `Err(SectionError::NotApplicable { reason: "beacon timeout".to_string() })`. `check.rs::section_input_from_result` maps `SectionError::NotApplicable` to `SectionInput { status: SectionStatus::NotApplicable { reason }, checks: vec![] }`.
15. The system shall update `profiles/default.toml` with the new weight distribution and add the `[sections.email]` block. The `README.md` Scoring section must be updated in the same commit (SCORING SYNC RULE).
16. The system shall render an `EmailSection` component in the frontend displaying four bucket rows. Buckets with `not_applicable: true` display the verdict badge text "N/A" (neutral/grey styling) and the message "No MX records — email receiving not configured". The overall `Summary` component shows a footnote when `summary.not_applicable` is non-empty.
17. The `SectionError` enum shall gain a new variant `NotApplicable { reason: String }` used exclusively by the email backend for the beacon-Skipped case.

---

## File & Module Structure

### New files

| Path | Purpose |
|---|---|
| `src/backends/email.rs` | `EmailBackend` struct + `Backend` impl; `parse_summary`, `map_buckets`, `detect_no_mx` pure functions |
| `src/backends/sse.rs` | `pub async fn collect(resp: reqwest::Response, terminal_event: &str) -> Result<Vec<Value>, String>` |
| `frontend/src/components/EmailSection.tsx` | Email section card with four bucket rows and N/A states |
| `tests/email_fixtures/mail_domain.json` | Beacon summary: all 11 categories pass |
| `tests/email_fixtures/no_mx.json` | Beacon summary: `mx.verdict = Fail` |
| `tests/email_fixtures/beacon_timeout.json` | Beacon summary: `grade = "Skipped"` |
| `tests/email_fixtures/partial_fail.json` | Beacon summary: mixed pass/warn/fail across buckets |

### Modified files

| Path | Change |
|---|---|
| `src/backends/mod.rs` | Add `pub mod email; pub mod sse;`; add `Email { ... }` variant to `BackendExtra`; add `dkim_selectors: Option<Vec<String>>` to `BackendContext` |
| `src/backends/dns.rs` | Call `super::sse::collect(resp, "done")` instead of `collect_sse`; remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` from `parse_lint_event` output and `build_headline`; `build_headline` updated to use `["dnssec", "caa", "ns", "cname_apex"]` with labels `["DNSSEC", "CAA", "NS", "CNAME-apex"]` |
| `src/check.rs` | Add `NotApplicable { reason: String }` variant to `SectionError` (defined in this file); add `dkim_selectors: Option<Vec<String>>` to `CheckInput`; update `section_input_from_result` to handle `SectionStatus` and `SectionError::NotApplicable`; populate `BackendContext.dkim_selectors` from `CheckInput` when building `wave1_context` |
| `src/scoring/engine.rs` | Replace `errored: bool` with `status: SectionStatus`; add `not_applicable: HashMap<String, String>` to `OverallScore`; update `score_section` and `compute_score`; update all test helpers |
| `src/config.rs` | Add `pub email: Option<netray_common::backend::BackendConfig>` to `BackendsConfig` |
| `src/state.rs` | Conditionally register `EmailBackend` after HTTP backend when `config.backends.email` is `Some` with a non-empty url |
| `src/routes.rs` | Add `dkim_selectors` to `CheckGetQuery` struct and `CheckPostBody` struct; validate and pass through to `CheckInput`; update `guide_url_for` (remove all email-check entries — see Phase 3 Commit A) |
| `profiles/default.toml` | See Configuration section |
| `lens.example.toml` | Add `[backends.email]` block |
| `README.md` | Update Scoring section: weights table, sending-vs-receiving split, per-bucket N/A rule, SSE events table |
| `frontend/src/lib/types.ts` | Add `EmailBucket`, `EmailEvent`, update `SummaryEvent` |
| `frontend/src/lib/sse.ts` | Add `onEmail` callback; register `email` event listener |
| `frontend/src/components/Summary.tsx` | Add N/A footnote when `summary.not_applicable` non-empty |
| `frontend/src/App.tsx` | Wire `email` SSE event to state; render `EmailSection` |
| `CHANGELOG.md` | Entry under `Unreleased` |

### Cross-repo (netray.info meta repo, separate commit after lens merges)

| Path | Change |
|---|---|
| `site/guide/lens.html` | New section: sending-vs-receiving scoring model, worked examples, `dkim_selectors` URL param |

---

## Data Models

### Rust — engine changes (`src/scoring/engine.rs`)

```rust
/// Replaces SectionInput.errored: bool
#[derive(Debug, Clone)]
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
    /// Section name → reason. Always present (may be empty). Populated only for NotApplicable.
    pub not_applicable: HashMap<String, String>,
}
```

`score_section` signature is unchanged: `fn score_section(section_checks: &HashMap<String, u32>, input: &SectionInput) -> Option<SectionScore>`. It returns `None` when `status` is `Errored` or `NotApplicable`.

`compute_score` populates `not_applicable` when processing a `NotApplicable` input:
```rust
// pseudocode inside compute_score loop
SectionStatus::NotApplicable { reason } => {
    not_applicable.insert(name.clone(), reason.clone());
}
```

### Rust — check.rs changes

```rust
pub enum SectionError {
    BackendError(String),
    Timeout,
    NoDnsResults,
    NotApplicable { reason: String },  // NEW: beacon-Skipped case
}

pub struct CheckInput {
    pub domain: String,
    pub dkim_selectors: Option<Vec<String>>,  // NEW: validated, split, trimmed
}
```

`section_input_from_result` updated:
```rust
fn section_input_from_result(result: &Result<BackendResult, SectionError>) -> SectionInput {
    match result {
        Ok(r) => SectionInput { checks: r.checks.clone(), status: SectionStatus::Scored },
        Err(SectionError::NotApplicable { reason }) => SectionInput {
            checks: vec![],
            status: SectionStatus::NotApplicable { reason: reason.clone() },
        },
        Err(_) => SectionInput { checks: vec![], status: SectionStatus::Errored },
    }
}
```

`run_backends` must pass `CheckInput` (not just `domain: &str`) to `EmailBackend::run`. The `Backend` trait signature remains `fn run(&self, domain: &str, context: &BackendContext)`. The email backend receives `dkim_selectors` by storing it as a field set per-request — since backends are stateless structs, the selector is passed via `BackendContext`:

```rust
pub struct BackendContext {
    pub resolved_ips: Vec<IpAddr>,
    pub dkim_selectors: Option<Vec<String>>,  // NEW: forwarded to email backend
}
```

### Rust — `BackendExtra::Email` (`src/backends/mod.rs`)

```rust
Email {
    raw_headline: String,           // e.g. "Auth: OK  Infra: N/A  Transport: N/A  Brand: N/A"
    detail_url: String,             // https://email.netray.info/?domain=example.com
    grade: Option<String>,          // beacon's grade: "A"/"B"/"C"/"D"/"F"/"Skipped"/None
    /// Bucket name → reason string for buckets that are not-applicable.
    /// UI uses this to render "Not configured" badge. Empty when all buckets scored.
    bucket_na: HashMap<String, String>,
}
```

Note: `raw_headline` uses ASCII text only (`OK`, `Warn`, `Fail`, `N/A`) — no emoji, consistent with project rule (no emoji in files/values).

### Rust — `src/backends/sse.rs`

```rust
/// Drain an SSE byte-stream from a reqwest Response into a Vec of JSON events.
///
/// Each event is returned as `{"type": "<event-name>", "data": <parsed-json>}`.
/// Reading stops when an event matching `terminal_event` is dispatched, or when
/// the stream ends. Returns Err on a chunk read error or UTF-8 decode failure.
pub async fn collect(
    resp: reqwest::Response,
    terminal_event: &str,
) -> Result<Vec<Value>, String>
```

`dns.rs` calls: `sse::collect(resp, "done").await`
`email.rs` calls: `sse::collect(resp, "summary").await`

### TypeScript — `frontend/src/lib/types.ts` additions

```typescript
export interface EmailBucket {
  verdict: Verdict;
  messages: string[];
  not_applicable: boolean;
}

export interface EmailEvent {
  status: 'ok' | 'error' | 'not_applicable';
  grade?: string;
  buckets: {
    email_authentication: EmailBucket;
    email_infrastructure: EmailBucket;
    email_transport: EmailBucket;
    email_brand_policy: EmailBucket;
  };
  headline: string;
  detail_url: string;
}

// SummaryEvent — existing fields preserved; `not_applicable` is the only new field.
// `hard_fail_reason` already exists in the current code (string when hard_fail=true, else omitted/null).
export interface SummaryEvent {
  sections: Record<string, Verdict>;
  section_grades: Record<string, string>;
  overall: Verdict;
  grade: string;
  score: number;
  hard_fail: boolean;
  hard_fail_checks: string[];
  hard_fail_reason?: string;               // PRE-EXISTING — do not add, verify present
  not_applicable: Record<string, string>;  // NEW: always present, may be empty
}
```

---

## API Contracts

### Request

```
GET  /api/check/{domain}?dkim_selectors=sel1,sel2
POST /api/check
     Content-Type: application/json
     { "domain": "example.com", "dkim_selectors": "sel1,sel2" }
```

`dkim_selectors` validation (lens-side, before any backend call):
- Split on `,`; trim whitespace from each token.
- Each token: characters `[a-zA-Z0-9-]`, length 1–63 chars.
- Total count: 1–10 selectors.
- Empty string (`dkim_selectors=`) → HTTP 400.
- Absent → omit from beacon query (beacon uses built-in provider map).

Outbound beacon URL (POST to beacon's `/inspect`):
```
POST https://email.netray.info/inspect
Content-Type: application/json

{ "domain": "example.com", "dkim_selectors": ["sel1", "sel2"] }
```

Notes:
- Beacon's endpoint path is `/inspect`, not `/api/check`.
- The body field `dkim_selectors` is a **JSON array of strings**, not a comma-separated string. When no selectors are validated, omit the key entirely from the body.
- Beacon also exposes `GET /inspect/{domain}?selector=sel1&selector=sel2` (repeatable `selector` query param). Lens uses the POST form exclusively for consistency with how `dns.rs` drives prism.
- Response is SSE (`text/event-stream`); beacon emits `category` events during inspection and exactly one `summary` event at the end. Lens drains until the `summary` event (see `sse::collect(resp, "summary")`).

### SSE event — `email`

```json
{
  "status": "ok",
  "grade": "A",
  "buckets": {
    "email_authentication": {
      "verdict": "pass",
      "messages": [],
      "not_applicable": false
    },
    "email_infrastructure": {
      "verdict": "skip",
      "messages": ["No MX records — email receiving not configured"],
      "not_applicable": true
    },
    "email_transport": {
      "verdict": "skip",
      "messages": ["No MX records — email receiving not configured"],
      "not_applicable": true
    },
    "email_brand_policy": {
      "verdict": "skip",
      "messages": ["No MX records — email receiving not configured"],
      "not_applicable": true
    }
  },
  "headline": "Auth: OK  Infra: N/A  Transport: N/A  Brand: N/A",
  "detail_url": "https://email.netray.info/?domain=example.com"
}
```

When email errored: `{ "status": "error", "error": "<message>" }`.
When section NotApplicable: `{ "status": "not_applicable", "reason": "beacon timeout" }`.

### SSE event — `summary` (additions)

```json
{
  "overall": "A",
  "grade": "A",
  "score": 87.0,
  "hard_fail": false,
  "hard_fail_checks": [],
  "sections": { "dns": "pass", "tls": "pass", "http": "pass", "email": "pass", "ip": "pass" },
  "section_grades": { "dns": "A", "tls": "A+", "http": "A", "email": "A", "ip": "A" },
  "not_applicable": {}
}
```

`not_applicable` is always present. When a section is N/A: `"not_applicable": { "email": "beacon timeout" }`. `hard_fail_reason` already exists in the current response shape — do not remove it.

---

## Configuration

### `profiles/default.toml` (after all phases)

```toml
[sections.tls]
weight = 35      # was 40

[sections.dns]
weight = 20      # was 30
hard_fail = []   # was ["spf", "dmarc"]

[sections.dns.checks]
# removed: spf, dmarc, mta_sts, tlsrpt, bimi, mx
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

[sections.http]
weight = 20      # unchanged

[sections.ip]
weight = 10      # unchanged

[sections.email]
weight = 15
hard_fail = []

[sections.email.checks]
email_authentication = 10   # always scored
email_infrastructure = 5    # N/A when no MX (CheckVerdict::Skip)
email_transport      = 5    # N/A when no MX
email_brand_policy   = 2    # N/A when no MX
# Total section weights: tls(35) + dns(20) + http(20) + ip(10) + email(15) = 100
```

Note: during Phase 3 (DNS pruned, email not yet added), the active weight pool is 90. `compute_score` uses relative weights so this is numerically correct. README is updated in Phase 5 when weights reach 100.

### `lens.example.toml` addition

```toml
[backends.email]
url = "http://beacon:8085"
# Public URL for user-facing `detail_url` is sourced from
# [ecosystem] email_base_url — not a backend-level field. This matches
# how dns/tls/http/ip backends source their public URLs.

[ecosystem]
# email_base_url = "https://email.netray.info"   # used to build detail_url
```

`BackendConfig` (provided by `netray_common::backend::BackendConfig` v0.7.0) does NOT have a `public_url` field. It exposes: `url`, `timeout_ms`, `max_concurrent`, `cache_ttl_secs`, `cache_capacity`. The public URL for every backend (dns, tls, http, ip, **email**) comes from `EcosystemConfig` (`netray_common::ecosystem::EcosystemConfig`), which already defines `email_base_url: Option<String>` in the v0.7.0 release. `state.rs` reads it the same way as the other four backends:
```rust
public_url: eco.email_base_url.clone().unwrap_or_default(),
```

### Environment variables

| Variable | Type | Default | Notes |
|---|---|---|---|
| `LENS_BACKENDS__EMAIL__URL` | URL string | none | Absent → email backend not registered |
| `LENS_ECOSYSTEM__EMAIL_BASE_URL` | URL string | empty | Public URL used when constructing the `detail_url` in the `email` SSE event; falls back to using `url` if empty |

No `timeout_secs` override — the 15-second timeout is hardcoded in `EmailBackend` (matching the 5 s safety margin before lens's 20 s hard deadline).

---

## Error Handling

| Failure | Trigger | Behaviour | User-visible |
|---|---|---|---|
| beacon unreachable | Connection refused / DNS failure on reqwest send | `Err(SectionError::BackendError(msg))` | `email` SSE event: `{"status":"error","error":"..."}` |
| beacon HTTP non-2xx | Non-success status from beacon | `Err(SectionError::BackendError("beacon returned HTTP <N>"))` | Same as above; tracing warn with status code |
| lens 15 s timeout | reqwest send or SSE drain exceeds 15 s | `Err(SectionError::Timeout)` | `email` SSE event: `{"status":"error","error":"timeout"}` |
| beacon `grade: "Skipped"` | Beacon's own 30 s cap (rare — lens fires first) | `Err(SectionError::NotApplicable { reason: "beacon timeout" })` → `SectionStatus::NotApplicable` | `email` SSE event: `{"status":"not_applicable","reason":"beacon timeout"}`; summary footnote |
| Domain has no MX | `mx.verdict == Fail` in beacon summary | `email_infrastructure`, `email_transport`, `email_brand_policy` use `CheckVerdict::Skip`; `email_authentication` scores normally | Email card shows 1 scored bucket + 3 "Not configured" buckets |
| Missing beacon category | Fewer than 11 categories in summary | Missing category treated as `Skip` | Bucket scores on present categories only |
| Invalid `dkim_selectors` | Bad label chars, >63 chars, >10 selectors, empty | HTTP 400 with structured error before backend calls | Error response |
| Email backend not configured | `LENS_BACKENDS__EMAIL__URL` unset | `EmailBackend` not registered; no `email` SSE event; profile `email` section silently absent from scoring | No email card rendered |

---

## Implementation Phases

### Phase 1 — Engine: `SectionStatus`

**Files changed:** `src/scoring/engine.rs`, `src/check.rs`, `README.md`

1. Define `pub enum SectionStatus` and `pub use` it where needed in `engine.rs`.
2. Replace `SectionInput.errored: bool` with `SectionInput.status: SectionStatus`.
3. Update `score_section`: return `None` for `Errored` and `NotApplicable`.
4. Add `not_applicable: HashMap<String, String>` to `OverallScore`; always serialize (no `skip_serializing_if`).
5. Update `compute_score`: populate `not_applicable` on `NotApplicable` status.
6. Add `SectionError::NotApplicable { reason: String }` to `src/check.rs`.
7. Update `section_input_from_result` to handle `SectionError::NotApplicable` → `SectionStatus::NotApplicable`.
8. Update `build_score_from_errors` in `check.rs` to use `SectionStatus::Errored`.
9. Update all existing tests that construct `SectionInput { checks, errored }` directly:
   - Engine unit tests in `src/scoring/engine.rs` — `no_error()` helper gains `status: SectionStatus::Scored`, `errored()` helper uses `SectionStatus::Errored`.
   - Integration test `tests/scoring_regression.rs` — same helper updates (the file defines its own `no_error`/`errored` constructors at lines 48–60).
   Confirm both locations are updated; `grep -rn 'errored:' src/ tests/` must return zero results after Phase 1.
10. Add three new engine unit tests: `NotApplicable` → `None` + reason recorded; `Errored` → `None` + reason not recorded; all `Scored` → `not_applicable` empty.
11. Update `README.md` Scoring section: describe the three section states.

**Complete when:** `cargo test` green; new engine tests pass; README reflects the three states.

### Phase 2 — Shared SSE helper

**Files changed:** `src/backends/sse.rs` (new), `src/backends/mod.rs`, `src/backends/dns.rs`

1. Create `src/backends/sse.rs` with `pub async fn collect(resp: reqwest::Response, terminal_event: &str) -> Result<Vec<Value>, String>`. Move the body of `dns.rs::collect_sse` into it, replacing the hardcoded `"done"` terminal check with the `terminal_event` parameter.
2. Add `pub mod sse;` to `src/backends/mod.rs`.
3. In `dns.rs`, replace `collect_sse(resp).await` with `super::sse::collect(resp, "done").await`. Delete `collect_sse`.

**Complete when:** `cargo test` green; DNS backend tests unchanged; `collect_sse` no longer exists in `dns.rs`.

### Phase 3 — Prune email checks from lens.dns

**Files changed:** `src/backends/dns.rs`, `profiles/default.toml`

Two separate commits:
- **Commit A** (`refactor(dns):`): remove `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` from `parse_lint_event` filtering (these categories can still come in the SSE stream — just don't emit `CheckResult` for them). Update `build_headline` to show `["dnssec", "caa", "ns", "cname_apex"]` with labels `["DNSSEC", "CAA", "NS", "CNAME-apex"]`. In `src/routes.rs::guide_url_for`, remove the entries for `spf`, `dmarc`, `mta_sts`, `tlsrpt`, `bimi`, `mx` (all six) since these checks no longer appear in DNS output.
- **Commit B** (`chore(scoring):`): remove those six keys from `[sections.dns.checks]`; clear `hard_fail = []`; set `dns.weight = 20`. Update `README.md` Scoring section (weight table shows tls=40, dns=20, http=20, ip=10 summing to 90; note that email section will be added in a later phase). Update scoring regression fixtures to remove SPF/DMARC from DNS section expectations.

**Complete when:** `cargo test` green; hard-fail tests for `spf`/`dmarc` removed from engine tests (they now live in email section); `tests/scoring_regression.rs` assertion at line 131 (`dns.hard_fail` non-empty) removed or updated to expect empty; DNS section regression baseline updated.

### Phase 4 — Email backend skeleton + config

**Files changed:** `src/backends/email.rs` (new), `src/backends/mod.rs`, `src/check.rs`, `src/config.rs`, `src/state.rs`, `src/routes.rs`, `lens.example.toml`

1. Add `pub email: Option<netray_common::backend::BackendConfig>` to `BackendsConfig` in `config.rs`.
2. Add `dkim_selectors: Option<Vec<String>>` to `BackendContext` in `backends/mod.rs` (already validated and split at the routes layer).
3. Add `dkim_selectors: Option<Vec<String>>` to `CheckInput` in `check.rs`. Update `run_backends` to populate `wave1_context.dkim_selectors` from `CheckInput`.
4. Add `?dkim_selectors=` query param to `CheckGetQuery` and `CheckPostBody` in `routes.rs` (note: these are the **actual** struct names — do not invent new ones). Validate per Req 10; HTTP 400 on failure. Pass validated value to `CheckInput`.
5. Add `BackendExtra::Email { ... }` to `src/backends/mod.rs`.
6. Create `src/backends/email.rs`:
   - `pub struct EmailBackend { pub email_url: String, pub public_url: String, pub timeout: Duration, pub client: reqwest::Client }`
   - `impl Backend for EmailBackend` with `section() -> "email"`
   - `run` method: POST to `{email_url}/inspect` (beacon's actual endpoint) with body `{"domain": domain, "dkim_selectors": selectors_array}` (omit the `dkim_selectors` key when None or empty); wrap call + drain in `tokio::time::timeout(self.timeout, ...)`; on timeout return `Err(SectionError::Timeout)`. Use `sse::collect(resp, "summary")` to drain until beacon's terminal `summary` event.
   - Parse beacon summary event (event type `"summary"`) from drained events. Implement `parse_summary(events: &[Value]) -> Result<BeaconSummary, SectionError>`.
   - Implement `map_buckets(summary: &BeaconSummary) -> [CheckResult; 4]` as a standalone pure function (independently testable). The four results have names `email_authentication`, `email_infrastructure`, `email_transport`, `email_brand_policy`. In Phase 4, no N/A detection yet — all buckets aggregate worst verdict from present categories, missing = Skip.
   - Detect `grade == "Skipped"` → `Err(SectionError::NotApplicable { reason: "beacon timeout".to_string() })`.
   - Build `BackendExtra::Email` and return `Ok(BackendResult { checks, extra })`.
7. Add `pub mod email;` to `src/backends/mod.rs`.
8. Conditionally register `EmailBackend` in `state.rs` after HTTP backend, before IP. Source `public_url` from `EcosystemConfig.email_base_url`, matching the pattern used for dns/tls/http/ip:
   ```rust
   if let Some(ref email_cfg) = config.backends.email
       && let Some(ref url) = email_cfg.url
   {
       backends.push(Box::new(EmailBackend {
           email_url: url.clone(),
           public_url: eco.email_base_url.clone().unwrap_or_else(|| url.clone()),
           timeout: Duration::from_secs(15),
           client: client.clone(),
       }));
   }
   ```
9. Add `WAVE1_SECTIONS` entry `"email"` in `check.rs`.
10. Add `[backends.email]` block to `lens.example.toml`.
11. Add unit tests in `email.rs` using `mail_domain.json` and `beacon_timeout.json` fixtures.

**Complete when:** `make dev` with `LENS_BACKENDS__EMAIL__URL` set emits an `email` SSE event; fixture tests green; `?dkim_selectors=` round-trip test passes.

### Phase 5 — Per-bucket N/A + scoring weights

**Files changed:** `src/backends/email.rs`, `profiles/default.toml`, `README.md`

Two separate commits:
- **Commit A** (`feat(email):`): add `detect_no_mx(summary: &BeaconSummary) -> bool` function. In `map_buckets`, when `detect_no_mx` is true: set `email_infrastructure`, `email_transport`, `email_brand_policy` to `CheckResult { verdict: CheckVerdict::Skip, messages: vec!["No MX records — email receiving not configured".to_string()], ... }`. Populate `BackendExtra::Email.bucket_na` with those three names → `"no MX records"`. Add fixture tests for `no_mx.json` and `partial_fail.json`.
- **Commit B** (`chore(scoring):`): add `[sections.email]` to `profiles/default.toml` with weights as specified; set `tls.weight = 35`; set `dns.weight = 20` (already set in Phase 3). Total = 100. Update `README.md` Scoring section: complete weights table (all five sections), sending-vs-receiving explanation, per-bucket N/A rule, note that `email_authentication` always scores.

**Complete when:** no-MX fixture test shows `possible = 10` (authentication weight only); overall score for no-MX domain does not penalise for missing MX infrastructure; `cargo test` green.

### Phase 6 — Frontend

**Files changed:** `frontend/src/lib/types.ts`, `frontend/src/lib/sse.ts`, `frontend/src/components/EmailSection.tsx`, `frontend/src/components/Summary.tsx`, `frontend/src/App.tsx`

1. Add `EmailBucket`, `EmailEvent` to `types.ts`; add `not_applicable: Record<string, string>` to `SummaryEvent`.
2. Add `onEmail: (data: EmailEvent) => void` to `SseCallbacks` in `sse.ts`; register `email` event listener.
3. Create `EmailSection.tsx` mirroring structure of `DnsSection.tsx`/`TlsSection.tsx`:
   - Four bucket rows; each row uses `VerdictDot` (or equivalent badge).
   - Buckets with `not_applicable: true` display verdict text "N/A" (CSS class `verdict--na`, neutral grey) and message "No MX records — email receiving not configured".
   - Error state: single row "Email check unavailable" with error styling.
   - NotApplicable state (section-level beacon timeout): single row "Email check unavailable (timed out)" with neutral styling.
   - `detail_url` links to beacon service.
4. Update `Summary.tsx`: when `summaryData.not_applicable` has entries, append a footnote line below the grade card: "* Score computed without: <comma-joined section names> (<reason>)."
5. Update `App.tsx` to wire email state and render `<EmailSection />`.

**Complete when:** `npm run build` green; manual test against `make dev` for three cases: mail domain (4 scored buckets), no-MX domain (1 scored + 3 N/A buckets), beacon-off (error card visible, other sections unaffected).

### Phase 7 — Documentation (meta repo, cross-repo commit)

**Files changed:** `../site/guide/lens.html` in netray.info meta repo

Add a new section "Email security scoring" explaining:
- Sending identity checks (`email_authentication`) apply to all domains.
- Receiving infrastructure checks apply only when MX records exist.
- `dkim_selectors` URL parameter.
- Worked example: mail domain vs. parked domain.

**Complete when:** guide page renders without broken links locally; content matches README prose.

---

## Test Scenarios

### Engine — `SectionStatus`

GIVEN `SectionInput { status: SectionStatus::NotApplicable { reason: "no MX" }, checks: vec![] }`
WHEN `score_section` is called
THEN it returns `None`.
AND `compute_score` records `"email" → "no MX"` in `OverallScore.not_applicable`.
AND the email section is absent from `OverallScore.sections`.

GIVEN `SectionInput { status: SectionStatus::Errored, checks: vec![] }`
WHEN `score_section` is called
THEN it returns `None`.
AND `OverallScore.not_applicable` does not contain `"email"`.

GIVEN all sections have `SectionStatus::Scored` with passing checks
WHEN `compute_score` is called
THEN `OverallScore.not_applicable` is an empty map.
AND behaviour matches the pre-Phase-1 baseline (grade A+, hard_fail false).

### Email backend — fixture-based

GIVEN `mail_domain.json` fixture (all 11 categories pass, MX present, grade "A")
WHEN `EmailBackend::run` parses the summary event
THEN all four `CheckResult`s have `CheckVerdict::Pass`.
AND `BackendExtra::Email.bucket_na` is empty.
AND `BackendExtra::Email.grade == Some("A")`.

GIVEN `no_mx.json` fixture (`mx.verdict = Fail`, other infra/transport/brand categories fail, auth categories pass)
WHEN `EmailBackend::run` parses the summary event
THEN `email_authentication` has `CheckVerdict::Pass` (or the actual worst of spf/dkim/dmarc).
AND `email_infrastructure`, `email_transport`, `email_brand_policy` each have `CheckVerdict::Skip`.
AND `bucket_na` contains exactly those three keys, all with reason `"no MX records"`.

GIVEN `beacon_timeout.json` fixture (any summary with `grade = "Skipped"`)
WHEN `EmailBackend::run` processes the events
THEN it returns `Err(SectionError::NotApplicable { reason: "beacon timeout" })`.

GIVEN `partial_fail.json` fixture (`spf=Pass`, `dkim=Warn`, `dmarc=Pass`; all infrastructure/transport/brand pass)
WHEN `map_buckets` processes the summary
THEN `email_authentication.verdict == CheckVerdict::Warn` (worst of Pass/Warn/Pass).
AND `email_authentication.messages` contains the dkim warning message (max 5 messages).

GIVEN beacon returns HTTP 503
WHEN `EmailBackend::run` makes the request
THEN it returns `Err(SectionError::BackendError("beacon returned HTTP 503"))`.

GIVEN lens 15 s timeout fires (beacon unresponsive)
WHEN `EmailBackend::run` completes
THEN it returns `Err(SectionError::Timeout)` within 15 s ± 500 ms.

### Routes — `dkim_selectors` validation

GIVEN `dkim_selectors = "google,selector1"` on GET `/api/check/example.com`
WHEN the handler validates input
THEN `CheckInput.dkim_selectors == Some(vec!["google".into(), "selector1".into()])`.
AND beacon's outbound POST body contains `"dkim_selectors": ["google","selector1"]` (JSON array).

GIVEN `dkim_selectors = "bad..selector"` (empty label between dots)
WHEN the handler validates input
THEN it returns HTTP 400 before any backend call.

GIVEN `dkim_selectors = ""` (empty string)
WHEN the handler validates input
THEN it returns HTTP 400.

GIVEN `dkim_selectors` absent from request
WHEN the handler processes the request
THEN `CheckInput.dkim_selectors == None`.
AND beacon's outbound POST body omits the `dkim_selectors` key entirely.

GIVEN `dkim_selectors = "a,b,c,d,e,f,g,h,i,j,k"` (11 selectors)
WHEN the handler validates input
THEN it returns HTTP 400 with message referencing the 10-selector limit before any backend call.

### Orchestration

GIVEN email backend timeout + all other backends succeed
WHEN `run_check` completes
THEN `CheckOutput.sections["email"] == Err(SectionError::Timeout)`.
AND `OverallScore.sections` does not contain `"email"`.
AND `OverallScore.overall_percentage` is computed over the four remaining sections.

GIVEN no-MX domain + all backends succeed
WHEN `run_check` completes
THEN `OverallScore.not_applicable` is empty (section scored, only individual buckets are Skip).
AND `OverallScore.sections["email"].possible == 10` (authentication weight only).
AND `OverallScore.sections["email"].percentage` reflects authentication-only result.

### Scoring regression (`tests/scoring_regression.rs`)

Add fixture-based tests (no live network calls):
- Full-pass email section: expected grade A+, `not_applicable` empty.
- No-MX domain (authentication pass, three buckets skip): expected no grade penalty vs. full-pass domain; section percentage reflects authentication-only scoring.
- Authentication fail (SPF/DKIM/DMARC all fail): grade drops through `email_authentication` score, not through `dns` section.

---

## Decision Log

### Consume beacon via SSE, not a JSON endpoint

Prism already streams SSE; lens drains it with `collect_sse`. Adding a synchronous JSON endpoint to beacon was rejected because one shared SSE helper covers both backends with no new protocol surface.

### Aggregate into 4 buckets (not 11 individual checks)

Beacon's own UI uses the same four groups. Rendering 11 rows would overwhelm the UI. The HTTP backend already uses worst-verdict aggregation for identical reasons (`http.rs::aggregate_worst`).

### Per-bucket N/A via `CheckVerdict::Skip` (not a new verdict)

`Skip` already means "excluded from both earned and possible totals." The per-bucket N/A signal for the UI is carried in `BackendExtra::Email.bucket_na`, keeping the scoring engine's verdict set minimal.

### Explicit `SectionStatus::NotApplicable` in the engine

The engine already returns `None` when `possible == 0` (all-Skip). Relying on that silently buries intent. `NotApplicable` is first-class: explicit in code, on `OverallScore`, and in the UI. Scoped to email only in v1 — generalise when a second caller appears.

### `SectionError::NotApplicable` for beacon-Skipped

Beacon's `grade: "Skipped"` means beacon completed normally but had no signal (its own timeout). This is not a backend error and not a lens timeout. A dedicated `SectionError` variant maps cleanly to `SectionStatus::NotApplicable` in `section_input_from_result` without conditional logic on strings.

### Move SPF/DMARC/MTA-STS/TLS-RPT/BIMI/MX out of lens.dns

Beacon handles these with greater fidelity (recursive SPF expansion, MTA-STS policy fetch, etc.). Keeping them in DNS would double-count email concerns against the overall grade. DNSSEC stays in DNS (DNS infrastructure, independent of mail).

### Drop `dns.hard_fail` entries

Hard-failing SPF/DMARC predates beacon. With email a first-class section, hard-fails (if any) belong there. In v1 no email hard-fails exist — email standard adoption is uneven and a single missing control should not F-grade an otherwise healthy domain.

### No `timeout_secs` config for email backend

The 15 s timeout is load-bearing (leaves 5 s safety margin before lens's 20 s hard deadline). Exposing it as an operator-tunable env var invites misconfiguration. No existing backend exposes per-backend timeout overrides in `BackendConfig`.

### Lift `collect_sse` at two uses

The helper is ~50 LOC. Parameterising the terminal event is trivial. Two uses (DNS + email) is enough to justify extraction, and delaying means the email backend duplicates code that will need future changes.

### `dkim_selectors` via URL/body (not a new form field)

Minimal surface today. Power users and automation can pass via URL. Frontend UI input field is explicitly deferred to a future iteration.

### `summary.not_applicable` always serialized

Stable schema for consumers. Matches the principle of predictable response shapes over minimal payloads. The field is cheap (usually an empty object).

### `raw_headline` uses ASCII text values, not emoji

The project CLAUDE.md rule prohibits emoji in files. Runtime string values are files-at-rest in the response body and server logs. Using `OK`, `Warn`, `Fail`, `N/A` is unambiguous and avoids rendering issues in log aggregators.

### Bucket weights: auth=10, infra=5, transport=5, brand=2

Chosen over the alternative `auth=10, infra=5, transport=7, brand=3`. Reasons:
- **KISS**: "authentication is highest because it applies to every domain; the three receiving buckets share equal weight because they are the same class of signal." One-sentence README.
- **Anti-spoofing primacy**: auth = 45% of the email section (10/22). SPF/DKIM/DMARC are the universal anti-impersonation floor; weighting them dominantly reflects real-world attack surface.
- **Per-check weight ordering is correct**: auth 3.3/check > infra 1.7/check > transport 1.25/check > brand 1.0/check. Each auth check (SPF, DKIM, DMARC) is individually more load-bearing than each transport check (MTA-STS, TLS-RPT, DANE) and than brand's BIMI/cross-validation which are advisory.
- **Adjustable later**: easier to add weight to transport once telemetry shows transport gaps as the dominant deduction than to defend an unjustified asymmetry on day one.

### Beacon endpoint is `/inspect` and body uses a JSON array

Verified against `beacon/src/routes.rs`. `POST /inspect` takes `InspectRequest { domain: String, dkim_selectors: Vec<String> }`. Lens's external API keeps the comma-separated URL form for human ergonomics; internal wire uses the array form that matches beacon's schema exactly — no string-splitting on beacon's side, and validation errors surface cleanly.

### Phase 3 two-commit split

`CLAUDE.md` forbids mixing formatting-only/config changes with functional changes in the same commit. Removing DNS parse logic is a backend change; updating the profile weights is a config change. Two commits keeps each independently revertable.

---

## Open Decisions

None. Both previously-open decisions (beacon API contract and bucket weights) are now resolved above — see Req 10a and the `[sections.email.checks]` block respectively. See Decision Log for rationale.

---

## Out of Scope

- **Full per-check "Explain" affordance** — expandable detail per check/bucket. Tracked as a separate SDD covering all backends.
- **DKIM selector UI input field** — URL-only in v1.
- **Historical trending** — no storage of prior email check results.
- **Email-specific hard-fails** — revisit after usage data is available.
- **Re-enabling `dns.hard_fail` with non-email entries** — not part of this change.
- **MCP server tool description updates** — `check_domain` tool in the MCP server repo; flagged for that repo's owner.
- **Deploy/infra changes** — adding beacon to the Compose topology is in the ops repo.
- **Logging/telemetry signature changes** — existing tracing spans per backend cover email; no new conventions needed.
(No deferred cleanup — `guide_url_for` pruning now folded into Phase 3 Commit A.)
