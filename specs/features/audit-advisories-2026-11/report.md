# Report: audit-advisories-2026-11

## Phase 1 — Cargo advisories

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: Cargo.lock holds rustls ≥ 0.23.46, rustls-webpki ≥ 0.103.16, webpki-roots ≥ 1.0.10, webpki-root-certs ≥ 1.0.10, tokio ≥ 1.53.3, rustix ≥ 1.1.6 via cargo update -p only; rust-version stays 1.88 | green | tests/repo/test_advisories.sh |
| C2 | R2: deny.toml ignores exactly RUSTSEC-2024-0436, -2026-0459, -2026-0374, -2024-0449, each under its own comment block naming path and reachability | green | tests/repo/test_advisories.sh |
| C3 | R3: test_advisories.sh expects the four IDs and requires keywords sqlx (0459), askama (0374), uaparser and serde_yaml (0449) | green | tests/repo/test_advisories.sh |
| C4 | R4: test_advisories.sh fails on any package of R1 below its floor or absent; existing checks unchanged | green | tests/repo/test_advisories.sh |
| C5 | R5: cargo deny check advisories exits 0 with no advisory-not-detected warning | test-unwritable | — |
| C6 | Scenario: test_advisories.sh fails today on the ignore list and the floors | green | tests/repo/test_advisories.sh |
| C7 | Scenario: a Cargo.lock copy with rustls 0.23.45 fails the floor check naming rustls 0.23.45 | green | tests/repo/test_advisories.sh |
| C8 | Scenario: a Cargo.lock copy meeting every floor passes the floor check | green | tests/repo/test_advisories.sh |
| C9 | Scenario: a Cargo.lock copy with no rustix fails naming rustix | green | tests/repo/test_advisories.sh |
| C10 | Scenario: deny-missing-comment.toml with the four expected IDs still reports RUSTSEC-2024-0436 has no comment | green | tests/repo/test_advisories.sh |
| C11 | Scenario: a deny.toml whose RUSTSEC-2026-0459 comment lacks sqlx fails naming RUSTSEC-2026-0459 | green | tests/repo/test_advisories.sh |
| C12 | Scenario: cargo deny check advisories exits 0 on the updated tree | test-unwritable | — |
| C13 | Scenario: netray serve --check-config crates/netray/netray.example.toml exits 0 from a fresh build | already_implemented | tests/repo/test_check_config.sh |
| C14 | Scenario: rust-version still 1.88 and no manifest dependency line changed | already_implemented | tests/repo/test_advisories.sh |

Notes: C5 and C12 need the network advisory database; they are behavioural checks (below). C13 is held by `tests/repo/test_check_config.sh`, which runs `netray serve --check-config` on `crates/netray/netray.example.toml`. C14's rust-version line is held by the existing check in `test_advisories.sh`; no manifest is in the phase diff.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 15963 | 100 |
| G2 | 1 | sonnet | 19479 | 15 |
| G3 | 0 | behavioural, no file | 0 | 0 |

### Reader

specs/features/audit-advisories-2026-11/reader.p1.md: 0 blockers, 0 amendments, 0 deferred, 0 nits.

### Behavioural verification

```
$ cargo deny check advisories
advisories ok          (exit 0, no advisory-not-detected)
$ bash tests/repo/test_advisories.sh
PASS: advisories ignore list, lockfile and rust-version
$ bash tests/repo/test_check_config.sh   # netray serve --check-config crates/netray/netray.example.toml from a fresh build
PASS: test_check_config
```
