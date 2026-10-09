# Report: advisories

## Phase 1 — Fixable advisories

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R1: `rust-version` 1.88; `time` ≥ 0.3.47; RUSTSEC-2026-0009 not ignored | green | tests/repo/test_advisories.sh |
| C2 | R2: ifconfig-rs on `lru` 0.18; RUSTSEC-2026-0253 not ignored; cache capacity and behaviour unchanged | green | tests/repo/test_advisories.sh, crates/ifconfig-rs (cache test) |
| C3 | R3: RUSTSEC-2023-0071 and its comment removed | green | tests/repo/test_advisories.sh |
| C4 | R4: tlsight PEM via `rustls-pki-types`; no `rustls-pemfile`; RUSTSEC-2025-0134 not ignored; same accept/reject | green | tests/repo/test_advisories.sh, crates/tlsight (custom CA test) |
| C5 | R5: three unmaintained ignores stay with true path/reachability comments; label bounds pinned | green | tests/repo/test_advisories.sh, crates/lens (OG label test) |
| C6 | R6: hickory ignores' comment states the true fix status and reachability | green | tests/repo/test_advisories.sh |
| C7 | R9: repository check pins ignore list (3 + hickory 2), path comments, no `rustls-pemfile`, `time` ≥ 0.3.47 | green | tests/repo/test_advisories.sh |
| C8 | ignore list = three R5 IDs + RUSTSEC-2026-0118/-0119, each with path comment; fails today | green | tests/repo/test_advisories.sh |
| C9 | `Cargo.lock` has no `rustls-pemfile`, `time` ≥ 0.3.47; fails today | green | tests/repo/test_advisories.sh |
| C10 | an ignore without a path comment makes the check fail (fixture) | green | tests/repo/test_advisories.sh |
| C11 | a PEM bundle of two certificates loads both, as today | already_implemented | crates/tlsight |
| C12 | a file with no PEM certificate is refused, as today | already_implemented | crates/tlsight |
| C13 | DNS cache of capacity N evicts the least recently used on N+1, as today | already_implemented | crates/ifconfig-rs |
| C14 | OG label of 33 printable bytes → 400 | already_implemented | crates/lens |
| C15 | OG label containing `é` → 400 | already_implemented | crates/lens |
| C16 | OG label of 32 printable bytes → 200 | already_implemented | crates/lens |

C11–C16 are pinning tests, green at the baseline (`519ae54`); C1–C10 failed there through `tests/repo/test_advisories.sh`.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| fixable advisories | 2 | sonnet | 69078 | 265 |

### Review

- BLOCKER (fixed, second pass by the orchestrator) | `deny.toml:29` said the OG text is never shaped; usvg's `text` feature runs rustybuzz over the domain and label on every OG render (`crates/lens/src/og/render.rs:153`, `crates/lens/Cargo.toml:69`). The comment now says rustybuzz shapes the rendered text and names the bounded inputs.
- AMENDMENT | spec requirement 5 and SDD R2.5 keep RUSTSEC-2026-0206/-0192 as "no fix"; resvg/usvg 0.48 (MSRV 1.85) replaced rustybuzz/ttf-parser with harfrust/skrifa, and fontdb 0.24 no longer needs ttf-parser, so SC10 asks for the fix | affected_phase: 1 | repaired_in_phase: no → operator decided 2026-10-09: bump in this feature, new Phase 3 (requirement 12). For the planning session: SDD R2.5 is wrong on "no fix".
- AMENDMENT | plan gap: `rust-version` 1.88 enables let-chains, so `clippy::collapsible_if` fired once (`crates/common/src/backend.rs:232`), collapsed behaviour-neutrally | affected_phase: 1 | repaired_in_phase: yes
- Sound per the reader: PEM loading accepts and rejects the same input as `rustls_pemfile::certs` (same pki-types state machine; non-certificate sections skipped, malformed ones logged and skipped); lru 0.18.5 API and capacity unchanged; `rsa` stays a lock-only entry outside the resolved graph; the paste and hickory comments hold.

### Behavioural verification

`cargo deny check advisories` → `advisories ok`, no `advisory-not-detected` warning. `bash tests/repo/test_advisories.sh` → `PASS: advisories ignore list, lockfile and rust-version`.

## Phase 2 — mhost 0.12.0

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | R7: mhost `=0.12.0`; `authcompare.rs` via `u16`; prism `hickory-proto` ≥ 0.26.1; no hickory < 0.26.1; RUSTSEC-2026-0118/-0119 not ignored, -0120 never added | green | tests/repo/test_advisories.sh |
| C2 | R8: `build_resolver_group` sets `deny_non_global(true)` without `@system`, maps `NameServerNotGlobal` to the blocked-target error | green | crates/mhost-prism (query tests) |
| C3 | R10: repository check allows only the three R5 IDs; every hickory package in `Cargo.lock` ≥ 0.26.1 | green | tests/repo/test_advisories.sh |
| C4 | R11: `dns_raw` on hickory-proto 0.26 with today's wire behaviour | green | crates/mhost-prism (dns_raw tests) |
| C5 | ignore list exactly the three R5 IDs, hickory ≥ 0.26.1 in `Cargo.lock` (fails at Phase 1's end) | green | tests/repo/test_advisories.sh |
| C6 | query for `example.com` DNSKEY, id 0x1234, RD off, DO on encodes to the bytes recorded on 0.25 (pinning) | already_implemented | crates/mhost-prism (dns_raw tests) |
| C7 | recorded referral response decodes to today's sections and rcode (pinning) | already_implemented | crates/mhost-prism (dns_raw tests) |
| C8 | server `127.0.0.1` or `10.0.0.1`, address check bypassed → blocked-target error from `NameServerNotGlobal` (fails today) | green | crates/mhost-prism (query tests) |
| C9 | `@system` builds, as today | already_implemented | crates/mhost-prism (query tests) |
| C10 | a predefined public provider builds, as today | already_implemented | crates/mhost-prism (query tests) |
| C11 | lint results table on mhost 0.12.0: signed-zone row without "DNSKEY present but no RRSIG"; moved rows carry `ADLC-Test-Change` naming R7 | green | crates/mhost-prism/tests/lint_results_table.rs |
| C12 | lens goldens green unchanged on mhost 0.12.0, or moved rows carry `ADLC-Test-Change` naming R7 | already_implemented | crates/lens/tests/lens_golden.rs |

C6, C7, C9, C10 are pinning tests, green at the baseline (`9313ff4`) and unchanged after the bump; C12 holds because the lens goldens stayed byte-identical on mhost 0.12.0. C1–C5 and C8 failed at the baseline. C11 moved its rows (below).

Pinned rows moved (`ADLC-Test-Change`, requirement 7): `crates/mhost-prism/tests/lint_results_table.rs` — the signed-zone rows lose "DNSKEY present but no RRSIG records found"; `check_ttl` lists a record answered by two nameservers once. The DNSSEC lint still counts KSK/ZSK twice over two nameservers (R5.1). Two existing `dns_raw.rs` test bodies (`build_query_sets_rd_false`, `build_query_with_dnssec_ok_sets_do_bit`) read the hickory-proto 0.26 fields instead of the removed accessors; same assertions.

### Runs

| Group | Coder runs | Green by | Tokens | Seconds |
|---|---|---|---|---|
| mhost 0.12.0 | 1 | sonnet (rest by the orchestrator: test-owned fmt, field ports, row moves) | 67486 | 244 |

### Review

- AMENDMENT (fixed, second pass) | `crates/mhost-prism/src/api/compare.rs:130` dropped the new `BlockedTargetIp` from `build_resolver_group` during its transport probe: with `allow_arbitrary_servers = true`, `@198.18.0.1` gave 500 `RESOLVER_ERROR` instead of 422 `BLOCKED_TARGET_IP` | affected_phase: 2 | repaired_in_phase: yes (test `compare_post_with_non_global_server_returns_blocked_target_ip`, `964d19a`; the probe now returns the refusal). Production keeps arbitrary servers off.
- DEFERRED | mhost 0.12.0 no longer lowercases CAA tags (`mhost-0.12.0/src/resources/rdata/caa.rs:42`; hickory 0.26 keeps the wire case), while its CAA lint compares case-sensitively: a zone publishing `CAA 0 ISSUE "…"` now gets "Unknown CAA tag(s): ISSUE", and lens's `caa` moves Pass → Warn. RFC 8659 tags are case-insensitive. The fix belongs in mhost; no pinned row covers it. For the operator and the mhost-security session.
- DEFERRED | hickory-proto 0.26 rejects a whole message with any non-OPT record of zero-length RDATA (`InvalidEmptyRecord`, `message.rs:431`); 0.25 decoded it. prism's raw paths (trace, DNSSEC walk, authcompare, NS/SOA checks) now report that server as a decode error.
- DEFERRED | with `@system` in the same group, mhost's non-global refusal is off for caller IPs too (spec decision, R2.6); `NameServerConfig::is_global()` could check the non-system configs alone. Production keeps arbitrary servers off.
- NIT | `BlockedTargetIp.ip` carries mhost's nameserver display string (`udp:198.18.0.1:53`), and the reason is always "blocked address range", also for port 0.
- Sound per the reader: query bytes identical on 0.25 and 0.26 (RD 0, DO 1, payload 4096); section, truncation, rcode (with EDNS extension) and TCP fallback unchanged; RRSIG fields read the same wire values; the `u16` record-type round trip is lossless; no predefined provider is refused.

### Behavioural verification

`netray dns` on a local copy of `prism.production.toml`:
- `GET /api/query?q=example.com A @cloudflare +tls&stream=false` → resolves (`tls:1.1.1.1:853`, two A records); failed on mhost 0.11 (no root store).
- `… @cloudflare +https` → resolves (`https:1.1.1.1:443`, two A records).
- `… @127.0.0.1` → `ARBITRARY_SERVERS_DISABLED`, the first layer as before.

`cargo deny check advisories` → `advisories ok`. `Cargo.lock`: hickory-proto, hickory-net, hickory-resolver all 0.26.3.
