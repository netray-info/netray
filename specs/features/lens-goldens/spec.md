# Spec: lens result goldens and backend results tables

Status: Ready for Implementation
Created: 2026-10-09

Nothing pins lens's whole result today. `tests/fixtures/contracts/` holds the backends' goldens but no lens golden, so a change in scoring, in a backend's mapping or in a backend's own checks can move a grade without any test noticing. The backend contract goldens are built from literals (`crates/tlsight/tests/contract_golden.rs`, `crates/beacon/tests/contract_golden.rs`), so they cannot show a change inside a backend's check code either.

This spec pins today's results on two layers before 0.23.0 changes any of them: lens's result for each committed backend golden, and results tables that drive the real check code of the backends whose output 0.23.0 changes. Every later grade change then moves pinned rows deliberately, in a commit carrying `ADLC-Test-Change` that names its requirement. The next major version's engine compares against both layers.

No production behaviour changes. Every test here is a pinning test: green at its baseline commit.

## Requirements

1. **lens result goldens.** `crates/lens/tests/lens_golden.rs` starts local stub servers that serve the committed backend goldens from `tests/fixtures/contracts/` at the paths and methods lens calls (the pattern of `crates/lens/tests/contract_backends.rs`). lens runs with the backend and scoring settings of `crates/lens/tests/fixtures/lens.production.toml`, the backend URLs pointed at the stubs. The test drives `POST /api/check` through lens's router in-process and reads the SSE events. For each fixture it compares a projection with `tests/fixtures/contracts/lens-<fixture>.json`, and rewrites that file when `UPDATE_GOLDEN=1` is set.
2. **Projection.** The golden records:
   - the summary: `grade`, `score`, `overall`, `sections` (status per section), `section_grades`, `hard_fail`, `hard_fail_checks`, and the section names in `not_applicable` (the reason is prose);
   - per section event: its status, its grade if present, and every check as `name` plus `verdict`.

   Check messages, durations, the cache flag, the snapshot id and the domain-independent metadata are not recorded. Maps are written in sorted key order and lists in emitted order, so the file is byte-stable across runs. Completeness is recorded in today's shape (`overall` and the section statuses); a later `complete` field adds to the projection when it exists.
3. **First fixtures.** Three fixtures, built only from goldens that exist today:
   - `healthy`: `prism.sse`, `tlsight-inspect.json`, `spectra-inspect.json`, `beacon.sse`, `ifconfig-json.json`;
   - `no-mx`: as `healthy`, email from `beacon-no-mx.sse`;
   - `mx-cname`: as `healthy`, email from `beacon-mx-cname.sse`.

   A fixture is one entry in a table in the test (name plus the golden per backend), so a later requirement adds one by adding an entry and running with `UPDATE_GOLDEN=1`.
4. **beacon DKIM results table.** A table test drives the real `check_dkim` (`crates/beacon/src/checks/dkim.rs`) with a stub resolver and records per row the category verdict, the sub-check names and verdicts, and the category detail. Rows: parked domain (Null MX, `SPF -all`) whose only key has an empty `p=`; sending domain whose only key has an empty `p=`; sending domain with no key at any probed selector; sending domain with one valid key.
5. **tlsight port results table.** A table test drives the real `assess_port` (`crates/tlsight/src/quality/mod.rs`) and records per row the port verdict and every check name and verdict. Rows: every IP failed with a target-side error; IPv6 failed and IPv4 succeeded; every IP succeeded. A second table drives tlsight's per-IP inspection against a closed local port and records the error code it reports.
6. **prism lint results table.** A table test drives mhost's `check_dnssec` and `check_ttl`, as prism calls them, over recorded `Lookups` built in the test, and records per row every lint result. Rows: a signed zone answered without RRSIGs to a stub lookup; one record set returned with differing TTLs; the same lint line produced by two resolvers, as prism's `+check` output carries it.
7. **target policy results table.** A table test over `netray_common::target_policy::is_allowed_target` records allowed or refused for one address from each of `0.0.0.0/8` (not `0.0.0.0`), `240.0.0.0/4`, `198.18.0.0/15`, `192.0.0.0/24`, a 6to4 address embedding a public IPv4, and for `8.8.8.8`, `1.1.1.1` and `2606:4700::`.
8. **No production change.** Where a table cannot reach the real code from a test, the only production change allowed is a behaviour-neutral extraction or visibility change that exposes it, as `outbound-fetch` requirement 7 did.

## Phase 1 — lens result goldens

**Depends on:** none
**Requirements:** 1, 2, 3, 8

### Test Scenarios

- GIVEN the stubs serve the `healthy` goldens WHEN `POST /api/check` runs for `example.com` THEN the projection equals `lens-healthy.json`.
- GIVEN the stubs serve the `no-mx` goldens WHEN the check runs THEN the projection equals `lens-no-mx.json`; today lens marks the no-MX mail buckets `skip` and `not_applicable` stays empty (amended in Phase 1, see the report).
- GIVEN the stubs serve the `mx-cname` goldens WHEN the check runs THEN the projection equals `lens-mx-cname.json`.
- GIVEN any fixture WHEN the test runs twice THEN both runs write byte-identical files.
- GIVEN `UPDATE_GOLDEN` unset and a golden that differs from the projection WHEN the test runs THEN it fails and prints the differing fields.

## Phase 2 — Backend results tables

**Depends on:** Phase 1
**Requirements:** 4, 5, 6, 7, 8

### Test Scenarios

- GIVEN a parked domain whose only DKIM key has an empty `p=` WHEN `check_dkim` runs THEN the row records today's `key_revoked` sub-check verdict and the category verdict.
- GIVEN a sending domain whose only DKIM key has an empty `p=` WHEN `check_dkim` runs THEN the row records today's result and detail.
- GIVEN a sending domain with no key at any probed selector WHEN `check_dkim` runs THEN the row records today's `no_dkim` result.
- GIVEN a sending domain with one valid key WHEN `check_dkim` runs THEN the row records today's pass.
- GIVEN every IP of a port failed with a target-side error WHEN `assess_port` runs THEN the row records today's verdict and check list.
- GIVEN IPv6 failed and IPv4 succeeded WHEN `assess_port` runs THEN the row records today's verdict and check list.
- GIVEN every IP succeeded WHEN `assess_port` runs THEN the row records today's verdict and check list.
- GIVEN a closed local port WHEN tlsight inspects that IP THEN the row records today's error code.
- GIVEN a signed zone answered without RRSIGs WHEN `check_dnssec` runs THEN the row records today's lint results.
- GIVEN one record set with differing TTLs WHEN `check_ttl` runs THEN the row records today's lint results.
- GIVEN two resolvers producing the same lint line WHEN prism assembles its `+check` lint output THEN the row records today's lines.
- GIVEN one address from each named range, a public 6to4 address and three public addresses WHEN `is_allowed_target` runs THEN each row records today's answer.

## Open decisions

None. Golden at route level, messages not recorded, all four tables in this feature (operator, 2026-10-09).
