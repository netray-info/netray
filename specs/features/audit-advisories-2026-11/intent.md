# Intent: audit-advisories-2026-11

## Problem

On 2026-11-14, `cargo deny check advisories` fails with 11 errors and `npm audit` reports 3 vulnerable dev packages. Most have a fixed release within the current constraints. The rest are an unfixable SQLite advisory and two unmaintained crates that `deny.toml` does not cover.

## Proposed outcome

`cargo deny check advisories` passes and full `npm audit` reports 0 vulnerabilities. Every remaining ignore carries a comment naming its dependency path and why it is unreachable, and `tests/repo/test_advisories.sh` pins the new state offline.

## Affected users and systems

The netray workspace's `Cargo.lock`, `deny.toml`, the islands' `package.json` and `package-lock.json`, and `tests/repo/test_advisories.sh`. Users see no change.

## Constraints

Rust 1.88 (`rust-version`). Advisory scans stay out of the gate (workflow-rules R-J6).

## Open questions

None. The spec's decision log covers them.
