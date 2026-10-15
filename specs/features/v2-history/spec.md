# Spec: v2-history

Status: Ready for Implementation
Created: 2026-10-15

## Goal

Every complete lens run with default options, whatever route started it, is recorded in a new `history.db` owned by a new crate `netray-store`: an identical rerun moves the latest entry's `last_seen_at` and stores nothing, any other complete run becomes a new entry carrying its diff against the predecessor, an incomplete run is diffed but never stored, and an hourly sweep enforces the 90-day retention of SDD §4.5. The diff is a pure function in `netray-model`, table-tested over every R12 class. An incomplete run is never badged. lens's V1 wire, its goldens, the moka cache, OG and `snapshots.db` are unchanged. This is SDD Phase 2 criteria V2.1–V2.5; V2.5's "never served as a cache hit" holds today (`crates/lens/src/cache.rs:39-87` refuses incomplete runs) and moves onto the history with `v2-admission`.

## Non-goals

- Replacing the moka cache with the history, joining runs in flight, throttle and admission (V2.6, feature `v2-admission`).
- `/r/<id>` for history entries, its 410 and the privacy page (V2.7–V2.8, feature `v2-entries`).
- Serving the diff to any client (API v1, Phase 3; result page, Phase 4).
- Recording runs with caller-supplied options (`dkim_selectors`).
- Migrating or touching `snapshots.db` (S2, SC15).
- A second real scoring profile.
- A release; ships with 0.27.0.

## Context and constraints

- Rust workspace, `members = ["crates/*"]`; a new crate `crates/store` (package `netray-store`) joins automatically. `netray-model` has no workspace dependency; `netray-store` depends on `netray-model` only (SDD §3.1). Scoring lives in `netray-engine` (`crates/engine/src/scoring.rs:23`); only profile v3 is embedded (`crates/engine/src/profile.rs:13`, `Profiles::get` at :147).
- sqlx 0.8 is used only by lens (`crates/lens/Cargo.toml:65`, features `runtime-tokio, sqlite, migrate, chrono`); a second user moves it to `[workspace.dependencies]` (CLAUDE.md "Shared dependencies"). The only migrations are `crates/lens/migrations/0001_create_snapshots.sql`.
- The snapshot store is the pattern: SqlitePool with WAL, `sqlx::migrate!`, nanoid shortid of 8 chars over `[0-9A-Za-z]` with collision retry (`crates/lens/src/snapshot/store.rs:37-89`, `shortid.rs`), hourly sweep loop (`snapshot/sweep.rs`), opened in `crates/lens/src/lib.rs:67-87` with `exit(1)` on open or migration failure.
- `[snapshots] enabled, db_path` (`crates/lens/src/config.rs:410-429`); every config struct carries `deny_unknown_fields` (`tests/repo/test_config_strict.sh`). `--check-config` reads no file a config names (CLAUDE.md); startup refusals of a path are tested by starting lens, as the GeoIP block does (`tests/repo/test_check_config.sh:219-228`).
- Gate scripts start lens inside the checkout and redirect only the snapshot DB (`tests/repo/test_smoke_services.sh:29`, `test_header_parity.sh:52`, `justfile:129`).
- Every lens run goes through `run_check_with_evidence` (`crates/lens/src/check.rs:90`): `/api/check` (`routes.rs:735`), badge (`routes.rs:37` via `run_check`) and OG (`og/handler.rs:142`). `/api/check` with `dkim_selectors` bypasses the cache (`routes.rs:760`) and probes those selectors (`crates/email/src/checks/dkim.rs:96`).
- lens scores by mapping its sections to `SectionOutcome`s (`crates/lens/src/score.rs:165-185`) and calling `netray_engine::score`; its `OverallScore` (`grade`, `complete`) is what lens reports, and can differ from scoring the raw engine events (lens maps an empty IP lookup to `NoDnsResults`, `crates/lens/src/modules.rs:414`; the IP module says `not_applicable`). A hard fail gives grade F even when incomplete (S35, `scoring.rs:137`); the badge renders that F today (`routes.rs:604`).
- A section-level outcome (`NotApplicable`, `Incomplete`, `TimedOut`) carries no checks (`crates/engine/src/lib.rs:113`, `:301`); the email module answers `Incomplete "busy"` when its semaphore is full (`crates/email/src/module.rs:208`).
- `netray-model` has `CheckId`, `Status` (incl. `NotTested`, `Unmeasured`), `CheckResult`, `Grade` (Serialize only, `crates/model/src/lib.rs:158`), and the evidence shapes `Records, Table, Chain, Tree, Fields, Text` (`crates/model/src/evidence.rs:277`). There is no `Run` type, no canonical JSON, no fingerprint and no diff.
- Volatile paths are declared per module by block name (`Module::volatile()`, `crates/engine/src/lib.rs:30`, `:145`), while blocks carry address qualifiers (`tls.handshake@<addr>`, `crates/tls/src/module.rs:407`). CSP nonces are not addressable by a path (SDD §3.7, `v2.md:313`). `fields` blocks hold sets as repeated keys in resolver order (SPF `authorized_prefixes`, `crates/email/src/module.rs:510`).
- `/api/check` does not merge concurrent requests for a domain (`routes.rs:760-788`); sqlx 0.8 begins deferred transactions, so two concurrent read-then-write transactions in WAL mode fail with `SQLITE_BUSY_SNAPSHOT`.
- Every catalogue check has weight ≥ 1 (V1.10), so a stored (complete) run never holds `unmeasured`; an incomplete current run may.
- Gate: `just adlc-verify`; full suite `just check`; run feature-unification-sensitive tests with `--workspace`. Amend `specs/sdd/v2.md` in the same commit as the code that changes what it says.

## Requirements

1. `netray-model` holds a serialisable `Run`: per check its ID and status, the evidence blocks, `profile_version`, `engine_version`, `grade` and `complete`, and per section the outcome kind and reason. `Grade` gains `Deserialize`; a `Run` round-trips through JSON unchanged.
2. lens builds the `Run` from what it scored: the `SectionOutcome`s of `crates/lens/src/score.rs`, the `grade` and `complete` of its `OverallScore`, and the blocks of `run_check_with_evidence`. Every catalogue check of a section appears in the `Run`: a measured section with its statuses, a not-applicable section with every check `not_applicable`, an incomplete or timed-out section with every check `unmeasured`.
3. `netray-model` computes a run's fingerprint as SHA-256 over canonical JSON of: the profile version; every check's ID and status; every evidence block with the volatile paths removed (a path matches a block by name, whatever its address qualifier; a row path removes the whole row), every set in canonical order (records; table rows by key; repeated keys of a fields block by value; headers by name), and CSP nonce sources (`'nonce-…'`) in `content-security-policy` and `content-security-policy-report-only` values replaced by one fixed token. Two runs that differ only in volatile values, set order or nonce values have equal fingerprints.
4. `netray-model` diffs a current run against a predecessor run per check by status transition: fixed (warn or fail → pass), improved (fail → warn), new (pass → warn or fail), worse (warn → fail), now applicable (not_applicable → measured), no longer applicable (measured → not_applicable), into or out of not tested, into or out of not measured (`unmeasured`). When both statuses are not_tested or unmeasured, the current status decides: into not tested or into not measured. Neither not-tested nor not-measured transitions ever count as fixed or worse. A check whose status is unchanged produces no row.
5. A check ID present in only one run is `added` or `removed`; when the two runs' `engine_version` differ it is `added by release` or `removed by release` instead, and the diff marks that the predecessor has another engine version.
6. The diff reports evidence changes per block by one algorithm over the six shapes, only for blocks of sections measured in both runs: changed keys (fields), added, removed or changed rows by key (table), each item signed `+`, `-` or `~`, added or removed records (records), changed positions (chain), changed node paths (tree), changed (text); added or removed blocks. Volatile paths and nonce values are never reported; set order is never a change.
7. The diff reports the grade change; when the profile versions differ it reports two steps, predecessor grade → predecessor re-scored with the current profile (profile effect) → current grade (domain effect). The re-scored grade is an input to the model's diff; `netray-engine` computes it from a stored `Run` and any loadable profile.
8. `netray-store` creates `history.db` with the schema of SDD §4.1 (columns, `ON DELETE SET NULL` on `predecessor_id`, index on `(domain, created_at DESC)`) through sqlx migrations that use no SQLite-only types or functions; the entry ID is an 8-character nanoid over `[0-9A-Za-z]` with collision retry.
9. `netray-store` records a complete run in one transaction: read the domain's latest entry; equal fingerprint → move its `last_seen_at` to now and store nothing else; otherwise insert a new entry with `predecessor_id` = latest, the run JSON (with the evidence schema version), the fingerprint and the diff (none for a first entry). Record returns which of first, identical or changed happened, and the diff. The store serialises its writes (record and sweep) within the process, so concurrent records of one domain chain or match and never fail on each other.
10. `netray-store` refuses to store a run whose `complete` is false, whatever its grade; for such a run it returns the diff against the latest entry (or none without one) and writes nothing, `last_seen_at` included.
11. `netray-store`'s sweep, in one transaction at a given time: deletes every entry of a domain whose latest entry's `last_seen_at` is older than 90 days; otherwise deletes an entry whose `last_seen_at` is older than 90 days unless it is the latest entry or the latest entry's predecessor. A surviving entry whose predecessor was deleted keeps its stored diff and gets `predecessor_id` NULL.
12. lens config gains `[history]` with `enabled` (default true) and `db_path` (default `"history.db"`), `deny_unknown_fields`. With `enabled`, lens opens and migrates the store at startup and exits 1 with an error naming the history store when either fails; `--check-config` does not open it. `tests/repo/test_check_config.sh` tests the startup refusal of an unopenable `db_path` as its GeoIP block does. lens runs the sweep hourly. Every gate script that starts lens sets `LENS_HISTORY__DB_PATH` into its temporary directory, so no `history.db` is written into the checkout.
13. Every run without caller-supplied options that ends in `run_check_with_evidence` is recorded (complete) or diffed (incomplete) once, whichever route started it; a run with `dkim_selectors` is neither recorded nor diffed. lens's responses, V1 wire, lens goldens, moka cache, OG and snapshot behaviour are unchanged.
14. The badge route renders an incomplete run (`complete` false) as unscored, with the short Cache-Control of `is_unscored_grade` (`crates/lens/src/routes.rs:854`), whatever its grade.
15. A failed record logs at `error` and increments `lens_history_record_errors_total` (name default, not interviewed); the client's response is unaffected. The metric is registered at startup.
16. `specs/sdd/v2.md` is amended: §4.3 names the not-measured class, the precedence of requirement 4 and the release-added/removed classes; §3.1 says the profile-effect grade is computed by `netray-engine` and passed to the model's diff, keeping `netray-store` on `netray-model` only; §3.7 adds repeated fields keys to the canonical sets; §4.1 says the store serialises its writes until §4.4's one-run-per-domain exists; §4.4's moka citation names `Cache::builder` in `crates/lens/src/state.rs` instead of a line; the Phase 2 table marks V2.1–V2.5 as built by `v2-history`.
17. The modules' volatile lists cover values that change between identical responses: http adds the header rows `expires`, `x-timer`, `x-served-by`, `x-cache`, `x-cache-hits`, `cf-cache-status` and the `http.server` keys `address`, `org`, `network_type` (the first A record in resolver order, `crates/http/src/input.rs:70`); dns adds `dns.rrsig/data` (expiration, inception and signature in one text value, `crates/dns/src/module.rs:467`). SDD §3.7's volatile table lists them.

## Phases

## Phase 1 — Run, fingerprint and diff in netray-model

**Depends on:** none
**Requirements:** 1, 3, 4, 5, 6

`crates/model`: `Run`, `Grade: Deserialize`, canonical JSON and fingerprint taking the volatile paths as a parameter, the diff over checks and evidence. Table-driven tests over every status pair and every shape.

### Test Scenarios

GIVEN a `Run` with all six shapes WHEN it is serialised and parsed THEN it equals the original.
GIVEN two runs differing only in `http.headers/value@date` and the order of A records WHEN fingerprinted with http's and dns's volatile paths THEN the fingerprints are equal.
GIVEN the same two runs with a changed `http.headers/value@strict-transport-security` WHEN fingerprinted THEN the fingerprints differ.
GIVEN two runs differing only in `handshake_ms` of `tls.handshake@192.0.2.1:443` WHEN fingerprinted with the path `tls.handshake/handshake_ms` THEN the fingerprints are equal.
GIVEN two runs differing only in the order of repeated `authorized_prefixes` keys of a fields block WHEN fingerprinted and diffed THEN fingerprints are equal and the diff has no evidence row.
GIVEN two runs whose fields block differs in one repeated key's value WHEN fingerprinted THEN the fingerprints differ.
GIVEN two runs differing only in the nonce of a CSP header WHEN fingerprinted and diffed THEN fingerprints are equal and the diff has no evidence row.
GIVEN two runs differing in a CSP directive other than the nonce WHEN fingerprinted THEN the fingerprints differ.
GIVEN the status table (pass, warn, fail, not_applicable, not_tested, unmeasured)² WHEN each pair is diffed THEN each unequal pair yields exactly one class per requirement 4, equal pairs no row, and no not-tested or not-measured pair yields fixed or worse.
GIVEN a check only in the current run and equal engine versions WHEN diffed THEN it is `added`; GIVEN different engine versions THEN `added by release` and the diff marks the version change.
GIVEN a predecessor whose tls section is not applicable (every tls check `not_applicable`) and a current run with tls measured, across different engine versions WHEN diffed THEN every tls check is `now applicable`, none `added by release`.
GIVEN a complete predecessor and a current run whose email section is incomplete (every email check `unmeasured`, no email blocks) WHEN diffed THEN every email check is `into not measured` and no email block is reported removed.
GIVEN a table block with one changed row and one reordered row WHEN diffed THEN exactly the changed row is reported.
GIVEN a changed value under a volatile path WHEN diffed THEN no evidence change is reported.

## Phase 2 — Profile-effect grade in netray-engine

**Depends on:** Phase 1
**Requirements:** 7

`crates/engine`: score a stored `Run` under a given `Profile`; the diff's grade part takes predecessor grade, re-scored grade and current grade. Tests build a second profile in memory; nothing is added to `lock.toml`.

### Test Scenarios

GIVEN a predecessor scored under profile A and a current run under profile B with an in-memory profile B that reweights one check WHEN diffed THEN the grade change has two steps, profile effect then domain effect.
GIVEN both runs under the same profile WHEN diffed THEN the grade change has one step.
GIVEN a stored `Run` WHEN re-scored under its own profile THEN the grade equals the stored grade.

## Phase 3 — netray-store

**Depends on:** Phase 1
**Requirements:** 8, 9, 10, 11

New crate `crates/store`: migrations, `record`, `latest`, `sweep(now)`; sqlx moves to `[workspace.dependencies]`. Time is a parameter in every call so tests set it.

### Test Scenarios

GIVEN a fresh database WHEN migrated THEN the schema of §4.1 exists and the migration files contain no SQLite-only type or function (a test greps them).
GIVEN no entry for a domain WHEN a complete run is recorded THEN one entry exists with `predecessor_id` NULL, no diff, outcome first.
GIVEN a latest entry WHEN an identical complete run is recorded later THEN the entry count is unchanged and `last_seen_at` moved.
GIVEN a latest entry WHEN a complete run with one changed status is recorded THEN a second entry exists with `predecessor_id` = the first and a stored diff.
GIVEN no entry WHEN two different complete runs of one domain are recorded concurrently THEN both succeed and two entries exist, the second's predecessor the first.
GIVEN no entry WHEN two identical complete runs of one domain are recorded concurrently THEN both succeed and one entry exists.
GIVEN a latest entry WHEN an incomplete run with grade F is recorded THEN nothing is written, `last_seen_at` unchanged, and the diff is returned.
GIVEN a latest entry WHEN an incomplete run identical in fingerprint is recorded THEN `last_seen_at` is unchanged.
GIVEN a domain whose latest entry was last seen 91 days ago WHEN swept THEN no entry of the domain remains.
GIVEN entries e1 → e2 → e3 (latest) all last seen 91 days ago except e3 at 1 day WHEN swept THEN e1 is deleted, e2 and e3 remain.
GIVEN e1 → e2 → e3 → e4 (latest, 1 day), e2 last seen 91 days ago WHEN swept THEN e2 is deleted and e3 keeps its diff with `predecessor_id` NULL.
GIVEN an entry last seen 89 days ago WHEN swept THEN it remains.

## Phase 4 — lens records every run

**Depends on:** Phase 2, Phase 3
**Requirements:** 2, 12, 13, 14, 15, 16, 17

`crates/lens`: `[history]` config, store open/migrate/sweep at startup beside the snapshot store, one record call at the end of `run_check_with_evidence` for runs without options, building the `Run` from lens's scored sections, the badge's incomplete branch, the error metric, the gate-script overrides and the startup test, the SDD amendments.

### Test Scenarios

GIVEN `[history] enabled = true` and a temporary `db_path` WHEN `/api/check` completes a complete run THEN `history.db` holds one entry for the domain and the response body equals the golden.
GIVEN the same WHEN the badge route computes a complete run on a cache miss THEN an entry is recorded.
GIVEN a run with `dkim_selectors` WHEN it completes THEN `history.db` holds no new entry.
GIVEN an incomplete run WHEN it completes THEN `history.db` holds no new entry.
GIVEN a run lens scores incomplete because the IP section has no DNS results WHEN it completes THEN the recorded `Run` is not stored and its IP checks are `unmeasured`.
GIVEN an incomplete run with a hard fail (grade F) WHEN the badge renders it THEN the badge is unscored with the short Cache-Control.
GIVEN a complete run with grade F WHEN the badge renders it THEN the badge shows F with the long Cache-Control.
GIVEN `[history] enabled = false` WHEN a run completes THEN no `history.db` is created.
GIVEN a `db_path` in a nonexistent directory WHEN lens starts THEN it exits non-zero with an error naming the history store, and `--check-config` on the same file exits 0.
GIVEN a store whose writes fail WHEN a run completes THEN the response is unchanged and `lens_history_record_errors_total` is 1.
GIVEN an unknown key under `[history]` WHEN the config loads THEN it is rejected.
GIVEN `just adlc-verify` on a clean checkout WHEN it finishes THEN `git status --porcelain` lists no `history.db*`.
GIVEN two http module runs differing only in `Expires`, `cf-cache-status` and the order of the A records WHEN fingerprinted with http's volatile list THEN the fingerprints are equal.
GIVEN two dns module runs differing only in an RRSIG's `data` WHEN fingerprinted with dns's volatile list THEN the fingerprints are equal.

## Decision log

- lens records into the history now, moka/OG unchanged until `v2-admission` over a library-only store or replacing moka here: real entries accumulate from the 0.27.0 deploy while §4.4 stays in one feature (operator, 2026-10-15).
- Profile split tested with an in-memory second profile over deferring the split: V2.3 stays whole without adding a profile to the lock (operator, 2026-10-15).
- `[history] enabled, db_path` like `[snapshots]`, exit 1 on open or migration failure over always-on: a switch for dev and tests, same failure mode as the snapshot store (operator, 2026-10-15).
- Diff not on the lens wire over an additive field: V1 wire and goldens stay byte-identical; the diff becomes visible with API v1 and the result page (operator, 2026-10-15).
- A failed record logs and counts over failing the request: the store is not read by anyone yet, so it must not take down checks (operator, 2026-10-15).
- Record every complete run regardless of route over `/api/check` only: §4.4 "whatever started it"; badge-only domains would leave holes (operator, 2026-10-15).
- A separate not-measured class over folding `unmeasured` into not tested: a lookup failure (Rule M) and a local connect error stay distinguishable, and neither is a change of the domain (operator, 2026-10-15).
- Profile-effect grade computed in `netray-engine` and passed to the model's diff: keeps `netray-model` free of scoring and `netray-store` on `netray-model` only (SDD §3.1).
- Runs with `dkim_selectors` not recorded over recording them: a request option is not a domain change, and any visitor could otherwise create entries (independent reading; implementer under the operator's autonomy mandate, 2026-10-15).
- The badge's incomplete branch built here over deferring "never badged": V2.5 is an approved SDD criterion and the change is one branch (independent reading; implementer under the operator's autonomy mandate, 2026-10-15).
- Section-level outcomes expanded into per-check statuses (not_applicable, unmeasured) over diffing sections: a domain gaining an A record reads as "now applicable", a timeout as "into not measured", never as added or removed (independent reading; implementer, 2026-10-15).
- The `Run` built from lens's scored result over the raw engine events: what lens reported as incomplete is never stored (independent reading; implementer, 2026-10-15).
- Writes serialised in the store over relying on §4.4's one run per domain: §4.4 comes with `v2-admission`, and concurrent `/api/check` requests exist today (independent reading; implementer, 2026-10-15).
- Phase 1 reader findings, implementer under the operator's autonomy mandate (2026-10-15): a volatile row path covers the whole row, presence included (SDD §3.7 "a path addresses a whole row"), over comparing the row's presence; table and fields items carry their sign (`~`, `-`, `+`) over a flat key list, because a stored diff outlives its predecessor (R11); a nonce source ends at its closing quote only before whitespace or `;`, over swallowing up to any later quote; the volatile lists gain cache, expiry and resolver-order values and RRSIG data (R17) over storing an entry per CDN hit or re-signature.

## Open decisions

None.

## Out of scope

- Cache, run join, throttle, admission, prerender: `v2-admission` (SDD §4.4, V2.6).
- `/r/<id>`, 410, privacy page: `v2-entries` (SDD §4.6–4.7, V2.7–V2.8).
- R19's "first state recorded" and "not recorded: this run is incomplete" lines: rendered by the result page (SDD §5.7, Phase 4).
- OG's rendering of an incomplete run: the OG card moves to entries with `v2-entries` (R15).
- Deploying `db_path` on the persistent volume: netray-ops, with 0.27.0.
