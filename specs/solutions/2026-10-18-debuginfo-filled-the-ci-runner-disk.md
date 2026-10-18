---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-14-shared-temp-dir-raced-only-in-ci.md
---
# Full debuginfo in two test-binary trees filled the CI runner disk

**What failed.** After PR #27, CI run 39131412716 died in `just check` with `/usr/bin/ld: final link failed: No space left on device` linking lens test binaries. The job builds the workspace tests twice (`adlc-verify`'s offline suite, then `test-rust`'s `cargo test --workspace` with GeoIP data, justfile:61), every Linux test binary carrying full debuginfo; locally on macOS (debuginfo in object files, ample disk) nothing showed.
**What worked.** Job env `CARGO_PROFILE_DEV_DEBUG: line-tables-only` plus a setup step removing `/usr/share/dotnet` (6 GB on the arm image; `/opt/ghc` and Android are absent there). Run 39143803077: green in 25m51s, no ENOSPC, ifconfig-rs data tests passed (`ok_handlers` 113).
**How to notice next time.** A CI step fails in the linker or writer with ENOSPC after a change that adds test files or dependencies, not one that touches the workflow.
