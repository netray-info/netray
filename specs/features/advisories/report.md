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
