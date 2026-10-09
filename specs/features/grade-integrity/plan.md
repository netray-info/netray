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
