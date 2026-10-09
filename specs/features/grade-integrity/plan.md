# Plan: grade integrity

## Phase 1 — Errored causes

### Groups

1. SSE collector: C1, C3–C6 — `crates/lens/src/backends/sse.rs`.
2. Unknown verdicts: C2, C7–C9 — `crates/lens/src/backends/{dns,tls,email,ip,http}.rs`.

The groups share no production file; one coder builds both in order.

### Plan

- `sse.rs` `drain`: accumulate every `data:` line of an event into a buffer joined with `\n`; on the blank line, parse the buffer, and return `Err` when it is not JSON; when the stream ends without the terminal event the caller asked for, return `Err` ("stream ended without <type>"). `collect` and `collect_until_type` keep their signatures.
- A small helper in `crates/lens/src/backends/mod.rs`, e.g. `fn unknown_verdict(section: &'static str, value: &str) -> BackendError` (or the module's error type), which does `metrics::counter!("lens_unknown_verdict_total", "section" => section).increment(1)`, logs `tracing::warn!`, and returns the error that makes the section Errored.
- `dns.rs` `classify_lint_result`: return a `Result`; an object without `Ok`/`Warning`/`Failed`/`NotFound` (or a non-object) is unknown → helper("dns").
- `tls.rs` status maps (`:214`, `:235`): explicit arms for tlsight's `CheckStatus` serde values (pass, warn, fail, skip); anything else → helper("tls").
- `email.rs` `parse_beacon_verdict` (`:400-408`): explicit arms for beacon's `Verdict` serde values (pass, warn, fail, info, skip; `info` and `skip` map as today); anything else → helper("email").
- `ip.rs` `network_type_verdict` (`:250-258`): explicit arms for ifconfig-rs's network type values (read `crates/ifconfig-rs/src/backend/mod.rs` classifier for the set, incl. today's `unknown` placeholder for a failed IP, which stays Pass); anything else → helper("ip").
- `http.rs`: on the JSON decode error at `:201`, increment the counter via the helper("http") when the error is an unknown enum variant (serde "unknown variant"); missing checks keep their Skip default.

## Phase 2 — Incomplete results

### Groups

One group: the engine's `complete`, the summary and the cache writer are one flow through `scoring/engine.rs`, `routes.rs`, `og/handler.rs` and `badge/render.rs`.

### Plan

- `scoring/engine.rs`: `OverallScore.complete: bool`. `compute_score` marks the result incomplete when any section is Errored or Scored with `possible == 0` (NotApplicable excluded); then `grade = "incomplete"`, finished sections keep their `SectionScore`; `overall_percentage` is the weighted average of finished sections (informational). The old all-gone `"error"` grade becomes `"incomplete"` too. Doc comments at `:42`, `:64`, `:117` state this.
- `routes.rs`: `SummaryEvent.complete: bool`; `section_status_from_checks` returns `"error"` for a Scored section whose weighted checks earn nothing possible (compute from the profile, or pass the section's `possible` from `OverallScore`); `summary_payload_from` copies `complete`.
- One writer, e.g. `fn store_result(state, key, output) -> Option<CachedResult>` in `crates/lens/src/cache.rs` or `routes.rs`, used by `/api/check` (`:1126-1138`), the badge recompute (`:916-934`) and the OG recompute (`og/handler.rs:139-165`): it refuses an incomplete result (no cache insert); `/api/check` snapshots only a complete result (`snapshot_id` null otherwise). The moka `or_insert_with_if` coalescing in badge/OG must not insert an incomplete value (return it to the caller without caching).
- Badge and OG: treat `"incomplete"` like `"error"` — `is_error` (`routes.rs:941`, `og/handler.rs:174`) and `badge/render.rs:27` (`?`); the OG card renders `?` in place of the letter (find where `svg_for_grade` maps `error`).
- Review fixes: README scoring section updated in the same commit (SCORING SYNC RULE); C5 dropped (R4.3 owns all-`skip` beacon answers).

## Phase 3 — Deadlines

### Groups

One group: `check.rs`, the backends' request paths, `state.rs` and `config.rs` share the timeout values.

### Plan

- `check.rs`: `pub async fn run_check_with_deadline(state, input, hard_deadline: Duration) -> CheckOutput`; `run_check_with_input` passes `HARD_DEADLINE` (a `pub const` of 20 s, used by the config check too). Run each wave-1 backend as its own task whose result is recorded as soon as it finishes (e.g. a shared `Mutex<HashMap>` or a `JoinSet` drained under `tokio::time::timeout_at(deadline)`), so on expiry the finished sections are kept and only unfinished ones become `Err(SectionError::Timeout)`; wave 2 (ip) runs within the remaining time; the score is computed from what is present (Phase 2's `compute_score` then makes it incomplete).
- Backends (`dns.rs`, `email.rs`, `tls.rs`, `http.rs`, `ip.rs`): wrap the whole call — connect, send and body read (`collect`, `collect_until_type`, `resp.json()`) — in one `tokio::time::timeout(timeout, async { … })`; for ip, per address as today.
- `state.rs:124`: email uses `config.backends.email.timeout_ms`; `config.rs:111` comment updated.
- `config.rs` `validate`: reject `max(dns, tls, http, email timeout_ms) + ip timeout_ms >= HARD_DEADLINE` with a message naming the backend timeouts and the hard deadline (it must contain the words `timeout` and `deadline`); `http` and `email` count only when configured.
- `crates/lens/tests/fixtures/lens.production.toml`, `crates/lens/lens.dev.toml`, `crates/lens/lens.example.toml`: dns, tls, http and email `timeout_ms = 15000`, ip 2000, email set explicitly.
- Review fix: the budget sum saturates (`saturating_add`).

## Phase 4 — TLS reachability

### Groups

1. tlsight: `tls/connect.rs` (keep the `io::Error` in the boxed error), `tls/mod.rs` (`pub fn error_code`, used by `inspect_ip`), `quality/mod.rs` (`assess_port` emits `tls_reachable` first: Pass when an IP succeeded; Fail when every IP failed and at least one failure is not `NOT_TESTED_FROM_HERE`; Skip when every failure is `NOT_TESTED_FROM_HERE`; with no successful IP, the checks are `[tls_reachable]` only), the check's label and category (`Protocol`, as the golden test pins).
2. lens: `crates/lens/profiles/default.toml` `tls_reachable = 10` under `[sections.tls.checks]`, `hard_fail = ["chain_trusted", "not_expired", "tls_reachable"]`; the README scoring tables if they list TLS weights (SCORING SYNC RULE).
3. Goldens, after 1 and 2 are green: `UPDATE_GOLDEN=1 cargo test -p tlsight --test contract_golden` (tlsight-inspect.json gains `tls_reachable`; tlsight-unreachable.json and tlsight-not-tested.json are written), then `UPDATE_GOLDEN=1 cargo test -p lens --test lens_golden` (lens-http-only.json, lens-no-weighted-tls.json written; the existing lens goldens move in their TLS score). The orchestrator commits the regenerated goldens with `ADLC-Test-Change` naming requirement 7.
- Review fixes: `EHOSTUNREACH` → `HANDSHAKE_FAILED`; tlsight `qualityVerdict` all-skip → skip; `tls_reachable` texts, labels and docs.
