---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-09-dkim-test-key-never-parsed.md
---
# A rewritten SSE golden passed through the error path it did not name

**What failed.** `incomplete_c5_email_all_skip_is_incomplete_with_error_section` rewrote `beacon.sse` line by line with `.lines()…join("\n")`, which dropped the stream's final blank line. The `summary` event was never dispatched, the strict SSE collector returned "stream ended without summary", and email went Errored, so the test passed. A well-formed all-`skip` answer actually scores email Pass (buckets start at Pass, `email.rs:362,440`).
**What worked.** The phase reader compared the bytes (`od`) of golden and rewrite; the test was removed and the case moved to R4.3; `specs/rules/testing-rules.md` now asks rewrites to keep the framing.
**How to notice next time.** A test that rewrites a recorded stream and expects an error or degraded result: check that the rewrite still parses as a complete stream.
