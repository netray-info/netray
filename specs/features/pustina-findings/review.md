
## d2dc7674..55ce9f67

### Reader

COUNTS blockers=0 majors=0 minors=1
LENSES Engineering, Testing

MINOR | specs/sdd/v2.md:690 | The amended card paragraph says the grades line lists "the protocols present in the run". The card draws only the sections `score_run` scored, so a measured section with an unmeasured weighted check is missing from the card even though the run contains it. | Fetch `/og/example.com.png` for the first time (no stored head), for a run that fails `tls.not_expired` (a hard fail in profile v5) while `http.https_redirect` (weight 5) is unmeasured after a port-80 timeout. `score.hard_fail_triggered` gives the run card a plate (og.rs:164). `score_section` returns `None` for HTTP (scoring.rs:57), so `layout.grades` (plate.rs:45) has no HTTP entry. The card then reads "DNS … · TLS … · Email … · IP …" without HTTP, although the run measured that section. (Reported as a NIT in reader.p1.2.md; still open.)

Traced and found sound:
- DNS checks:
  - `check_dnssec_anchored` adds the not-anchored failure only when DNSKEY records are present and DS is empty.
  - A failed DS batch, whether from the name or from the zone climb, still makes `dns.dnssec` and `dns.dnssec_rollover` unmeasured through `CHECK_INPUTS`, which run before the new remap in `translate`.
  - The `ksk_tags.len() == 1 && ds_records.is_empty()` guard runs on deduplicated records (`unique_records`), so two resolvers returning the same KSK do not produce two tags.
  - The rollover remap to not_applicable only receives a NotFound that DNSKEY-absent handling (`dnssec_absent`) has not already consumed.
- Result page, unchanged re-check:
  - The `unchanged` flag in `stored_shown` matches `HistoryStore::record_timed`: `last_seen_at` moves only on `Identical`, and an insert binds both columns to one `now`.
  - The flag drives the `first`, `since`, diff-note and run-title branches correctly for one entry and for several entries.
  - It covers both the stream finish and a later GET.
- Pass rows: the `If it failed:` body and the guide and evidence links in `passed_checks_with`. The stored page uses entry links and live pages use `#ev-`/`#evidence-`, as attention rows do.
- OG cards:
  - Plate grades are placed in absolute coordinates 500/385.
  - `CARD_RENDERING_VERSION` is in both ETags.
  - The light grade-letter tokens are used.
  - The plateless card is left only for `?`/`incomplete`.
- Header logo: the partial reproduces logo.svg's geometry and takes its colour from tokens.
- Catalogue: the `checks` field is validated, and the strongest pass-branch sentences hold against their checks' code (`tls.tls_version`, `http.hsts`, `http.cors`, `http.referrer_policy`, `email.dmarc`, `email.mta_sts`, `tls.expiry_window`, `tls.aia_reachability`, `ip.reputation`).

```quote specs/sdd/v2.md:690
protocol grades (`DNS A+ · TLS A+ · …`, the protocols present in the run), and the card draws above them the plate of `spectrum::layout` (V4.4) for the entry's run over its
```

```quote crates/web/src/pages/og.rs:164
    if score.complete || score.hard_fail_triggered {
```

```quote crates/engine/src/scoring.rs:57
            Status::Unmeasured => return None,
```

```quote crates/web/src/plate.rs:45
    layout.grades = score_run(run, profile)
```
### Spec reader

COUNTS blockers=0 majors=0 minors=3
REQUIREMENTS met=7 partial=0 missed=0 contradicted=0
MINOR | crates/web/src/pages/og.rs:162 | `/og/<domain>.png` after an admitted scored run now draws the run's full spectrum plate, where the spec's decision log keeps that card plate-less and only adds the protocol grades | unrequested; R7 and decision log line 90: an admitted complete run of a domain with no head entry previously returned `Card::new(...)` with `plate: None`, now `run_card.plate = Some(card_plate(&card_layout(&published.run, None)))`
MINOR | crates/web/src/entries/routes.rs:145 | both OG ETags change format to carry a card rendering version (`"<id>-<letter>-c2"`), an output the spec never names | unrequested; GET `/r/<id>/og.png` for a grade-A entry answered `ETag: "<id>-A"` before the range and `"<id>-A-c2"` at END; `/og/<domain>.png` hashes the version into its ETag too (`crates/web/src/pages/og.rs:45`)
MINOR | crates/engine/src/catalogue.rs:23 | the new `checks` field is serialised into API v1: `GET /api/v1/catalogue` returns it and the OpenAPI document lists it as a required `CheckSpec` property, a public contract change the spec never names | unrequested; R6 asks for a catalogue field for pass rows, the range also adds `"checks"` to `CheckSpec.required` in `tests/fixtures/contracts/api-v1.openapi.json:763`

Rule 3 applies: `specs/features/pustina-findings/bug.md` exists, so Phase 1's shape (Rules 1 and 2) is not judged.

#### Anchors

Finding 1, the admitted-run card's plate:

```quote crates/web/src/pages/og.rs:162
    let mut run_card = Card::new(&score.grade, score.overall_percentage, SystemTime::now());
    // A scored run (complete, or graded by a hard fail) shows its plate; an unscored one stays `?`.
    if score.complete || score.hard_fail_triggered {
        run_card.plate = Some(card_plate(&card_layout(&published.run, None)));
    }
```

```quote specs/features/pustina-findings/spec.md:90
- Unchanged re-check of a one-entry domain drops the first-state note too (R5); the plain `/og/<domain>.png` card without a plate shows the protocol grades like the entry card (R7); the card's grade letter and the logo glyph size take tokens valid for the light card and every viewport.
```

```quote specs/features/pustina-findings/spec.md:51
7. The social card `/r/<id>/og.png` and `/og/<domain>.png` use the light tokens for background,
   text and the spectrum strip, and the former score slot shows the protocol grades
   (`DNS A+ · TLS A+ · …`) of the run. `specs/sdd/v2.md` §card text is amended to match.
```

Finding 2, the ETag format:

```quote crates/web/src/entries/routes.rs:145
    let etag = format!(
        "\"{id}-{letter}-c{}\"",
        crate::og::layout::CARD_RENDERING_VERSION
    );
```

```quote crates/web/src/pages/og.rs:45
    let mut input = format!(
        "{}\x00{domain}\x00{grade}\x00{label}\x00{score_pct:.1}",
        crate::og::layout::CARD_RENDERING_VERSION
    );
```

Finding 3, the API contract:

```quote crates/engine/src/catalogue.rs:17
#[derive(Debug, Clone, PartialEq, Serialize, utoipa::ToSchema)]
pub struct CheckSpec {
    pub id: CheckId,
    pub protocol: Protocol,
    pub title: String,
    /// One positive sentence: what the check verifies.
    pub checks: String,
```

```quote tests/fixtures/contracts/api-v1.openapi.json:763
          "checks": {
            "description": "One positive sentence: what the check verifies.",
            "type": "string"
          },
```

#### Requirements traced (met)

- R1: walked each of the loop's six conditions through the code at END. With a lone KSK and an empty DS set, `dns.dnssec` fails and `dns.dnssec_rollover` becomes `Skip`, which maps to `not_applicable`. The header carries no `<img src="/static/logo.svg"`. A head with `last_seen_at > created_at` sets `unchanged`, so `first_run` is `None`. Pass rows carry `/guide/...` inside `details.clean-bar`. The card background is `COLOR_LIGHT_BG`. The live run against a signed zone without DS is still pending (report C1).
- R2: `check_dnssec_anchored` (`crates/dns/src/api/check.rs:1198`) adds a `Failed` result reading "signed but not anchored in its parent zone" when there are DNSKEY records and no DS records. With no DNSKEY it adds nothing, and with a matching DS mhost's outcome stands. A failed DNSKEY or DS lookup stays `unmeasured` through `CHECK_INPUTS`.
- R3: `ksk_tags.len() == 1 && ds_records.is_empty()` returns `NotFound`, and `translate` maps that to `Skip`/`not_applicable` (`crates/dns/src/module.rs:827`). With several KSKs or at least one DS the per-KSK warning stays. The orphaned-DS `Failed` branch is unchanged.
- R4: `base.html:18` includes `partials/header_logo.html`, an inline `<svg fill="currentColor">` styled with `color: var(--color-ink)`. That token switches under both `prefers-color-scheme` and `[data-theme]` (`build.rs:160`). No template references `/static/logo.svg`; `gone.html` drops the header entirely.
- R5: `stored_shown` sets `unchanged` on `Recorded::Identical`, or on a stored view whose `last_seen_at > created_at`. The run title then reads "unchanged since <created_at> · last checked <last_seen_at>", the panel note carries both dates, and `first` is false. A `Recorded::First` still shows "First run recorded.".
- R6: all 57 `[[check]]` tables carry `checks`. `field()` refuses a missing or empty one, and `catalogue_checks_text.rs` enforces one sentence. `passed_checks_with` writes the body `{checks} If it failed: {impact}` followed by any findings, and sets `guide_href = spec.guide` and `evidence_href = evidence_href(check, links)` the same way `issue_row` does. A stored page passes `Links::Entry(&entry.id)`.
- R7: `og/layout.rs` and `plate.rs` use `COLOR_LIGHT_*` and `STATUS_LIGHT_*`. `card_plate` writes "DNS A+ · TLS A+ · …" at the former score baseline, and the percentage text is gone. The SDD §4.6 and §5 card text is amended.

### Refuted

none: no BLOCKER or MAJOR finding to refute.

### Calibration

none: the refuter was not called.

### Roll call

P03 | absence — sg/rg for *Status/*Verdict enums and codenames in changed templates/src/static — no added enum declarations and no added lens/prism/tlsight/spectra/beacon in crates/web/{templates,src,static} (git diff -U0 range)
P10 | convergence — followed changed scoring paths: dns check_dnssec_anchored and rollover NotFound->Skip — crates/dns/src/api/check.rs:1196 adds a Failed for DNSKEY without DS (a failed DS lookup also fails, never passes); crates/dns/src/module.rs:825 Skip only for a lone KSK, whose missing anchor dns.dnssec fails
P12 | absence — sg for reqwest builders and engine module entry calls in range — no reqwest client or engine entry call added or changed
P13 | absence — read limiter setup; new routes attached to a limiter — no new route; crates/web/src/entries/routes.rs:145 only changes the ETag of an existing route
P18 | absence — sg for reqwest::Client constructions outside fetch.rs; test_config_strict.sh — no Client construction and no config struct added in range; tests/repo/test_config_strict.sh not run
P26 | convergence — rg for new check IDs and second fetches — no new check ID in the range (catalogue diff adds only `checks` texts); the DS/DNSKEY facts are read from the existing dns Lookups (check.rs:1196), no new lookup
P35 | absence — new or changed rule in specs/rules must arrive with a tests/repo check — git diff --stat shows no change under specs/rules or tests/repo
P36 | convergence — git diff --stat for generated files against source — crates/web/tokens.json (source) changed with the consuming pages.css and templates; no generated CSS or sitemap file edited by hand; tests/fixtures/contracts/api-v1.openapi.json changes alongside the dns/web source
P40 | absence — sg for module crate paths in the engine; diff of module add/remove — range adds or removes no module; crates/engine/src/catalogue.rs changes only 4 lines and no module crate path is added

### Summary

- Reader: 0 blockers, 0 majors, 1 minor (before and after refutation).
- Spec reader: 0 blockers, 0 majors, 3 minors; REQUIREMENTS met=7 partial=0 missed=0 contradicted=0.
- Refutation: not run (no BLOCKER or MAJOR); verified 0, held 0.
- Roll call: answered 9, convergence 3, divergence 0, absence 6, unproven 0.
- Four MINORs recorded as deferred rows.
