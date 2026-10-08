---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-08-csp-check-matched-text-not-source-rules.md
---
# A golden with invented sub-checks let lens's parser bugs pass

**What failed.** The first beacon golden (`tests/fixtures/contracts/beacon.sse`) carried invented sub-checks (`mx_present`, detail `example.com`). lens's contract tests passed while lens took bucket messages from the category status line and read any `mx` fail as "no MX records" — an MX pointing at a CNAME would have scored the email section 100 %.
**What worked.** Goldens rebuilt from beacon's real sub-check names and detail texts (`crates/beacon/src/checks/*.rs`), plus `beacon-no-mx.sse` and `beacon-mx-cname.sse`; lens reads reasons from `sub_checks[].detail` and detects no-MX only on beacon's exported `NO_MX` (commits `700eee2`, `7bc7645`, `3dee309`).
**How to notice next time.** A golden whose identifiers were typed by hand rather than taken from the producer tests the consumer against a format nobody sends.
