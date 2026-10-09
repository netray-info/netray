# Report: grade integrity

## Phase 1 — Errored causes

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: stream without terminal event → `Err`; multi-line `data:` joined with `\n`; unparseable payload → `Err`; section Errored | green | crates/lens/src/backends/sse.rs |
| C2 | R2: dns, tls, email, ip map their producer's vocabulary explicitly; unknown → Errored; counter `lens_unknown_verdict_total{section}` + warn, also for spectra's decode failure | green | crates/lens/tests/unknown_verdicts.rs |
| C3 | one event, no `done` → `Err` | green | crates/lens/src/backends/sse.rs |
| C4 | stream with `done` → `Ok`, as today | already_implemented | crates/lens/src/backends/sse.rs |
| C5 | two `data:` lines → joined with `\n` | green | crates/lens/src/backends/sse.rs |
| C6 | non-JSON payload → `Err` | green | crates/lens/src/backends/sse.rs |
| C7 | prism, tlsight, beacon, ifconfig goldens with a verdict renamed `"passed"` → section Errored, counter +1 | green | crates/lens/tests/unknown_verdicts.rs |
| C8 | spectra golden with a status renamed → http Errored, counter +1 | green | crates/lens/tests/unknown_verdicts.rs |
| C9 | unchanged goldens → scored as today, no counter (beacon `info` included) | already_implemented | crates/lens/tests/unknown_verdicts.rs |

C4 and C9 passed at the baseline (`6e1721b`) and pin today's behaviour; the others failed there.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| SSE collector + unknown verdicts | 2 | sonnet | 59904 | 151 |

### Review

- No BLOCKER. The reader checked every value each producer emits against lens's new maps: prism lint (`Ok`, `Warning`, `Failed`, `NotFound`), tlsight `CheckStatus` (pass, warn, fail, skip), beacon `Verdict` (skip, pass, info, warn, fail) in the summary map, category events and sub-checks, ifconfig-rs network types (internal, c2, bot, cloud, vpn, tor, spamhaus, datacenter, residential), and the terminal events (prism `done`, beacon `summary`); no successful stream ends without its terminal event; heartbeats and CRLF are handled.
- AMENDMENT | plan: `map_buckets` stays infallible because `email.rs`'s test module calls it; validation sits in `parse_summary` and also covers category events and sub-checks | affected_phase: 1 | repaired_in_phase: yes
- NIT → acted on as test strength: c5 now also proves the separator is a newline; `unknown_verdicts.rs` gains rows for a tlsight port check (`chain_trusted`) and beacon's scored summary map (`spf`). Without them, reverting the port-check or summary guard stayed green, and Phase 5 would have left the TLS row without a hostname check to rename.
- DEFERRED | the `"Skipped"` guard (`email.rs:133`) never matches beacon's `skipped`; spec non-goal, R4.2.

### Behavioural verification

skipped: no entry point changes in this phase; the backends are driven by the contract-style tests against the real goldens.
