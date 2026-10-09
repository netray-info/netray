# Spec: grade integrity

Status: Ready for Implementation
Created: 2026-10-09

## Goal

A lens grade is never better than what lens measured. A result with an Errored section, a timed-out section, or a Scored section whose weighted checks earn nothing possible (`possible == 0`) is **incomplete**: its grade is `incomplete`, the summary says `complete: false`, and it is never cached, snapshotted, or badged or rendered as a letter. An HTTPS-less site fails TLS through a hard-fail `tls_reachable` check. Each backend call has one deadline; on the hard deadline finished sections are kept. Unknown backend verdicts and truncated or malformed SSE streams make a section Errored. HSTS and the HTTPS redirect are scored once, in HTTP. One target policy in `netray_common::target_policy` is the only blocklist.

## Non-goals

- The dot colour and any rendering of `incomplete` in the lens frontend beyond the API (V2, SDD SC16).
- Email scoring (SDD R4.1–R4.5). The `"Skipped"` spelling guard (`email.rs:133`) stays; R4.2 corrects it.
- IP enrichment failures (one IP or all) turning into reputation Pass (`crates/lens/src/backends/ip.rs:171`): SDD R5.2.
- A domain without address records as NotApplicable: incomplete in 0.23.0 (SDD SC14).
- prism's nameserver queries under the target policy: SDD R5.8, its own feature.
- Any snapshot DB migration, wipe or truncation (SDD SC15).

## Context and constraints

Line numbers at `c9a16f4`.

- Scoring: `score_section` (`crates/lens/src/scoring/engine.rs:65`) returns None for Errored/NotApplicable (`:70`) and for `possible == 0` (`:103-105`); `compute_score` (`:122`) drops a None silently (`:137-141`) and returns grade `"error"` only when nothing is left (`:145-155`). Doc comments `:42`, `:64` say "100% (full credit)", the opposite of the code. `scoring_regression.rs:571` pins an Errored email section as grade A+, `:600` a NotApplicable one.
- lens's only NotApplicable producer is the beacon guard `summary.grade == "Skipped"` (`crates/lens/src/backends/email.rs:133`); beacon serialises `skipped` (`crates/beacon/src/quality/types.rs:36`), so a real beacon timeout reaches lens as `skip` verdicts in every category, which gives `possible == 0` for email.
- Three cache writers build `CachedResult` themselves: `/api/check` (`crates/lens/src/routes.rs:1129-1138`, snapshot first at `:1126`), the badge recompute (`routes.rs:916-934`), the OG recompute (`crates/lens/src/og/handler.rs:139-165`). Badge and OG pick the short `Cache-Control` and the `?` rendering only for grade `"error"` (`routes.rs:941`, `og/handler.rs:174`, `crates/lens/src/badge/render.rs:27`). `section_status_from_checks` (`routes.rs:1175-1190`) returns `"pass"` for an `Ok` without warn/fail. `SummaryEvent` is `routes.rs:484-497`.
- Hard deadline 20 s, a constant (`crates/lens/src/check.rs:77`); on expiry every section becomes Timeout and finished results are discarded (`:83-96`). dns and email apply their timeout twice (send, then stream: `dns.rs:114-125,145-148`, `email.rs:87-90,107-110`); tls, http and ip bound only the send (`tls.rs:162-165,184`, `http.rs:179-182,201`, `ip.rs:156-158,176`). Email hard-codes 15 s (`crates/lens/src/state.rs:124`; `config.rs:111`); the default `timeout_ms` is 2000 (`config.rs:125-127`). `Config::validate` (`config.rs:440-455`) checks no timeouts; `--check-config` runs `Config::load` (`crates/netray/src/main.rs:98-100`). Configs that load lens: `lens.production.toml` fixture (dns/tls/http/email 20000, ip 2000), `crates/lens/lens.dev.toml` (dns/tls/http 20000, ip 2000, email without `timeout_ms`), `crates/lens/lens.example.toml` (dns/tls 20000, ip 2000); they are loaded by `repo_config_files_load` (`config.rs:692`), `tests/repo/test_smoke_services.sh:29` and the release smoke (`.github/workflows/release.yml:74`).
- `sse::drain` (`crates/lens/src/backends/sse.rs:26-85`): `Ok` without the terminal event (`:84`), unparseable payload dropped (`:53-54`), a second `data:` line overwrites the first (`:76-77`).
- Unknown verdicts: prism lint results → Pass (`dns.rs:356`; known: `Ok`, `Warning`, `Failed`, `NotFound`, `:343-353`), ifconfig-rs network type → Pass (`ip.rs:256`), tlsight status → Skip (`tls.rs:214,235`), beacon verdict → Skip (`email.rs:406`; beacon's `Verdict` includes `info`, `types.rs:24`). spectra: an unknown status already fails decoding (`http.rs:20,201`), so the section is Errored without a counter; a missing check defaults to Skip (`http.rs:244-270,371-376`) and stays so. Metrics use `metrics::counter!` (e.g. `routes.rs:882`).
- tls.rs copies tlsight's hostname checks `hsts` and `https_redirect` into TLS (`tls.rs:226-244`); http.rs emits the same names (`http.rs:275,285`); `contract_backends.rs:123` pins `hsts` in the TLS result.
- tlsight: `assess_port` returns `Skip` with no checks when no IP succeeded (`crates/tlsight/src/quality/mod.rs:32-38`); every connect error is `HANDSHAKE_FAILED` (`crates/tlsight/src/tls/mod.rs:89`); consistency already uses only IPs with `tls.is_some()` (`routes.rs:1042`). The tlsight contract golden is built from literals (`crates/tlsight/tests/contract_golden.rs:125`). lens profile `[sections.tls] hard_fail = ["chain_trusted", "not_expired"]` (`crates/lens/profiles/default.toml:23`).
- Blocklists: `crates/common/src/target_policy.rs` (already refuses broadcast `:53` and `fec0::/10` `:99`; lacks 0.0.0.0/8 beyond `0.0.0.0`, 240/4, 198.18/15, 192.0.0/24; judges 6to4 by its embedded IPv4 `:106-115`, pinned `:276`); `crates/common/src/ip_filter.rs` `is_blocked_ip` (`:5`; lacks broadcast and `fec0::/10`; 6to4 pinned allowed `:270`); `crates/tlsight/src/security/target_policy.rs` (refuses all of 2002::/16 `:74-77`; `check_allowed_with_policy` with the `allow_blocked` switch `:106-113`); lens's dead `crates/lens/src/security/target_policy.rs`. prism's wrapper `is_allowed_target` (`crates/mhost-prism/src/security/query_policy.rs:121`) has two callers (`:108`, `dns_trace.rs:179`); its tests pin `fec0::1` as allowed (`:301`, `:442`).
- Pinned results that move, each with `ADLC-Test-Change` naming its requirement (testing-rules): `tests/fixtures/contracts/lens-*.json`, the tlsight contract golden, `crates/tlsight/tests/port_results_table.rs` (all three rows), `crates/common/tests/target_policy_results_table.rs`, `scoring_regression.rs:571`, `contract_backends.rs:123`, `ip_filter.rs:270`, `target_policy.rs:276`, prism's `query_policy.rs` tests.
- The gate is `just adlc-verify`, offline. Status words keep one spelling: `incomplete`, `tls_reachable`, `NOT_TESTED_FROM_HERE`, `Errored` (SDD SC16).

## Requirements

1. **SSE collector (R3.4).** A stream that ends without its terminal event is an `Err`; consecutive `data:` lines of one event join with `\n`; a payload that does not parse as JSON is an `Err`. The section then is Errored.
2. **Unknown verdicts (R3.5, SC9).** dns, tls, email and ip map the values of their producer's own vocabulary explicitly (prism lint `Ok`/`Warning`/`Failed`/`NotFound`; tlsight `CheckStatus`; beacon `Verdict` including `info`, mapped as today; ifconfig-rs network types); any other value makes the section Errored. Every unknown value, and spectra's decode failure on an unknown status, increments `lens_unknown_verdict_total{section}` and logs a warning.
3. **Incomplete (R3.2, SC12, SC14).** A result is incomplete when any weighted section is Errored, Timeout, or Scored with `possible == 0`. NotApplicable stays excluded without making the result incomplete. An incomplete result has grade `incomplete`; finished sections keep their section scores and grades; the summary carries `complete` (false or true); a Scored section with `possible == 0` has status `"error"`. Phase 2 also corrects the engine's doc comments.
4. **One cache writer (R3.2, SC12).** `/api/check`, the badge recompute and the OG recompute store through one function that refuses an incomplete result; an incomplete result gets no snapshot (`snapshot_id` null). Badge and OG treat `incomplete` as they treat `error` today: the `?` value or card and the short `Cache-Control`.
5. **One deadline per backend call (R3.3, SC8).** Each backend's connect, send and body read (stream or JSON) run under one `timeout_ms`. Email honours `config.backends.email.timeout_ms`. On the hard deadline, finished sections are kept and only unfinished ones become Timeout. The hard deadline stays 20 s; the check function takes it as a parameter so a test can shorten it.
6. **Budget check (R3.3, K2).** Config load rejects `max(dns, tls, http, email timeout_ms) + ip timeout_ms ≥` the hard deadline, so `netray lens --check-config` exits 1 for it. `lens.production.toml` (fixture), `lens.dev.toml` and `lens.example.toml` set dns, tls, http and email to 15000 and ip to 2000, email included explicitly.
7. **TLS reachability (R3.1).** tlsight emits a `tls_reachable` port check: Pass when at least one IP completed the handshake; Fail when every IP failed with a target-side error (refused, reset, TLS alert, handshake timeout); Skip when the only failures are local (requirement 8). lens weights `tls_reachable` with 10 and adds it to `[sections.tls] hard_fail`.
8. **Not tested from here (R5.3).** A connect error raised locally before any packet reaches the target (`ENETUNREACH`, `EHOSTUNREACH`, `EADDRNOTAVAIL`) gets code `NOT_TESTED_FROM_HERE`; such IPs do not count for `tls_reachable` (consistency already ignores them).
9. **HSTS once (R3.6).** lens no longer copies tlsight's `hsts` and `https_redirect` into the TLS section; HTTP owns them.
10. **One blocklist (R3.7, SC4).** `netray_common::target_policy` refuses the union of today's copies: in addition all of `0.0.0.0/8`, `240.0.0.0/4`, `198.18.0.0/15`, `192.0.0.0/24` and all of `2002::/16`. `ip_filter::is_blocked_ip` delegates to it (so prism also refuses broadcast and `fec0::/10`). tlsight calls it and keeps its `allow_blocked` switch; lens's copy is deleted; prism's wrapper is renamed `check_target_ip`. A convention test in `tests/repo/` fails when a Rust source under `crates/*/src/` outside `crates/common` defines `fn is_allowed_target(`, `fn is_blocked_ip(` or `fn check_allowed(`, or calls `is_private()`/`is_loopback()` in a `security/` module.
11. **Lens golden projection (R3.8).** The lens golden projection records `complete`; fixture `no-address-records` (prism answering A and AAAA with `NxDomain`, tlsight and spectra erroring) is added.
12. **TLS lens goldens (R3.8).** Fixtures `http-only` (`tlsight-unreachable.json`, written by tlsight's contract test from a real `assess_port` result with every IP refused) and `no-weighted-tls` (a tlsight answer whose only IPs are `NOT_TESTED_FROM_HERE`, produced the same way) are added.

## Phase 1 — Errored causes

**Depends on:** none
**Requirements:** 1, 2

`crates/lens/src/backends/sse.rs`; the verdict maps in `dns.rs`, `tls.rs`, `email.rs`, `ip.rs`; spectra's decode path in `http.rs`; the counter.

### Test Scenarios

- GIVEN a stream with one event and no `done` WHEN collected THEN `Err`.
- GIVEN a stream with `done` WHEN collected THEN `Ok` with its events, as today.
- GIVEN an event with two `data:` lines WHEN collected THEN the payload is both lines joined with `\n`.
- GIVEN an event whose payload is not JSON WHEN collected THEN `Err`.
- GIVEN the prism, tlsight, beacon and ifconfig goldens each with one verdict renamed to `"passed"` WHEN lens parses them THEN that section is Errored and `lens_unknown_verdict_total{section}` increments.
- GIVEN the spectra golden with one status renamed WHEN lens parses it THEN http is Errored and the counter increments.
- GIVEN each unchanged golden WHEN lens parses it THEN the section is scored as today with no counter increment (beacon `info` included).

## Phase 2 — Incomplete results

**Depends on:** Phase 1
**Requirements:** 3, 4, 11

`crates/lens/src/scoring/engine.rs`, `crates/lens/src/routes.rs` (summary, section status, cache writer, snapshot, badge), `crates/lens/src/og/handler.rs`, `crates/lens/src/badge/render.rs`, `crates/lens/tests/lens_golden.rs`.

### Test Scenarios

- GIVEN the email stub returns 500 WHEN `POST /api/check` THEN `grade:"incomplete"`, `complete:false`, no snapshot id, and a second request is a cache MISS (`scoring_regression.rs:571` moves to `incomplete`).
- GIVEN the email stub serves a beacon answer with `skip` in every category WHEN checked THEN `grade:"incomplete"` and email status `"error"`.
- GIVEN the DNS stub answers A and AAAA with `NxDomain` and the tlsight and spectra stubs error WHEN checked THEN `grade:"incomplete"`, `complete:false`; the `no-address-records` lens golden pins it.
- GIVEN the email stub returns 500 WHEN `GET /badge/example.com.svg` first THEN the badge shows `?` with the short `Cache-Control`, and `/api/check` afterwards is a cache MISS.
- GIVEN the email stub returns 500 WHEN `GET /og/example.com.png` first THEN the card shows `?` with the short `Cache-Control`, and `/api/check` afterwards is a cache MISS.
- GIVEN every backend unreachable WHEN `GET /badge/example.com.svg` THEN `max-age=300`, as today (`badge_routes.rs:305`).
- GIVEN a scoring input with email NotApplicable and every other section scored WHEN `compute_score` runs THEN a letter grade and complete (`scoring_regression.rs:600` holds).
- GIVEN the three committed lens goldens WHEN projected THEN each gains `complete:true` and is otherwise unchanged in this phase.

## Phase 3 — Deadlines

**Depends on:** Phase 2
**Requirements:** 5, 6

`crates/lens/src/check.rs`, each backend's request path, `crates/lens/src/state.rs`, `crates/lens/src/config.rs`, the three lens configs.

### Test Scenarios

- GIVEN a hard deadline of 1 s passed to the check function, backend timeouts of 5000 ms in a directly built state, a DNS stub answering in 100 ms and an email stub that never answers WHEN checked THEN dns is scored, email is Timeout, `complete:false`, and the run ends in under 1.5 s.
- GIVEN an email stub that sends headers and then stalls the stream, `timeout_ms = 1000` WHEN checked THEN email is Timeout after at most 1.2 s (not 2 s).
- GIVEN a tlsight stub that sends headers and then stalls the body, `timeout_ms = 1000` WHEN checked THEN TLS is Timeout after at most 1.2 s.
- GIVEN email `timeout_ms = 3000` WHEN lens builds its state THEN the email client uses 3000 ms, not 15 s.
- GIVEN a lens config with 20000/2000 WHEN `netray lens --check-config` THEN exit 1 naming the budget.
- GIVEN each of the production fixture, `lens.dev.toml` and `lens.example.toml` WHEN `netray lens --check-config` THEN exit 0.

## Phase 4 — TLS reachability

**Depends on:** Phase 3
**Requirements:** 7, 8, 12

tlsight's `tls/mod.rs` (error codes), `quality/mod.rs` (`assess_port`), its contract golden; lens's profile; `crates/lens/tests/lens_golden.rs` fixtures.

### Test Scenarios

- GIVEN every IP refused WHEN `assess_port` runs THEN a `tls_reachable` Fail check.
- GIVEN IPv6 `NOT_TESTED_FROM_HERE` and IPv4 ok WHEN `assess_port` runs THEN `tls_reachable` Pass and the port verdict comes from IPv4 only.
- GIVEN every IP ok WHEN `assess_port` runs THEN `tls_reachable` Pass (the port results table's three rows move, `ADLC-Test-Change` naming requirement 7).
- GIVEN only `NOT_TESTED_FROM_HERE` failures WHEN `assess_port` runs THEN `tls_reachable` Skip.
- GIVEN each of `ENETUNREACH`, `EHOSTUNREACH`, `EADDRNOTAVAIL`, `ECONNREFUSED` WHEN mapped THEN the first three are `NOT_TESTED_FROM_HERE` and refused stays `HANDSHAKE_FAILED`.
- GIVEN a closed local port WHEN inspected THEN `HANDSHAKE_FAILED`, as today.
- GIVEN `tlsight-unreachable.json` WHEN lens scores it THEN grade `F` and `hard_fail_checks` contains `tls_reachable`; the `http-only` lens golden pins it.
- GIVEN the `no-weighted-tls` tlsight answer WHEN lens scores it THEN `grade:"incomplete"` and TLS status `"error"`.
- GIVEN the healthy tlsight contract golden regenerated with `tls_reachable` Pass WHEN the lens goldens run THEN their TLS scores, section grades and summary scores move (`ADLC-Test-Change` naming requirement 7).

## Phase 5 — HSTS owned by HTTP

**Depends on:** Phase 4
**Requirements:** 9

`crates/lens/src/backends/tls.rs`.

### Test Scenarios

- GIVEN a tlsight golden whose hostname checks contain `hsts` Fail WHEN lens parses it THEN no TLS check is named `hsts` or `https_redirect` (`contract_backends.rs:123` moves).
- GIVEN the healthy fixture WHEN checked THEN `hsts` and `https_redirect` appear once, in HTTP; the lens goldens move in the TLS check list and the TLS scores (`ADLC-Test-Change` naming requirement 9).

## Phase 6 — One blocklist

**Depends on:** none
**Requirements:** 10

`crates/common/src/target_policy.rs`, `crates/common/src/ip_filter.rs`, `crates/tlsight/src/security/target_policy.rs` and its callers, `crates/lens/src/security/`, prism's `query_policy.rs` and `dns_trace.rs`, `tests/repo/`.

### Test Scenarios

- GIVEN `0.1.2.3`, `240.0.0.1`, `198.18.0.1`, `192.0.0.8` and `2002:808:808::1` WHEN `is_allowed_target` runs THEN each is refused (the target policy results table moves, `ADLC-Test-Change` naming requirement 10).
- GIVEN `8.8.8.8`, `1.1.1.1`, `2606:4700::` WHEN `is_allowed_target` runs THEN allowed, as today.
- GIVEN `255.255.255.255` and `fec0::1` WHEN prism's `ip_filter::is_blocked_ip` runs THEN blocked (fails today).
- GIVEN tlsight's check with `allow_blocked = false` WHEN given the addresses above THEN it answers as `target_policy`; with `allow_blocked = true`, `10.0.0.1` is allowed, as today.
- GIVEN the tree WHEN the convention test runs THEN it passes; it fails on today's tree (tlsight's and lens's copies, prism's `is_allowed_target`).
- GIVEN the beacon, spectra and prism results tables WHEN run THEN green unchanged (none uses a newly refused range).

## Decision log

- One feature for SDD Phase 3 plus R5.3, over R5.3 in Phase 5: R3.1's Skip for local-only failures needs R5.3's code (operator, 2026-10-09).
- R5.8 in its own feature, landed only at the release: a security fix with neutral wording, and `adlc feature finish` pushes to the public repo (operator, 2026-10-09).
- `tls_reachable` weight 10 over 2: existing grades are no constraint; the V2 engine rethinks scoring from scratch (operator, 2026-10-09).
- Badge and OG show `?` for an incomplete result, with the short `Cache-Control` used for errors today (operator, 2026-10-09; SC12; independent reading).
- Timeouts 15000 (dns, tls, http, email) and 2000 (ip) in the production fixture, the dev and the example config: 17 s < 20 s; argus sets the same before the deploy (operator, 2026-10-09; K2/K4). Dev and example follow, because the budget check loads them in the gate and the release smoke (independent reading).
- The hard deadline stays a constant passed into the check function, over a config key: a key would add a path argus's key comparison must render, and the test needs only a seam (independent reading).
- A real beacon timeout is incomplete through `possible == 0`; the `"Skipped"` guard stays for R4.2 (independent reading).
- IP enrichment failures stay as today here; R5.2 owns them (independent reading).
- Phase 6 depends on none and may be built first.

## Open decisions

None.

## Out of scope

- Email bucket N/A, beacon's `skipped` spelling, DKIM: SDD R4.1–R4.5 (planning repo).
- IP reputation flags, sampling and enrichment errors: SDD R5.2.
- Frontend rendering: V2.
