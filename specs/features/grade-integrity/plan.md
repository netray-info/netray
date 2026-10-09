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
