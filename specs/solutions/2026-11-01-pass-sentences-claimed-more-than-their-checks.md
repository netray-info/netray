---
class: verdict-broader-than-the-check
repeat-of: specs/solutions/2026-10-18-compare-row-claimed-more-than-its-checks.md
---
# Pass-row sentences seeded from V1 claimed more than their checks verify

**What failed.** pustina-findings added a catalogue `checks` sentence for every check, shown on passing rows, seeded from V1's `CHECK_DESCRIPTIONS`. Two readers in a row found pass rows asserting what the pass branch does not guarantee (`tls.caa_compliant` "authorised by the domain's CAA records" while no CAA records pass; `http.referrer_policy` "does not leak full URLs" while `no-referrer-when-downgrade` passes; `email.mx` "more than one server" while one dual-stack MX passes). An audit against each pass branch rewrote 40 of 57.
**What worked.** For every check, read the code that sets its status and write only what a pass guarantees, citing the line (the audit table in the finish of pustina-findings, commit 55ce9f67).
**How to notice next time.** A copy text written from a description or an older product, not from the check's own pass and fail branches.
