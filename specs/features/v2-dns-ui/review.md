# Review: v2-dns-ui

## f8ecf55d..42c11af2

### Reader

COUNTS blockers=1 majors=0 minors=2
LENSES Engineering, Security, Testing
BLOCKER | crates/web/src/api/jobs.rs:825 | `trace_note` picks the type names out of the raw input, while the trace runs `parsed.record_types.first()`. The parser drops duplicates and resolves aliases, so the note is wrong both ways: it can appear when it should not, be missing when it should appear, or name the wrong type. | `example.com MX MX +trace`: the parser yields one type (MX), yet the stream carries "Only the first type is traced: MX.". `example.com KEY MX TXT +trace`: the parser yields [DNSKEY, MX, TXT] and the trace runs DNSKEY, but the note says "traced: MX". `KEY` and `SIG` are mhost aliases and `NULL` is excluded from `record_types()`, so none of them count as known names. `example.com NULL MX +trace` traces NULL and shows no note at all.
```quote crates/web/src/api/jobs.rs:825
    let named: Vec<String> = tokens
```
MINOR | crates/web/src/api/jobs.rs:845 | Replaying a cached trace rebuilds the note from the stored query, and `trace_note` gives up when that query has no `+trace` token. A trace stored by the API's GET route therefore replays without the note, even when the query names several types. | `GET /api/v1/dns/trace?q=example.com%20MX%20TXT` traces MX and caches query "example.com MX TXT" with mode "trace". `/inspect/dns?r=<key>` then shows the MX trace with no "Only the first type is traced: MX.". This is MINOR only because the route documents q as "a domain, then a record type".
```quote crates/web/src/api/jobs.rs:845
                    notes.extend(trace_note(&cached.query));
```
MINOR | crates/web/tests/it/dns_view_review4.rs:91 | This test cannot fail for the behaviour it names: it accepts either "answer, no records" or "answer". Deleting the `"answer" if answers.is_empty()` branch in `crates/web/src/pages/views/dns.rs` leaves it green, and no other test asserts "answer, no records". | Remove the arm `"answer" if answers.is_empty() => "answer, no records".to_string(),`: a records-less authoritative answer then renders "answer", and the test still passes through its second alternative.
```quote crates/web/tests/it/dns_view_review4.rs:91
            || s.contains(r#"<span class="dns-hop__outcome">answer</span>"#),
```

### Spec reader

COUNTS blockers=0 majors=2 minors=0
REQUIREMENTS met=18 partial=2 missed=0 contradicted=0
MAJOR | crates/web/static/pages.css:1608 | Compact still hides record details while a server filter is on, but the spec says Compact does not act while a filter is on. | Req 11, case: `data-compact` and `data-server="<server>"` both on the region. The `:not([data-server])` guard covers only the group rule on line 1607. The details rule on line 1608 has no guard and still matches, so every record's `<dl>` is hidden under the filter.

```quote specs/features/v2-dns-ui/spec.md:152
    empty" (hides `no_records` and `nxdomain` groups), "Compact" (hides groups past the first
```

```quote crates/web/static/pages.css:1608
.dns-view[data-compact] .dns-record__details {
```

MAJOR | crates/web/templates/partials/dns_view.html:15 | The SOA serial age is shown only on the row (`<summary>`). The disclosed details body (`<dl>`: TTL and Servers) does not list it, but the spec lists it among what the details disclose. | Req 13, case: an SOA record with `soa_serial_age_days` 4. `RecordView.serial` = "serial 2026101501 · 4 days old" is rendered only inside `<summary>`. When the details are opened, the `<dl class="dns-record__details">` on line 16 holds only the TTL and Servers rows, with no serial row.

```quote specs/features/v2-dns-ui/spec.md:162
13. **Record details.** Each record row is a `<details>` disclosing: TTL in human units with the
```

Note on the second finding: the information is visible whether the details are open or closed. The gap is only against the spec's wording. Traced and found met: the summary and server blocks plus the API-cache replay; the part hook and the claim rules; the summary line, badges, groups, bars and tab badges; the server filter, toggles and storage; the trace and DNSSEC views and the "Only the first type is traced" note; the parse autocomplete; the exports; the no-JS markup; the budget test; and the Phase 6 rows. No unrequested behaviour found.

### Refuted

None: the refuter confirmed all three BLOCKER and MAJOR findings (confidence 8, 8, 7).

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| jobs.rs:825 trace_note counts raw tokens | CONFIRMED | 8 | held | `curl -X POST --data-urlencode 'q=example.com MX MX +trace' …/inspect/dns/run` on the stub server streams "Only the first type is traced: MX."; `example.com KEY MX TXT +trace` names MX while the trace runs DNSKEY |
| pages.css:1608 Compact hides details under a server filter | CONFIRMED | 8 | held | read: the details selector lacks `:not([data-server])` |
| dns_view.html:15 serial age not in the details | CONFIRMED | 7 | held | read: the `<dl>` holds TTL and Servers only |

### Summary

Before refutation: reader 1/0/2, spec reader 0/2/0 (REQUIREMENTS met=18 partial=2 missed=0
contradicted=0). After refutation: 1 blocker, 2 majors, 2 minors; verified 3, held 3. No roll
call: `adlc.toml` declares no principles with sentences.

### Fixes

Decision (implementer under the operator's autonomy mandate): fix on this branch. The blocker
and both majors are fixed with tests in `crates/web/tests/it/dns_view_review6.rs`; the API-route
replay minor is fixed (`types_note` for cached traces); the weak test in `dns_view_review4.rs`
now asserts "answer, no records" over a NODATA type.

## 42c11af2..89c0fbe4

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/web/src/api/jobs.rs:840 | No test covers the change to replaying a trace from the cache, so a regression back to `trace_note` would go unnoticed. | Store a trace with `GET /api/v1/dns/trace?q=example.com%20MX%20TXT`, then open `/inspect/dns?r=<key>`: the new code shows "Only the first type is traced: MX."; no replay test runs a trace.
MINOR | crates/web/tests/it/dns_view_review6.rs:80 | The new tests went in together with the fix, with no earlier failing-test commit carrying `ADLC-Baseline`, so the tamper check does not protect them on this branch until merge. | 89c0fbe4 has no `ADLC-Baseline` trailer; all three tests fail against the parent code.

```quote crates/web/src/api/jobs.rs:840
                    notes.extend(types_note(&cached.query));
```

```quote crates/web/tests/it/dns_view_review6.rs:80
async fn the_note_counts_the_types_the_parser_reads() {
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=3 partial=0 missed=0 contradicted=0

Requirements 11, 13 and 15 traced: the note names the type the trace runs (`example.com KEY MX TXT +trace` names DNSKEY; `MX MX` gives none); Compact leaves groups and details alone under a server filter; the SOA serial age is in the row and in the details. Outside the requirements: `named_record_types` sits between `parse_record_type` and its doc comment.

### Refuted

None: no BLOCKER or MAJOR finding to refute.

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename or new status enum in added web source or templates |
| P10 | absence | presentation only; errors stay visible, the cache replay re-adds them |
| P12 | absence | no outbound call or engine entry added |
| P13 | absence | no limiter or route change |
| P18 | absence | no reqwest client, no config struct touched |
| P26 | convergence | only existing dns block ids consumed; no new check id |
| P35 | absence | no rule changed |
| P36 | convergence | hand-written stylesheets; tokens untouched |
| P40 | absence | no module added or removed |

Read over the whole feature, f8ecf55d..89c0fbe4.

### Summary

Reader 0/0/2, spec reader 0/0/0 (REQUIREMENTS met=3 partial=0 missed=0 contradicted=0), before
and after refutation. verified 0, held 0. Roll call: answered 9, convergence 3, divergence 0,
absence 6.
