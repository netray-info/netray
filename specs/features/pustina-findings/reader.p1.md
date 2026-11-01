### Reader

COUNTS blockers=3 amendments=1 deferred=0 nits=0
LENSES Engineering

BLOCKER | crates/web/src/pages/og.rs:167 | Suppose an admitted run ends incomplete with a hard fail and the domain has no stored head. `/og/<domain>.png` then renders the plateless card with grade F and an empty former score slot. It shows no protocol grades: `card_svg` can only draw grades through a plate, and this change removed the percentage from the plateless card. That contradicts R7 and the decision log ("the plain `/og/<domain>.png` card without a plate shows the protocol grades like the entry card"). | First `/og/example.com.png` for a domain with no history, whose run fails `tls.not_expired` (a hard fail in the profile) while a weighted check such as `email.mx` is unmeasured (MX lookup SERVFAIL). This gives `score.complete=false` and `score.grade="F"`, as `OverallScore` documents. `head_card` is `None`, so `svg_for_grade(…, "F", …, None)` runs. The card shows "F" and nothing at y=385, although `score.sections` holds the DNS and HTTP grades it could show.
BLOCKER | specs/sdd/v2.md:693 | The amended §4.4 card paragraph still says `/og/<domain>.png` draws "a plain card after an admitted run". But og.rs:164 now draws a plate, with its grades line, on every complete admitted run. R7 requires the §card text to be "amended to match", and the root CLAUDE.md requires SDD edits in the same commit as the code. | `/og/example.com.png` with a stale head and a complete admitted run. The response carries `<g class="plate">…<text class="plate__grades">…`, built by `card_layout(&published.run, None)`, where the SDD promises a plain card. Because that plate is drawn over no predecessor, a line fixed since the old head renders as a plain pass. On the next request, served from the head, the same run renders as a fixed ring.
BLOCKER | crates/engine/catalogue/checks.toml:123 | The `dns.https_svcb` pass sentence says the record "announces its HTTP/3 and TLS capabilities". The check verifies no such thing: mhost 0.12.1 `check_svcb_records` passes any HTTPS/SVCB set that neither mixes alias and service mode nor holds several alias records, and never reads SvcParams. So the pass row claims a fact nobody checked (R6 "what it verifies"; root CLAUDE.md: read the check before writing its catalogue text). | `example.com. HTTPS 0 cdn.example.net.` (alias mode, no SvcParams) or `example.com. HTTPS 1 . alpn=h2` gives `CheckResult::Ok("HTTPS records are well-formed …")` and `dns.https_svcb` pass. The clean-list row then reads "The domain publishes a well-formed HTTPS or SVCB record that announces its HTTP/3 and TLS capabilities."
AMENDMENT | specs/features/pustina-findings/spec.md:77 | The pass-row scenario demands "an evidence link that resolves on that page". Amended R6 (spec.md:50) has a stored page link the entry's evidence as attention rows do: `/r/<id>/<protocol>#ev-<block>`, which is another page. On a stored result page the scenario and the requirement cannot both hold. | GET `/example.com` with a stored head. `stored_shown` builds pass rows with `Links::Entry(&entry.id)` (mod.rs:1073), so the `dns.dnssec` pass row's `evidence →` is `/r/<id>/dns#ev-dns.dnskey`. It does not resolve on `/example.com`, even though that page renders its own evidence panels. | affected_phase: 1 | repaired_in_phase: no

Traced and found sound: the DNS anchor wrapper and the guarded rollover rule through `translate` (lone KSK with empty DS gives not_applicable; failed DS lookup stays unmeasured; zone climb reads DS at the zone). Also the `unchanged` derivation in `stored_shown` against `HistoryStore::record_timed` (`last_seen_at` moves only on `Identical`; insert binds both columns to one `now`); the `If it failed:` body and links in `passed_checks_with`; the catalogue `checks` field validation; the light tokens and grades placement in the card SVG (absolute 500/385); the inline logo geometry, which measures 70 x 40 px in headless Chromium at `height: 2.5rem`.

```quote crates/web/src/pages/og.rs:167
    let card = head_card.as_ref().unwrap_or(&run_card);
```

```quote specs/features/pustina-findings/spec.md:90
- Unchanged re-check of a one-entry domain drops the first-state note too (R5); the plain `/og/<domain>.png` card without a plate shows the protocol grades like the entry card (R7); the card's grade letter and the logo glyph size take tokens valid for the light card and every viewport.
```

```quote specs/sdd/v2.md:693
entry, and a plain card after an admitted run (an admitted run is no entry yet).
```

```quote crates/web/src/pages/og.rs:164
        run_card.plate = Some(card_plate(&card_layout(&published.run, None)));
```

```quote crates/engine/catalogue/checks.toml:123
checks = "The domain publishes a well-formed HTTPS or SVCB record that announces its HTTP/3 and TLS capabilities."
```

```quote specs/features/pustina-findings/spec.md:77
GIVEN a run with passing checks WHEN the result page is served THEN every passing row in the clean list has a non-empty body (the `checks` sentence, then "If it failed:" and the `impact` sentence), a `/guide/` link and an evidence link that resolves on that page.
```

```quote specs/features/pustina-findings/spec.md:50
   page they link the entry's evidence as attention rows do.
```

```quote crates/web/src/pages/result/mod.rs:1073
        passed: views::passed_checks_with(&entry.run, cat, &Links::Entry(&entry.id)),
```
