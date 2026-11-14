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

## Phase 2 — npm advisories

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: islands package.json declares happy-dom ^21.0.4 and vitest ^4.1.12; test_islands_package.sh requires happy-dom ^21; package-lock resolves at or above; npm audit 0; npm test passes | green | tests/repo/test_islands_package.sh |
| C2 | R7: test_advisories.sh fails when package-lock.json resolves happy-dom below 21.0.4 or vitest or @vitest/mocker below 4.1.12, or one is absent; reads node_modules/<name> keys with node -e | green | tests/repo/test_advisories.sh |
| C3 | Scenario: test_advisories.sh fails today naming happy-dom 20.14.5 and vitest 4.1.11 | green | tests/repo/test_advisories.sh |
| C4 | Scenario: a package-lock copy with happy-dom 21.0.4, vitest 4.1.12, @vitest/mocker 4.1.12 passes the npm floor check | green | tests/repo/test_advisories.sh |
| C5 | Scenario: a package-lock copy with no @vitest/mocker fails naming @vitest/mocker | green | tests/repo/test_advisories.sh |
| C6 | Scenario: full npm audit at the root reports 0 vulnerabilities | test-unwritable | — |
| C7 | Scenario: the islands npm test passes | already_implemented | crates/web/islands/test |
| C8 | Scenario: test_islands_package.sh passes with ^21.0.4 and a package.json copy with ^20.14.5 fails it | green | tests/repo/test_islands_package.sh |
| C9 | Scenario: after adlc-setup a fresh netray build runs serve --check-config on the example config with exit 0 | already_implemented | tests/repo/test_check_config.sh |
| C10 | Scenario: just adlc-verify is verified | test-unwritable | — |

Notes: C6 needs the npm registry and C10 is the last phase's `adlc verify`; both are behavioural (below). C7 is held by the existing islands suite (46 files, 266 tests) that the gate runs. C9 is held by `tests/repo/test_check_config.sh`.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 21180 | 24 |
| G2 | 0 | behavioural, no file | 0 | 0 |

### Reader

none: review reads this range

### Behavioural verification

```
$ npm audit
found 0 vulnerabilities
$ npm test --workspaces --if-present
Tests  266 passed (266)   (exit 0)
$ just adlc-setup && cargo build -p netray && bash tests/repo/test_check_config.sh
dist/islands.js  33.1kb ... PASS: test_check_config
$ bash tests/repo/test_islands_package.sh
PASS: islands package and its dist check
```
