# Review: fix-ip-evidence-race

## 998b8e6..3b163f6

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Testing
MAJOR | crates/ip/tests/contract_status.rs:74 | The race this change fixes in evidence.rs is still present, unchanged, in `contract_status.rs`. That file runs in the same `cargo test` invocation (justfile:52), so CI can still fail `contract_status_table` with `ip.reputation: expected Fail, got Pass`. | `contract_status_table` and `only_private_addresses_is_not_applicable_no_public_addresses` run in parallel threads and both build `temp_dir()/netray-ip-contract-{pid}/feodo.txt`. The second test's run is immediate (no public address), and then it calls `remove_dir_all` (line 120). If that removal lands after the first test's `fs::write` (line 77) and before `FeodoBotnetIps::from_file` reads the file, the read fails. A truncation by the other test's `fs::write` at that moment has the same effect: the read gets an empty file. Either way `from_file` returns None, feodo is not loaded, `is_c2` is false, and reputation comes out as Pass. If the removal lands between `create_dir_all` and `write`, the test panics on `unwrap`.
MINOR | specs/features/fix-ip-evidence-race/report.md:10 | C2 is marked green, but the check behind it passes with or without the fix, so it cannot fail. It also ran half the iterations the spec requires. | report.md:12 says the race reproduced in 0 of 15 local runs before the fix, and the fix was then confirmed with "10 of 10" local runs. If lines 40-42 of evidence.rs go back to main's shared path, the same check is still green. The scenario in spec.md:30 asks for 20 runs.

```quote crates/ip/tests/contract_status.rs:74
    let dir = std::env::temp_dir().join(format!("netray-ip-contract-{}", std::process::id()));
```

```quote crates/ip/tests/contract_status.rs:120
    std::fs::remove_dir_all(&dir).ok();
```

```quote justfile:52
    cargo test -p netray-ip --features testing --test module --test module_data --test module_v3 --test contract_status --test evidence
```

```quote specs/features/fix-ip-evidence-race/report.md:10
| C2 | the evidence tests pass under parallel execution | green | `crates/ip/tests/evidence.rs` |
```

```quote specs/features/fix-ip-evidence-race/spec.md:30
- GIVEN the netray-ip evidence tests, WHEN run 20 times in parallel (`--test-threads` default), THEN every run passes.
```

The fix itself in crates/ip/tests/evidence.rs:40-42 is sound:
- `fetch_add` returns a unique value even with Relaxed ordering, so every `run` call gets its own directory (5 calls across 3 tests). The pid keeps separate processes apart.
- `FeodoBotnetIps::from_file` (crates/ip/src/backend/feodo.rs:7) reads the whole file at `IpModule::new`, so removing the directory after `run` cannot affect the result.
- The pre-fix race this commit names (a missing or truncated file gives None, which makes c2 false) matches the code.

Other temp paths checked for the shared-path pattern, none with a collision:
- crates/ip/tests/module.rs:277: one test uses the path.
- crates/tls/tests/config_reload_tolerance.rs: one test.
- crates/common/tests/config_loader.rs: one name.
- crates/lens/tests: `NamedTempFile`.
- Unit tests in crates/*/src: ip backend `*.rs` and asn_info use atomic counters; ip classify_tests has one user; http config names differ per test; email config names are distinct; tls state has a nanosecond suffix.

The `contract_golden` and `lens_golden` writes happen only under `UPDATE_GOLDEN`, each to its own fixture path.

### Refutation

| Finding | Answer | Confidence |
|---|---|---|
| MAJOR contract_status.rs:74 shared temp dir | CONFIRMED | 8 |

### Calibration

| Finding | Refuter | Confidence | Result | Command |
|---|---|---|---|---|
| MAJOR contract_status.rs:74 | CONFIRMED | 8 | held | read `crates/ip/src/backend/feodo.rs:7-15`: an unreadable file is `.ok()` → an empty set, so `ip.reputation` passes instead of failing |

### Summary

Before refutation 0/1/1, after 0/1/1. Verified 1, held 1. Disposition: the major is fixed on this branch (requirement 2), the minor is corrected in the report.

Record note: the review row was written with `--model sonnet`; the reader ran on `opus` (`agents/reviewer.md`). The row was left as written.

## 3b163f6..c2d3654

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing
MINOR | specs/features/fix-ip-evidence-race/report.md:11 | C3 and C4 are marked `green` on the strength of `crates/ip/tests/contract_status.rs`, but nothing in that file fails if the fix is reverted, so these rows cannot fail. | Revert `crates/ip/tests/contract_status.rs:77-79` to the old `format!("netray-ip-contract-{}", std::process::id())`. All three tests still pass on this host. The report's own record at report.md:16 says the race did not reproduce in 0 of 15 runs and the 20-run check "passes with or without the fix". C3 could be tested deterministically, for example by calling `module_with_listed_address` twice and asserting that the two paths differ. No such test exists.

```quote specs/features/fix-ip-evidence-race/report.md:11
| C3 | `module_with_listed_address` uses a directory unique per call | green | `crates/ip/tests/contract_status.rs` |
```

```quote specs/features/fix-ip-evidence-race/report.md:16
C2 and C4 cannot fail on this host: the race never reproduced locally, so the 20-run check passes with or without the fix. The fix rests on the code reading (review 998b8e6..3b163f6, CONFIRMED by the refuter), not on the runs. After requirement 2 (review amendment), 20 of 20 runs of `cargo test -p netray-ip --features testing --test contract_status --test evidence` passed.
```

The code fix is sound:
- The old race was real: `FeodoBotnetIps::from_file` on a deleted path loads nothing, and `load_with` sets `feodo_botnet_ips` to `None` (`crates/ip/src/enrichment.rs:176-187`).
- The new path `temp_dir()/netray-ip-contract-{pid}-{call}` comes from a function-local `static AtomicUsize`; `fetch_add` is atomic under `Relaxed`, so every call gets its own directory.
- No other test shares the path; the other `crates/ip/tests` binaries use different prefixes and separate processes.
- The commit carries the `ADLC-Test-Change` trailer.

### Summary

Before and after refutation 0/0/1 (no blocker or major, so no refutation pass). Verified 0, held 0. The minor is accepted: a test that asserts two temp paths differ would test the test; the fix rests on the code reading, as report.md says.

Record note: the review row for 3b163f6..c2d3654 was written with 17 insertions and 4 deletions; the amended range has 16 and 3.
