# Spec: advisories

Status: Ready for Implementation
Created: 2026-10-09

## Goal

`deny.toml` ignores exactly RUSTSEC-2026-0206, RUSTSEC-2026-0192 and RUSTSEC-2024-0436, each under a comment that states the dependency path, the caller inputs that reach it and their bounds, and tests pin those bounds. `cargo deny check advisories` passes with no `advisory-not-detected` warning. mhost is 0.12.0, and `Cargo.lock` holds no `hickory-proto` or `hickory-net` below 0.26.1 and no `rustls-pemfile`. prism's raw DNS queries encode and decode as today on hickory-proto 0.26, and prism builds a resolver group from caller-supplied nameservers with mhost's non-global refusal on.

## Non-goals

- No change to the advisory schedule: advisories stay out of `just check` and the gate (`justfile` `deny` recipe, workflow-rules R-J6); the daily watch on argus runs them.
- No severity or wording change of any DNS lint beyond what mhost 0.12.0 brings.
- No mhost 0.12.1 (`NxDomain::response_code`): it comes after 0.23.0.
- No `@system` policy change: SC17/R5.9 own it.

## Context and constraints

- `deny.toml:21-47` ignores nine IDs (RUSTSEC-2025-0134, -2024-0436, -2023-0071, -2026-0118, -2026-0119, -2026-0009, -2026-0253, -2026-0206, -2026-0192); comments for RUSTSEC-2025-0134 ("transitive"), RUSTSEC-2023-0071 (not in the tree) and RUSTSEC-2026-0118 ("no fixed release") are wrong (SDD security-correctness §1, R2.3, R2.4, R2.6).
- `time` is 0.3.45 in `Cargo.lock`; RUSTSEC-2026-0009 is fixed in 0.3.47, which requires Rust 1.88. The workspace resolver is MSRV-aware (`resolver = "3"`), so `cargo update -p time` stays at 0.3.45 while `rust-version` is 1.85 (independent reading, 2026-10-09).
- `lru = "0.16"` at `crates/ifconfig-rs/Cargo.toml:27`, used in `crates/ifconfig-rs/src/backend/mod.rs:23,33,39`; RUSTSEC-2026-0253 is fixed in 0.18.2.
- `rustls-pemfile = "2"` at `crates/tlsight/Cargo.toml:11`, used once, `crates/tlsight/src/state.rs:114`.
- The OG label check is at `crates/lens/src/og/handler.rs:80-92`: printable ASCII 0x20–0x7E, at most `MAX_LABEL_LEN` (32) bytes.
- mhost is `0.11` in the workspace (`Cargo.toml:24`); prism also depends on `hickory-proto = "0.25"` directly (`crates/mhost-prism/Cargo.toml:24`). `crates/mhost-prism/src/api/authcompare.rs:373` converts an mhost `RecordType` with `hickory_proto::rr::RecordType::from(rt)`.
- mhost 0.12.0 is on crates.io (released 2026-10-08): hickory 0.26.3, Rust 1.88, `mhost::Name`/`IntoName` its own types, `ResolverGroupBuilder::deny_non_global(bool)` with `build()` returning `Error::NameServerNotGlobal`, DoT/DoH resolve, no `/etc/hosts` answers, the dnssec lint no longer warns "DNSKEY present but no RRSIG", `check_ttl` collapses duplicate records (SDD Phase 2, SC11).
- `crates/common/Cargo.toml:11` declares `rust-version = "1.85"`; the toolchain is stable (`rust-toolchain.toml`).
- prism builds its resolver group in `build_resolver_group` (`crates/mhost-prism/src/api/query.rs:762`): predefined providers, `ServerSpec::System` (`:788`) and caller IPs (`ServerSpec::Ip`, `:792`). Caller IPs are checked first by `is_allowed_target` (`crates/mhost-prism/src/security/query_policy.rs:121`).
- hickory-proto 0.26 changes the `Message` API prism's raw queries use: `Message::new()` takes id, type and op code; `answers`, `authorities`, `additionals` and `edns` are fields; header access goes through `Metadata` (`hickory-proto-0.26.3/src/op/message.rs:150`; prism `crates/mhost-prism/src/dns_raw.rs:83-103,213-217`, tests `:418,:427`). The SDD's "exactly one line changes" covered the mhost bump alone.
- Docker builds with `clux/muslrust:stable` (`Dockerfile:8`) and CI with stable, so Rust 1.88 is available everywhere.
- Pinned results that 2b may move: `crates/mhost-prism/tests/lint_results_table.rs` (built on mhost 0.11.3's serde form) and the lens goldens `tests/fixtures/contracts/lens-*.json`. A moved row carries `ADLC-Test-Change` naming requirement 7 (testing-rules).
- The gate is `just adlc-verify`, offline; repository checks are `tests/repo/test_*.sh`, one test per file.

## Requirements

1. `rust-version` is 1.88 in `crates/common/Cargo.toml`; `time` is ≥ 0.3.47 in `Cargo.lock`, and RUSTSEC-2026-0009 is not ignored.
2. ifconfig-rs uses `lru` 0.18 (≥ 0.18.2), and RUSTSEC-2026-0253 is not ignored. The DNS cache keeps its capacity and behaviour.
3. RUSTSEC-2023-0071 and its comment are removed from `deny.toml`.
4. tlsight parses PEM with `rustls-pki-types` (`pem::PemObject`); `rustls-pemfile` is not a dependency of any crate, and RUSTSEC-2025-0134 is not ignored. Custom-CA loading accepts and rejects the same files as today.
5. The ignores of RUSTSEC-2026-0206, RUSTSEC-2026-0192 and RUSTSEC-2024-0436 stay, each comment stating its dependency path and reachability: rustybuzz/ttf-parser reach only the OG renderer, fed by the domain (validated by lens's input parser) and the OG `label` (printable ASCII 0x20–0x7E, at most 32 bytes), with bundled fonts; `paste` is a build-time proc-macro under utoipa-axum. lens tests pin the label bounds: 33 bytes → 400, a non-ASCII byte → 400, 32 printable bytes → 200.
6. Until requirement 7, the hickory ignores' comment states the true status: fixed in hickory ≥ 0.26.1, waiting for mhost 0.12.0, with the reachability argument of SDD R2.6.
7. The workspace depends on mhost `=0.12.0`; `authcompare.rs` converts the record type through `u16`; prism's direct `hickory-proto` is ≥ 0.26.1; no `hickory-proto` or `hickory-net` below 0.26.1 is in the tree; RUSTSEC-2026-0118 and RUSTSEC-2026-0119 are not ignored, and RUSTSEC-2026-0120 is never added.
8. `build_resolver_group` sets `deny_non_global(true)` when the group holds no `ServerSpec::System`, and maps `NameServerNotGlobal` to prism's existing blocked-target error. A group with `@system` builds as today.
9. A repository check `tests/repo/test_advisories.sh` pins the advisory state offline from `deny.toml` and `Cargo.lock`: the ignore list is exactly the three IDs of requirement 5 plus RUSTSEC-2026-0118 and RUSTSEC-2026-0119, each preceded by a comment naming its dependency path; `Cargo.lock` has no package `rustls-pemfile`; `time` in `Cargo.lock` is ≥ 0.3.47.
10. After requirement 7 the repository check allows only the three IDs of requirement 5, and every `hickory-proto` and `hickory-net` package in `Cargo.lock` is ≥ 0.26.1.
11. prism's raw DNS queries (`dns_raw.rs`) use hickory-proto 0.26's API with today's wire behaviour: a query for a fixed id, name, type, RD and DO bit encodes to the same bytes as on 0.25, and a recorded response decodes to the same answer, authority and additional records and response code.
12. lens renders OG images with resvg/usvg 0.48 and fontdb 0.24, so neither `rustybuzz` nor `ttf-parser` is in `Cargo.lock`; RUSTSEC-2026-0206 and RUSTSEC-2026-0192 are not ignored, and the repository check allows only RUSTSEC-2024-0436. OG images keep their size (1200×630), the label bounds of requirement 5 and the bundled fonts.

## Phase 1 — Fixable advisories

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 9

`crates/common/Cargo.toml` (`rust-version` 1.88), `Cargo.lock` (time), `crates/ifconfig-rs` (lru 0.18), `crates/tlsight/src/state.rs` (PEM through `rustls-pki-types`), `deny.toml` (removals and true comments), lens label-bound tests, `tests/repo/test_advisories.sh`.

### Test Scenarios

- GIVEN the tree WHEN the repository check runs THEN the ignore list is the three requirement-5 IDs plus RUSTSEC-2026-0118/-0119, each with a comment naming its path; it fails today (nine IDs).
- GIVEN `Cargo.lock` WHEN the repository check reads it THEN no package `rustls-pemfile` exists and `time` is ≥ 0.3.47 (fails today).
- GIVEN a `deny.toml` with an ignore lacking a path comment WHEN the check runs on it THEN it fails (the check's own fixture).
- GIVEN a PEM bundle with two certificates WHEN tlsight loads it as custom CA THEN both are loaded, as today.
- GIVEN a file with no PEM certificate WHEN tlsight loads it THEN it is refused as today.
- GIVEN the ifconfig-rs DNS cache with capacity N WHEN N+1 distinct addresses are inserted THEN the least recently used one is evicted, as today.
- GIVEN `GET /og/example.com.png?label=` of 33 printable bytes WHEN served THEN 400.
- GIVEN a label containing `é` WHEN served THEN 400.
- GIVEN a label of 32 printable bytes WHEN served THEN 200.

## Phase 2 — mhost 0.12.0

**Depends on:** Phase 1
**Requirements:** 7, 8, 10, 11

Workspace and prism manifests, `Cargo.lock`, `authcompare.rs:373`, `dns_raw.rs` (hickory-proto 0.26 `Message` API), `build_resolver_group`, `deny.toml`, the repository check's hickory rows, and the pinned rows mhost 0.12.0 moves.

### Test Scenarios

- GIVEN the tree WHEN the repository check runs THEN the ignore list is exactly the three requirement-5 IDs and every `hickory-proto`/`hickory-net` in `Cargo.lock` is ≥ 0.26.1 (fails at Phase 1's end).
- GIVEN a query for `example.com` DNSKEY with id 0x1234, RD off and DO on WHEN prism encodes it THEN the bytes equal those recorded on hickory-proto 0.25 (pinning; green before and after).
- GIVEN a recorded referral response with answer, authority and additional records WHEN prism decodes it THEN sections and response code equal today's (pinning).
- GIVEN a query with server `127.0.0.1` or `10.0.0.1` WHEN prism's `build_resolver_group` builds it with the address check bypassed THEN it returns the blocked-target error from `NameServerNotGlobal` (fails today: it builds).
- GIVEN a query with `@system` WHEN `build_resolver_group` builds it THEN it builds, as today.
- GIVEN a query with a predefined public provider WHEN `build_resolver_group` builds it THEN it builds, as today.
- GIVEN the lint results table WHEN it runs on mhost 0.12.0 THEN the signed-zone row no longer carries "DNSKEY present but no RRSIG", and every moved row carries `ADLC-Test-Change` naming requirement 7.
- GIVEN the lens goldens WHEN they run on mhost 0.12.0 THEN they are green unchanged, or each moved row carries `ADLC-Test-Change` naming requirement 7.

## Phase 3 — OG renderer on resvg 0.48

**Depends on:** Phase 2
**Requirements:** 12

`crates/lens/Cargo.toml` (resvg 0.48, fontdb 0.24), the OG renderer and font loading in lens where the API moved, `Cargo.lock`, `deny.toml`, the repository check's expected set.

### Test Scenarios

- GIVEN the tree WHEN the repository check runs THEN the ignore list is exactly RUSTSEC-2024-0436 and `Cargo.lock` has no `rustybuzz` or `ttf-parser` (fails at Phase 2's end).
- GIVEN a grade and a domain WHEN lens renders the OG image THEN it is a valid 1200×630 PNG, as today.
- GIVEN the label bounds tests WHEN they run THEN 33 bytes and non-ASCII give 400 and 32 printable bytes give 200, unchanged.

## Decision log

- resvg 0.48 and fontdb 0.24 in this feature (Phase 3, requirement 12) over keeping the two unmaintained ignores: 0.48 replaces rustybuzz/ttf-parser with harfrust/skrifa, and SC10 fixes what has a fix (Phase 1 reader, operator, 2026-10-09).

- `rust-version` 1.88 in Phase 1 over Phase 2 (SDD puts it with mhost): `time` 0.3.47 requires 1.88 and the resolver is MSRV-aware (independent reading, 2026-10-09).
- Advisory state read from `Cargo.lock` over `cargo tree -i`: `cargo tree -i <absent>` exits 101, so an absence check on its output cannot tell absent from broken (independent reading, 2026-10-09).
- prism's `dns_raw` port to hickory-proto 0.26 in this feature, pinned by encode/decode tests written before the bump: the SDD's one-line estimate missed prism's direct `Message` use; AMENDMENT for the planning session.
- One feature for SDD Phases 2a and 2b, two phases, over two features: 2b depends on 2a's `deny.toml` and repository check, and both ship in 0.23.0 (operator, 2026-10-09: "nicht anhalten, machen").
- mhost `=0.12.0` over `0.12`: a caret requirement resolves to 0.12.1, which comes after 0.23.0 (SDD SC11, project memory 2026-10-09).
- Advisories checked offline by the ignore list and `cargo tree`, over running `cargo deny check advisories` in the gate: the advisory DB needs the network and the gate is offline; advisories run on argus's daily watch (justfile `deny`, R-J6). `cargo deny check advisories` runs once as the behavioural verification of each phase.
- DoT/DoH resolving on mhost 0.12 is checked as a behavioural verification against a public resolver, over a unit test: a local TLS resolver would need a certificate mhost's root store trusts (SDD R2.6 acceptance names "recorded or local"; neither works offline).
- `deny_non_global(true)` only for groups without `@system`, over all groups: Docker's embedded resolver is not global, so `@system` would fail (SDD R2.6).

## Open decisions

None.

## Out of scope

- The rename of prism's `is_allowed_target` to `check_target_ip`: SDD R3.7, feature for Phase 3.
- prism's nameserver queries under the target policy: SDD R5.8.
- `@system` in the UI: SDD R5.9.
