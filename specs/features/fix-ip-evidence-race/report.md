# Report: fix-ip-evidence-race

## Phase 1 — Unique list directory

### Criteria

| ID | Criterion | Status | Test file |
|---|---|---|---|
| C1 | `run` uses a directory unique per call | green | `crates/ip/tests/evidence.rs` |
| C2 | the evidence tests pass under parallel execution | green | `crates/ip/tests/evidence.rs` |
| C3 | `module_with_listed_address` uses a directory unique per call | green | `crates/ip/tests/contract_status.rs` |
| C4 | the contract_status tests pass under parallel execution | green | `crates/ip/tests/contract_status.rs` |

The race did not reproduce locally (0 of 15 runs before the fix). The diagnosis comes from the code: `addresses_block_validates_and_reputation_references_it` and `listed_address_row_carries_the_matching_flag` share `temp_dir()/netray-ip-evidence-{pid}-feodo_botnet_ips`, and one test's `remove_dir_all` can empty the other's list before its module loads it. That is consistent with CI run 38523840435 reporting `c2` false. After the fix, 10 of 10 local runs passed and `adlc verify` is green.

C2 and C4 cannot fail on this host: the race never reproduced locally, so the 20-run check passes with or without the fix. The fix rests on the code reading (review 998b8e6..3b163f6, CONFIRMED by the refuter), not on the runs. After requirement 2 (review amendment), 20 of 20 runs of `cargo test -p netray-ip --features testing --test contract_status --test evidence` passed.

### Behavioural verification

skipped: a test-only change.
