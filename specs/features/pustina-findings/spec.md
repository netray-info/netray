# Spec: pustina-findings

Status: Ready for Implementation
Created: 2026-11-01

## Goal

A signed zone without a parent DS fails `dns.dnssec` and no longer reads as a key rollover; the header
logo follows the page theme; an unchanged re-check says "unchanged since" instead of "first run";
every passing line explains what it verified, with guide and evidence links; the social card is
light and carries the protocol grades. The loop in `bug.md` exits 0.

## Non-goals

- Defect: `email.dnsbl` stays `pass` when some zones answer and Spamhaus refuses the resolver (by design: other zones answered).
- Defect: No IPv6 egress on the production host; the TLS address row stays `NOT_TESTED_FROM_HERE` (infrastructure, pdt-public).
- No run-specific pass notes from the modules (e.g. "DNSKEY signed (tag …)"); pass lines carry catalogue text only.
- Unchanged re-checks still add no new history entry (SDD v2 R10 stands).

## Context and constraints

Rust workspace, `just adlc-verify` is the gate. Check copy lives only in the catalogue
`crates/engine/catalogue/checks.toml` (SDD v2 R37); its completeness test covers every field.
Colours for OG and badge come from the generated tokens module (`crates/web/tokens.json`). The
page CSP allows inline styles (`crates/web/src/pages/headers.rs:20`). mhost is a dependency (0.12.1)
and is not patched here: the DS rule is added on netray's side of the lint.

## Requirements

1. The loop of `bug.md`, run as `bash` from the repository root against a server built from this
   branch (`BASE=<its origin>`) after one "Re-check now" of a signed domain without parent DS,
   prints no `RED` line and exits 0.
2. `dns.dnssec` fails when the zone publishes DNSKEY records and the parent returns no DS; the
   finding says the zone is signed but not anchored in its parent zone. A zone with matching DS
   still passes; an unsigned zone keeps its current outcome.
3. `dns.dnssec_rollover` warns about a KSK without matching DS only when the zone has more than one
   KSK or the parent returns at least one DS; a single KSK with an empty DS set is `not_applicable`.
   The orphaned-DS failure is unchanged.
4. The header logo is inline SVG whose colour comes from the page's tokens, so it is legible under
   all four combinations of OS scheme (light, dark) and toggle (light, dark); no page references
   `/static/logo.svg` through `<img>`.
5. When the latest recording of a re-check is `Identical` to the stored entry, the run header and
   panel say the domain is unchanged since the entry's `created_at` and was last checked at
   `last_seen_at`; "first run" and "First run recorded." appear only for a domain's genuinely first
   recording.
6. Every catalogue check carries a positive one-sentence `checks` text ("what it verifies"); the
   completeness test fails on a missing or empty one. Each passing row shows that sentence and the
   check's `impact` sentence, introduced as `If it failed:`, as its body (followed by any
   findings), plus `guide →` and `evidence →` links built as for attention rows; on a stored
   page they link the entry's evidence as attention rows do.
7. The social card `/r/<id>/og.png` and `/og/<domain>.png` use the light tokens for background,
   text and the spectrum strip, and the former score slot shows the protocol grades
   (`DNS A+ · TLS A+ · …`) of the run. `specs/sdd/v2.md` §card text is amended to match.

## Phases

## Phase 1 — Six result-page fixes

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6, 7

Lint wrapper for the DS rule and the guarded rollover rule in `crates/dns`; inline logo in
`base.html` with token colour in `pages.css`; the `Identical` branch of the run header in
`pages/result`; the catalogue field, its 57 texts (seeded from V1's `CHECK_DESCRIPTIONS`) and
`passed_checks`; the light card in `og/layout.rs`, `og/render.rs` and `plate.rs::card_plate`.
Demonstrable on a local server: the loop runs green against it.

### Test Scenarios

GIVEN DNSKEY records and an empty DS set WHEN the DNS lints run THEN `dns.dnssec` is fail with the "not anchored in its parent zone" finding and `dns.dnssec_rollover` is not_applicable.
GIVEN DNSKEY records with a matching DS WHEN the DNS lints run THEN `dns.dnssec` is pass and `dns.dnssec_rollover` is pass.
GIVEN two KSKs and a DS matching one WHEN the DNS lints run THEN `dns.dnssec_rollover` warns for the other KSK.
GIVEN any page WHEN it is served THEN its header contains an inline `<svg>` logo and no `<img src="/static/logo.svg">`.
GIVEN a stored entry for a domain WHEN a re-check records `Identical` and the result page is served THEN it says "unchanged since <created_at>" and contains neither "first run" nor "First run recorded.".
GIVEN no stored entry WHEN the first check is recorded THEN the page still shows "First run recorded.".
GIVEN the catalogue WHEN a check lacks `checks` THEN the completeness test fails.
GIVEN a run with passing checks WHEN the result page is served THEN every passing row in the clean list has a non-empty body (the `checks` sentence, then "If it failed:" and the `impact` sentence), a `/guide/` link and an evidence link built as for that page's attention rows.
GIVEN a stored entry WHEN `/r/<id>/og.png` is fetched THEN the top-left pixel is the light background token and the image contains the protocol grades text in its SVG source.

## Decision log

- DS rule in netray around mhost's lint over a change to mhost: a dependency release would gate this fix; recorded in mhost later if wanted.
- H1b refuted: `crates/dns/src/dns_dnssec.rs` already fails on a missing DS but does not feed `dns.dnssec`; not reused as the judge, to keep one lint path.
- H4b refuted: the production store persists across redeploys (entry of 2026-10-28 served on 2026-11-01); no volume change.
- Inline SVG over two `<img>` variants: one asset, follows both `prefers-color-scheme` and `data-theme` through tokens.
- Light card over the specified dark card: operator decision 2026-11-01; the SDD text changes with it.
- New catalogue field over showing `impact` alone: `impact` is phrased as the risk and reads wrong on a pass line (operator decision 2026-11-01).
- One block for all six symptoms: operator decision 2026-11-01.
- Amended after review 2026-11-01 (operator choice A): a bare `impact` sentence on a pass row states the harm as fact, so it is introduced as `If it failed:`; the clean-list scenario covers passing rows only (N/A, not-tested and unmeasured rows keep their reasons); the loop's symptom 2 asserts `dns.dnssec_rollover` is `not_applicable` with an empty DS set instead of grepping one phrase.
- Unchanged re-check of a one-entry domain drops the first-state note too (R5); the plain `/og/<domain>.png` card without a plate shows the protocol grades like the entry card (R7); the card's grade letter and the logo glyph size take tokens valid for the light card and every viewport.

## Open decisions

None.

## Out of scope

- Spamhaus refusing the resolver: resolver choice, `crates/email/src/checks/dnsbl.rs`, not changed.
- IPv6 egress of the production container: `~/Documents/src/pdt-public`.
