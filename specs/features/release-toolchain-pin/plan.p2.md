## Phase 2 — Expiry on the sqlx advisory ignore

## Groups
G1: C1, C2, C3, C4, C5, C6, C7

## Plan
### G1
1. deny.toml — change — append `    # Expires: 2027-01-13 (revisit the sqlx cap then).` as the last line of the comment run directly above `"RUSTSEC-2026-0459"` (after the line ending `Revisit when sqlx lifts its cap.`), no blank line before the id; reason lines and the other three entries untouched — hangs on the `[advisories] ignore` entry, deny.toml:23-28, read by `parse_ignores`/`check_ignores`, tests/repo/test_advisories.sh:31/48 (RUSTSEC-2026-0459 arm :79-83)
2. tests/repo/test_advisories.sh, tests/repo/fixtures/advisories/{deny-all-correct,deny-0459-no-sqlx,deny-0459-no-expires,deny-0459-expired}.toml — none — C2 and C5–C7 are already delivered by the test commit cbe9fcb8 (check at :81-82, `EXPECTED_IGNORES` :19 unchanged, fixture cases :190-202); no production change, only verified by `bash tests/repo/test_advisories.sh` exiting 0 (C4) — hangs on `check_ignores`, tests/repo/test_advisories.sh:48
3. (no file) — none — C3 verified by running `cargo deny check advisories` (not covered by `just deny`, justfile:20-21, which checks bans licenses sources only): deny.toml parses and RUSTSEC-2026-0459 is reported ignored, not failed — hangs on nothing existing
