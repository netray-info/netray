---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-20-score-rounding-tests-carried-no-float-error.md
---
# A nothing-lost test passed with a value missing from the view

**What failed.** `crates/web/tests/it/views_nothing_lost.rs` looked for each value's display text in
the rendered view. The CSP `issue_count` needle was the bare "2", which the escaped issue text
`&#60;object&#62;` already contains; the HSTS flags were both `true`, so one "yes" covered both
cells. Deleting the count line or the preload cell left the test green (final review, v2-http-ui).

**What worked.** Needles in the view's own words ("2 issues") and fixture values that render to
distinct texts (`include_subdomains` true, `preload` false).

**How to notice next time.** A needle that is a short number or a word another value of the same
fixture renders too.

**Check to build.** In `views_nothing_lost.rs`, assert the needles of one row are pairwise distinct
and none occurs in the fragment more often than the fixture holds it.
