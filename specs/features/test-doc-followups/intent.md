# Intent: test-doc-followups

## Problem

Reviews of audit-advisories-2026-11 and release-toolchain-pin deferred findings against the
repository checks and their self-tests: a self-test that cannot fail on half of its assertion, a
floor check with no below-floor fixture, a toolchain check with no root override that its self-test
needs, and a crate doc that names the pinned Rust version but not the check that guards it.

## Proposed outcome

Taken into this feature and closed by a test or doc change:
`audit-advisories-2026-11/review-1`, `audit-advisories-2026-11/review-2`,
`release-toolchain-pin/review-1`, `release-toolchain-pin/review-2`, and the release-toolchain-pin
review findings F1 (no root override in the toolchain check) and F2 (the crate doc does not name
the check). The other rows of those reviews are closed by a recorded decision with its reason.
No production behaviour changes.

## Affected users and systems

Maintainer only. `tests/repo/test_advisories.sh`, `tests/repo/test_toolchain_pin.sh`,
`tests/repo/test_toolchain_pin_selftest.sh`, `crates/common/CLAUDE.md`.

## Constraints

- Protected tests change under `ADLC-Test-Change` trailers.
- No release or tag.
- feature/sqlx-0-9-bump also edits `tests/repo/test_advisories.sh` (removes the 0459 ignore and
  its expiry check): keep edits there narrow.
- audit-advisories-2026-11/review-3 is dropped: resolver-made edge moves, no version change.

## Open questions

- release-toolchain-pin/review-3 and review-4 name check behaviour the spec did not: keep it and
  supersede the old decision, or narrow the check.
