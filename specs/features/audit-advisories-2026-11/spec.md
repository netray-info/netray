# Spec: audit-advisories-2026-11

Status: Ready for Implementation
Created: 2026-11-14

## Goal

`cargo deny check advisories` passes with no `advisory-not-detected` warning, and full `npm audit` in the islands workspace reports 0 vulnerabilities. `Cargo.lock` holds rustls ≥ 0.23.46, rustls-webpki ≥ 0.103.16, webpki-roots ≥ 1.0.10, webpki-root-certs ≥ 1.0.10, tokio ≥ 1.53.3 and rustix ≥ 1.1.6. The islands use happy-dom ≥ 21.0.4 and vitest ≥ 4.1.12. `deny.toml` ignores exactly RUSTSEC-2024-0436, RUSTSEC-2026-0459, RUSTSEC-2026-0374 and RUSTSEC-2024-0449. Each ignore sits under a comment naming its dependency path and why it does not reach a caller. `tests/repo/test_advisories.sh` pins the ignore set, the comment keywords and the lockfile floors offline; the clean `cargo deny` and `npm audit` results are behavioural checks on the day of implementation.

## Non-goals

- No advisory scan in `just check`, the gate or CI: workflow-rules R-J6 keeps advisory scans off the push and PR path.
- No `npm audit` in the daily advisory watch on argus. That watch belongs to the private ops SDD in netray.info.
- No change for RUSTSEC-2023-0071 (rsa). It is a `Cargo.lock` entry reached only through sqlx-mysql, which is not in the resolved graph. cargo-audit reports it and cargo-deny, the tool of record, does not.
- No move to sqlx 0.9. It needs Rust 1.94 and still caps libsqlite3-sys below the fixed 0.39 (`cargo info sqlx@0.9.0`, 2026-11-14).
- No release, tag or version bump.
- The keyword arms of `test_advisories.sh` for IDs that are no longer expected (RUSTSEC-2026-0206, -0192, -0118, -0119) stay. They predate this feature.

## Context and constraints

- On 2026-11-14, `cargo deny check advisories` (cargo-deny 0.20.2, advisory-db 2026-11-13) reported 11 errors:
  - rustls 0.23.45: RUSTSEC-2026-0346 through -0349, fixed in ≥ 0.23.46.
  - rustls-webpki 0.103.15: RUSTSEC-2026-0382, fixed in ≥ 0.103.16.
  - webpki-roots 1.0.9: RUSTSEC-2026-0381, fixed in ≥ 1.0.10.
  - webpki-root-certs 1.0.9: RUSTSEC-2026-0383, fixed in ≥ 1.0.10.
  - tokio 1.53.2: RUSTSEC-2026-0386, fixed in ≥ 1.53.3.
  - libsqlite3-sys 0.30.1: RUSTSEC-2026-0459, fixed in ≥ 0.39.0.
  - basic-toml 0.1.10: RUSTSEC-2026-0374, unmaintained.
  - unsafe-libyaml 0.2.11: RUSTSEC-2024-0449, unmaintained.
- cargo-audit additionally reports rustix 1.1.5 (RUSTSEC-2026-0486, fixed in ≥ 1.1.6). rustix is dev-only, via tempfile and insta.
- `cargo update --dry-run -p rustls -p rustls-webpki -p tokio -p webpki-roots -p webpki-root-certs -p rustix` resolves all six within `rust-version = "1.88"` (`crates/common/Cargo.toml:11`). The resulting versions are 0.23.46, 0.103.17, 1.54.1, 1.0.12, 1.0.12 and 1.1.6.
- libsqlite3-sys comes in through sqlx-sqlite 0.8.6 and sqlx (`Cargo.toml:75`). sqlx-sqlite 0.9.0 still requires `>=0.30.1, <0.38.0`, and sqlx 0.9.0 declares rust-version 1.94.0, so no sqlx release reaches the fix (`cargo info sqlx@0.9.0` and the registry index, 2026-11-14).
- The advisory is a WAL information disclosure in C SQLite that needs local access. The store opens `history.db` in WAL mode (`crates/store/src/lib.rs:142-148`).
- basic-toml comes in only through askama_derive 0.16.1, a proc-macro used at build time. askama 0.16.1 is the latest release (`cargo info askama`, 2026-11-14).
- unsafe-libyaml has two paths:
  - serde_yaml 0.9 through uaparser 0.6.4, the latest release (`cargo info uaparser`, 2026-11-14). It parses the file at the configured `user_agent_regexes` path (`crates/ip/src/backend/user_agent.rs:16-18`). That file is uap-core's `regexes.yaml`, which the operator fetches at deploy time (`crates/ip/data/fetch.sh:32`); it is never request data.
  - serde_yaml_ng, which netray-ip uses as `serde_yaml` (`crates/ip/Cargo.toml:29`) only to serialize its own response values (`crates/ip/src/format.rs:45`).
- `deny.toml:21-26` ignores only RUSTSEC-2024-0436 (paste via utoipa-axum). It still matches `Cargo.lock`.
- `tests/repo/test_advisories.sh:17` pins `EXPECTED_IGNORES="RUSTSEC-2024-0436"`, with per-ID comment keyword arms at `:58-68` and lockfile floor checks at `:85-113`. A change to it carries `ADLC-Test-Change` (`AGENTS.md`).
- On 2026-11-14, `npm audit --omit=dev` reported 0 vulnerabilities. Full `npm audit` reported:
  - vitest and @vitest/mocker 4.1.11: GHSA-qxg6-89mg-rqv8 and GHSA-xp26-3454-mjgp, moderate, fixed in 4.1.12.
  - happy-dom 20.14.5: GHSA-834v-8h66-3h79, GHSA-c8p4-j97h-3vg8 and GHSA-5fpq-26gf-qwcr, high, fixed only in 21.0.4.
- Both vitest and happy-dom are direct dev dependencies (`crates/web/islands/package.json:12,14`). The lockfile is the root `package-lock.json` (npm workspaces).
- Advisory scans stay out of the gate (`justfile:19-21`, `specs/rules/workflow-rules.md:71`, R-J6). `cargo deny check advisories` and `npm audit` are each phase's behavioural check, as in `specs/features/advisories/spec.md`.
- The gate is `just adlc-verify`, which runs offline.

## Requirements

1. `Cargo.lock` holds every rustls package at ≥ 0.23.46, rustls-webpki ≥ 0.103.16, webpki-roots ≥ 1.0.10, webpki-root-certs ≥ 1.0.10, tokio ≥ 1.53.3 and rustix ≥ 1.1.6. Reach these with `cargo update -p` for these six crates only; before the update, a `--dry-run` confirms that no other package changes. No manifest changes, and `rust-version` stays 1.88 (agent default, confirmed by operator, 2026-11-14).
2. `deny.toml` ignores exactly RUSTSEC-2024-0436, RUSTSEC-2026-0459, RUSTSEC-2026-0374 and RUSTSEC-2024-0449 (agent default, confirmed by operator, 2026-11-14). Each new ignore has its own comment block directly above it, with a blank line between entries, so no two IDs share a comment (agent default, confirmed by operator, 2026-11-14). The comment states the dependency path and reachability:
   - RUSTSEC-2026-0459: libsqlite3-sys via sqlx-sqlite. No sqlx release allows ≥ 0.39. The disclosure needs local access to `history.db`. Revisit when sqlx lifts its cap.
   - RUSTSEC-2026-0374: basic-toml via askama_derive, a build-time proc-macro only.
   - RUSTSEC-2024-0449: unsafe-libyaml via uaparser, which parses only the operator-fetched uap-core `regexes.yaml` at startup, and via serde_yaml_ng, used only to serialize netray's own values.
3. `tests/repo/test_advisories.sh` expects exactly the four IDs of requirement 2. It requires these comment keywords, each matched as a case-insensitive substring and all required. The check guards against a missing or copy-pasted comment, not a misleading one, which review reads (agent default, confirmed by operator, 2026-11-14):
   - `sqlx` for RUSTSEC-2026-0459.
   - `askama` for RUSTSEC-2026-0374.
   - `uaparser` and `serde_yaml` for RUSTSEC-2024-0449.
4. `tests/repo/test_advisories.sh` fails when any package named in requirement 1 has a `Cargo.lock` version below its floor. It checks every package of that name, as it does for hickory, using the existing numeric `sort -t.` comparison on plain x.y.z versions. It fails when the lockfile has no such package. Its checks that exist today are unchanged (agent default, confirmed by operator, 2026-11-14).
5. `cargo deny check advisories` exits 0 and prints no `advisory-not-detected` warning. This holds against the advisory database on the day of implementation. It is a behavioural check, not a gate test, because the database moves daily (agent default, confirmed by operator, 2026-11-14).
6. `crates/web/islands/package.json` declares `happy-dom` `^21.0.4` and `vitest` `^4.1.12` (agent default, confirmed by operator, 2026-11-14). `tests/repo/test_islands_package.sh:29` requires happy-dom `^21` in place of `^20`, a change under `ADLC-Test-Change` naming this requirement (agent default, confirmed by operator, 2026-11-14). The root `package-lock.json` resolves happy-dom, vitest and @vitest/mocker at or above those versions. Full `npm audit` reports 0 vulnerabilities, and the islands' `npm test` passes.
7. `tests/repo/test_advisories.sh` fails when the root `package-lock.json` resolves happy-dom below 21.0.4, or vitest or @vitest/mocker below 4.1.12. It also fails when one of them is absent. It reads every `packages` key ending in `node_modules/<name>` with `node -e`, because no `tests/repo` script uses jq (agent default, confirmed by operator, 2026-11-14).

## Phase 1 — Cargo advisories

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

Covers `Cargo.lock` (`cargo update -p` for the six crates), `deny.toml` (three new commented ignores) and `tests/repo/test_advisories.sh` (expected set, keyword arms, lockfile floors). The end state: `cargo deny check advisories` exits 0, and the binary still builds and runs.

### Test Scenarios

GIVEN the tree WHEN `tests/repo/test_advisories.sh` runs THEN it fails today: the ignore list is not the four IDs, and rustls 0.23.45, tokio 1.53.2 and the other packages sit below their floors.
GIVEN a `Cargo.lock` copy with rustls at 0.23.45 and every other floor met WHEN the floor check reads it THEN it fails naming rustls 0.23.45.
GIVEN a `Cargo.lock` copy where every floor of requirement 1 is met WHEN the floor check reads it THEN it passes.
GIVEN a `Cargo.lock` copy with no rustix package WHEN the floor check reads it THEN it fails naming rustix.
GIVEN the existing fixture `tests/repo/fixtures/advisories/deny-missing-comment.toml` WHEN the ignore check runs on it with the four expected IDs THEN it still reports `RUSTSEC-2024-0436 has no comment`.
GIVEN a `deny.toml` whose RUSTSEC-2026-0459 comment does not mention sqlx WHEN the ignore check reads it THEN it fails naming RUSTSEC-2026-0459.
GIVEN the updated tree WHEN `cargo deny check advisories` runs THEN it exits 0 with no `advisory-not-detected` warning.
GIVEN the updated tree WHEN `netray serve --check-config crates/netray/netray.example.toml` runs from a fresh build THEN it exits 0.
GIVEN the updated tree WHEN `crates/common/Cargo.toml` is read THEN `rust-version` is still "1.88" and no `[dependencies]` line of any manifest changed.

## Phase 2 — npm advisories

**Depends on:** Phase 1
**Requirements:** 6, 7

Covers `crates/web/islands/package.json` (happy-dom 21, vitest 4.1.12), the root `package-lock.json`, the happy-dom major in `tests/repo/test_islands_package.sh:29`, any islands test that happy-dom 21 breaks, and the npm floor check in `tests/repo/test_advisories.sh`. The end state: full `npm audit` reports 0, the islands test suite passes, and the islands bundle still builds.

### Test Scenarios

GIVEN the tree WHEN `tests/repo/test_advisories.sh` runs THEN it fails today naming happy-dom 20.14.5 and vitest 4.1.11.
GIVEN a `package-lock.json` copy with happy-dom 21.0.4, vitest 4.1.12 and @vitest/mocker 4.1.12 WHEN the npm floor check reads it THEN it passes.
GIVEN a `package-lock.json` copy with no @vitest/mocker entry WHEN the npm floor check reads it THEN it fails naming @vitest/mocker.
GIVEN the updated tree WHEN full `npm audit` runs at the repository root THEN it reports 0 vulnerabilities.
GIVEN the updated tree WHEN the islands' `npm test` runs THEN it passes.
GIVEN the updated tree WHEN `tests/repo/test_islands_package.sh` runs THEN it passes with happy-dom `^21.0.4`, and a `package.json` copy with `^20.14.5` fails it.
GIVEN the updated tree WHEN `just adlc-setup` rebuilds the islands bundle and a fresh `netray` build embedding it runs `netray serve --check-config crates/netray/netray.example.toml` THEN it exits 0.
GIVEN the updated tree WHEN `just adlc-verify` runs THEN the gate is verified.

## Decision log

- One feature for both the Cargo and npm advisories, over two features: they share one triage date and one repository check (agent default, confirmed by operator, 2026-11-14).
- Cargo fixes go only through `cargo update -p`, over raising the manifest floors: the lockfile is what ships, and `test_advisories.sh` pins its floors (agent default, confirmed by operator, 2026-11-14).
- Ignore RUSTSEC-2026-0459 (libsqlite3-sys) with a reachability comment, over moving to sqlx 0.9 or patching libsqlite3-sys: no sqlx release reaches ≥ 0.39, the disclosure needs local access to `history.db`, and sqlx 0.9 needs Rust 1.94 (agent default, confirmed by operator, 2026-11-14).
- Ignore RUSTSEC-2026-0374 (basic-toml) and RUSTSEC-2024-0449 (unsafe-libyaml), over replacing askama or uaparser: both crates are unmaintained with no fix. One is build-time only; the other parses only the operator-fetched uap-core file and serializes netray's own values. Their parents are on their latest releases (agent default, confirmed by operator, 2026-11-14).
- Bump happy-dom to major version 21, over pinning 20.x with an npm-audit exception: 20.x has no fixed release, it is dev-only test tooling, and the islands suite is the guard (agent default, confirmed by operator, 2026-11-14).
- The repository check pins the npm floors (requirement 7), over leaving npm unpinned: the argus watch has no `npm audit`, so this check is the only offline guard against a regression (agent default, confirmed by operator, 2026-11-14).
- Leave RUSTSEC-2023-0071 (rsa) alone, over adding an ignore: cargo-deny, the tool of record, does not report it, and an ignore for an undetected advisory would trigger `advisory-not-detected` (agent default, confirmed by operator, 2026-11-14).
- Leave the obsolete keyword arms of `test_advisories.sh` in place, over removing them: they predate this feature and are harmless (agent default, confirmed by operator, 2026-11-14).

- Reading fixes: the uaparser path is described as it is (an operator-fetched file, not bundled). The version claims cite `cargo info` dated 2026-11-14. The entry-point scenarios use `netray serve --check-config crates/netray/netray.example.toml`. Phase 2 owns the happy-dom major pin in `test_islands_package.sh` and ends at the product's entry point. The Goal separates offline pins from behavioural checks. The keyword check's threat model is stated (agent default, confirmed by operator, 2026-11-14).
- Dry-run points, all taken: the six-crate update is re-probed with `--dry-run` first. Each ignore gets its own comment block. Keywords are case-insensitive substrings, all required. The floor checks cover every package of a name with the existing `sort -t.` comparison and fail on an absent package. The npm lockfile is read with `node -e`, not jq. Pre-release versions are out of scope, because the lockfiles hold plain x.y.z. `cargo deny` stays a behavioural check, since the advisory database drifts. A happy-dom 21 break in an islands test is fixed in the test or the code it tests, within phase 2 (agent default, confirmed by operator, 2026-11-14).

## Open decisions

None.

## Out of scope

- The daily advisory watch and its schedule: the private ops SDD (netray.info `specs/sdd/monorepo.md` R4.6).
- CI and gate composition: `justfile` `check`/`deny`, `.github/workflows/ci.yml`, `specs/rules/workflow-rules.md` R-J6.
