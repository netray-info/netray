---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-21-nothing-lost-needles-matched-other-text.md
---
# A merge dropped one CSS brace and every gate stayed green

**What failed.** Merging main (v2-ip-ui) into feature/v2-dns-ui conflicted in
`crates/web/static/pages.css`: both sides appended rules at the end. The diff3 resolution kept
both halves but the shared trailing `}` went to only one of them, leaving `.dns-level__record {
display: block;` open. Every later rule (all of v2-ip-ui's and the feature's newer ones) parsed
as nested inside it and the browser dropped them. `adlc verify`, the CSS gate and
`just design-check` (whose boards measure no DNS or IP page) all passed; the bug showed only
when a screenshot ignored new grid rules and `getComputedStyle` still read `display: flex`.
**What worked.** Restoring the brace, and `crates/web/tests/it/static_css_balanced.rs`, which
fails when any stylesheet under `static/` leaves a block open or opens a margin-0 rule inside one.
**How to notice next time.** After resolving a stylesheet conflict, count braces
(`python3 -c "s=open(p).read();print(s.count('{'),s.count('}'))"`) before trusting a green gate.
**Check to build.** Built in this block: `static_css_balanced.rs`.
