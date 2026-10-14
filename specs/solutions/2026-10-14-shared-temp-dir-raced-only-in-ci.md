---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-07-feature-unification-changes-service-behaviour.md
---
# Parallel tests sharing a pid-only temp directory failed only in CI

**What failed.** `crates/ip/tests/evidence.rs` and `crates/ip/tests/contract_status.rs` built their feodo list under `temp_dir()/netray-ip-…-{pid}`, a path every test in one binary shares. One test's `remove_dir_all` could delete another's list before `FeodoBotnetIps::from_file` read it; the module then loaded no list and `ip.reputation` passed. CI run 38523840435 failed with `feodo_botnet_ips: column c2 left Bool(false) right Bool(true)`; locally the race reproduced in 0 of 15 runs.
**What worked.** A function-local `static CALL: AtomicUsize` and `CALL.fetch_add(1, Ordering::Relaxed)` in the directory name, so every call gets its own directory; the review then found the second file with the same pattern.
**How to notice next time.** A test that fails in CI but never locally, and builds a temp path from `std::process::id()` alone.
