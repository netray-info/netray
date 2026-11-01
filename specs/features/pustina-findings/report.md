# Report: pustina-findings

## Phase 1 — Six result-page fixes

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: the loop of `bug.md` against a server built from this branch prints no `RED` line and exits 0 | test-unwritable | — (needs a live server, network and a signed zone without DS; run as behavioural verification) |
| C2 | R2: `dns.dnssec` fails when DNSKEY is published and the parent returns no DS (finding: signed but not anchored in its parent zone); matching DS passes; unsigned keeps its outcome | green | crates/dns/tests/it/dnssec_anchor_lints.rs |
| C3 | R3: `dns.dnssec_rollover` warns about a KSK without DS only with more than one KSK or at least one DS; single KSK with empty DS is not_applicable; orphaned-DS failure unchanged | green | crates/dns/tests/it/dnssec_anchor_lints.rs |
| C4 | R4: header logo is inline SVG coloured from page tokens; no page references `/static/logo.svg` through `<img>` | green | crates/web/tests/it/header_inline_logo.rs |
| C5 | R5: an `Identical` re-check says unchanged since `created_at`, last checked `last_seen_at`; "first run" only for a genuinely first recording | green | crates/web/tests/it/unchanged_recheck.rs |
| C6 | R6: every catalogue check carries a non-empty `checks` text (completeness test fails otherwise); pass rows show `checks`, then `If it failed:` + `impact`, as body, plus guide and evidence links (entry evidence on a stored page) | green | crates/engine/tests/it/catalogue_checks_text.rs, crates/web/tests/it/result_views_pass_rows.rs |
| C7 | R7: `/r/<id>/og.png` and `/og/<domain>.png` use light tokens and show the protocol grades; SDD card text amended | green | crates/web/tests/it/og_light_card.rs |
| C8 | GIVEN DNSKEY and empty DS WHEN lints run THEN `dns.dnssec` fail "not anchored in its parent zone" and `dns.dnssec_rollover` not_applicable | green | crates/dns/tests/it/dnssec_anchor_lints.rs |
| C9 | GIVEN DNSKEY with matching DS WHEN lints run THEN `dns.dnssec` pass and `dns.dnssec_rollover` pass | green | crates/dns/tests/it/dnssec_anchor_lints.rs |
| C10 | GIVEN two KSKs and a DS matching one WHEN lints run THEN `dns.dnssec_rollover` warns for the other KSK | green | crates/dns/tests/it/dnssec_anchor_lints.rs |
| C11 | GIVEN any page WHEN served THEN header has an inline `<svg>` logo and no `<img src="/static/logo.svg">` | green | crates/web/tests/it/header_inline_logo.rs |
| C12 | GIVEN a stored entry WHEN a re-check records `Identical` and the page is served THEN "unchanged since <created_at>", neither "first run" nor "First run recorded." | green | crates/web/tests/it/unchanged_recheck.rs |
| C13 | GIVEN no stored entry WHEN the first check is recorded THEN the page shows "First run recorded." | green | crates/web/tests/it/unchanged_recheck.rs |
| C14 | GIVEN the catalogue WHEN a check lacks `checks` THEN the completeness test fails | green | crates/engine/tests/it/catalogue_checks_text.rs |
| C15 | GIVEN a run with passing checks WHEN the page is served THEN every passing clean-list row has a non-empty body (`checks`, then "If it failed:" + `impact`), a `/guide/` link and an evidence link that resolves on that page | green | crates/web/tests/it/result_views_pass_rows.rs |
| C16 | GIVEN a stored entry WHEN `/r/<id>/og.png` is fetched THEN top-left pixel is the light background token and the SVG source contains the protocol grades text | green | crates/web/tests/it/og_light_card.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C2, C3, C8, C9, C10) | 2 | sonnet | 51769 | 60 |
| G2 (C4, C11) | 1 | sonnet | 56790 | 73 |
| G3 (C14, C6) | 1 | sonnet | 58919 | 76 |
| G4 (C5, C6, C12, C13, C15) | 1 | sonnet | 66724 | 82 |
| G5 (C7, C16) | 1 | sonnet | 64718 | 105 |
| G6 (C1) | — | behavioural verification | — | — |

G1's coder filtered mhost's orphaned-DS failure out of `dns.dnssec` to satisfy a wrong test row; the orchestrator removed the filter and corrected the row (42805ba7, `ADLC-Test-Change`). Existing tests the change moved were updated in d488bdff, each under `ADLC-Test-Change`.

### Runs (resume after the blocker halt, operator choice A)

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| first-review blockers (rollover status, stored pass-row evidence, OG ETag) | 1 | sonnet | 56953 | 459 |
| second-review blockers (FIRST_STATE, If it failed:, plain card, grade contrast, logo token) | 1 | sonnet | 80722 | 496 |

Spec amended in 7d82c9cc and ee6a5070 (R6 `If it failed:`, pass-row scenario scope and link form, loop symptom 2 by status).
The resumed reader's blockers (hard-fail card without grades, SDD §4.4 wording, `dns.https_svcb` sentence) were fixed by the orchestrator; its second pass found three overclaiming `checks` sentences, and an audit of all 57 against their pass branches rewrote 40 (deviation from the halt rule: copy-only fixes under the operator's "continue autonomously").

### Reader

- specs/features/pustina-findings/reader.p1.md: 3 blockers (fixed), 1 amendment (repaired in ee6a5070).
- specs/features/pustina-findings/reader.p1.2.md: 3 blockers (catalogue copy, fixed by the audit), 4 nits (two copy nits fixed by the audit; the plate-grade guard and the SDD "present" wording listed, not acted on).

### Behavioural verification

Local `netray serve` (`crates/netray/netray.dev.toml`, seeded `tests/acceptance/fixtures/history`, apex localhost:18731):

```
$ BASE=http://localhost:18731 DOMAIN=example.com bash loop.sh; echo exit $?
exit 0
$ BASE=http://localhost:18731 DOMAIN=shop.example.com bash loop.sh; echo exit $?
exit 0
svg logo: 1  img logo: 0
clean-list guide links: 37  'If it failed:' 37
etag: "Txi4I9hv-B-c2"
og top-left rgb [247, 248, 250]
```

Symptoms 1, 2 and 4 need live DNS of a signed zone without DS and a recorded `Identical` re-check; a fresh local binary has no network, so they are covered by `dnssec_anchor_lints`, `dnssec_rollover_status`, `unchanged_recheck` and `result_pass_and_unchanged_copy`. C1 (the loop against production) is checked after deploy.

### Checks to build

none: overclaiming copy needs a reading of each check's pass branch; no fixed pattern catches it.
