# Spec: fix ip evidence race

Status: Done
Created: 2026-10-14
Finished: 2026-10-14

## Goal

`crates/ip/tests/evidence.rs` and `crates/ip/tests/contract_status.rs` pass when its tests run in parallel: each `run` call writes its list file into a directory no other call uses.

## Non-goals

- Production code changes.

## Context and constraints

- `run` builds `temp_dir()/netray-ip-evidence-{pid}-{config_key}` (`crates/ip/tests/evidence.rs:37`) and removes it after the run. `addresses_block_validates_and_reputation_references_it` and `listed_address_row_carries_the_matching_flag` both use `feodo_botnet_ips` in one process, so one can delete the other's list before the module loads it.
- CI run 38523840435 failed with `feodo_botnet_ips: column c2, left Bool(false), right Bool(true)`.

## Requirements

1. `run` uses a directory unique per call (a process-wide atomic counter in the name).
2. `module_with_listed_address` in `crates/ip/tests/contract_status.rs` uses a directory unique per call, the same way.

## Phase 1 — Unique list directory

**Depends on:** none
**Requirements:** 1, 2

### Test Scenarios

- GIVEN the netray-ip evidence tests, WHEN run 20 times in parallel (`--test-threads` default), THEN every run passes.
- GIVEN the netray-ip contract_status tests, WHEN run 20 times in parallel, THEN every run passes.

## Decision log

- Fix the test, not the module: the module reads its configured path correctly; the race is the test's shared path (operator rule: feature path for every change, 2026-10-14).

- AMENDMENT 2026-10-14: requirement 2 added after the review found the same shared-path race in `contract_status.rs:74`; same fix, same feature (implementer, under the operator's "fix on the branch" rule for review findings).

## Open decisions

None.

## Out of scope

None.
