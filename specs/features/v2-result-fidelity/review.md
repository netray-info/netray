# Review: v2-result-fidelity

## 602e5a66..8554424c

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Security, Testing
BLOCKER | crates/web/src/pages/result/mod.rs:628 | The score line rounds the percentage, so whenever overall_percentage lands in [T−0.5, T) for any integer threshold T (97, 90, 75, 60, 40), the page shows the next grade's threshold under the lower letter. That breaks the spec's promise that "the page never shows a number that contradicts the letter" (spec.md:116). | Take a stored, complete profile‑3 run where everything passes except: dns.dnssec fail, dns.caa warn, http.hsts fail, email.dmarc warn, tls.ech_advertised warn. The sections score DNS 78.571, TLS 99.029, HTTP 78.261, Email 90.909, IP 100, so the weighted overall is 89.663. `lookup_grade` gives B because A starts at 90 (v3.toml:94). The stored grade is also B, so the guard lets the score through, and the header renders "B", "score 90 / 100" and "ok, weaknesses worth fixing". In the same way, 96.5–96.99 shows "score 97" under A. Rounding down instead of to nearest can never contradict an integer threshold.

```quote crates/web/src/pages/result/mod.rs:628
    Some(score.overall_percentage.round().clamp(0.0, 100.0) as u8)
```

```quote crates/engine/profiles/v3.toml:94
"A"  = 90
```

```quote specs/features/v2-result-fidelity/spec.md:116
   page never shows a number that contradicts the letter; such a mismatch logs a warning naming the
```

### Spec reader

COUNTS blockers=0 majors=3 minors=2
REQUIREMENTS met=40 partial=3 missed=0 contradicted=0
MAJOR | crates/web/static/pages.css:299 | The hard-fail banner gets fail-coloured bold text, but nothing draws the 3 px fail-tone bar the requirement asks for. | R11: a stored run with `tls.hostname_match` failed renders `<section id="hard-fail" class="hard-fail">`. The only rules for it are `.hard-fail` (margin, colour, weight) and `.hard-fail__without`. There is no border, no `::before` and no section-wide rule, so no bar is drawn.
MAJOR | crates/web/static/trace.css:83 | At phone width the trace's newest and oldest edge dates are set to `display: none`. R20 covers the phone boards 27/28 and has no phone exception for those dates. | R20 and R39: at 390 px with 12 entries the trace draws its 6 px rows, but `.trace__edges` is hidden, so "10-08" / "07-12" never show beside it. Neither the Decision log nor the operator proposals mention this.
MAJOR | specs/features/v2-result-fidelity/report.md:400 | Three visual-reading findings are marked closed as "proposal for the operator". They were neither fixed nor accepted by an operator decision. | R43: V3 (the board's evidence summary caption is missing), V4 (evidence blocks in the wrong order) and V14 (values wrap mid-token, swipe note far from the table) stay real deviations. The spec meanwhile says "Open decisions: None".
MINOR | crates/web/templates/partials/result_evidence.html:7 | "← swipe the table" appears in every panel that has at least one block, whether or not it holds a table, let alone a too-wide one. | R35: at 390 px a panel whose blocks are only `Fields`/`Chain`/`Text` shapes, with no `<table>`, still shows the swipe note.
MINOR | crates/web/src/pages/result/mod.rs:1039 | Unstored and unrecorded runs with no earlier entry get an h2 "This run" over the diff box. No requirement and no Decision-log line names this heading. | R12 lists the section h2s as "Since run <k−1>" or the first-run heading. Same code at mod.rs:1075. Example: a finished run of a new domain that was not stored (`not_stored_shown` with `entries` empty).

```quote specs/features/v2-result-fidelity/spec.md:140
11. **Hard-fail banner.** Between the result header and the spectrum, when the run has failed
```

```quote crates/web/static/pages.css:299
.hard-fail {
```

```quote specs/features/v2-result-fidelity/spec.md:180
20. **Trace form.** Under the spectrum each line carries its trace: one row per stored entry,
```

```quote crates/web/static/trace.css:82
@media (max-width: 480px) {
```

```quote specs/features/v2-result-fidelity/spec.md:300
43. **Visual reading.** A reader with no part in the build compares every side-by-side image the
```

```quote specs/features/v2-result-fidelity/report.md:400
| V3 | visual reading: F, FFirst evidence panel without the board's one-line summary caption | green | visual reading (proposal for the operator: evidence tab content) |
```

```quote specs/features/v2-result-fidelity/report.md:411
| V14 | visual reading: M key-value values wrap mid-token, swipe note far from the table | green | visual reading (proposal for the operator: evidence tab content) |
```

```quote specs/features/v2-result-fidelity/spec.md:253
    tabs from the stream (§6.1). At phone width a too-wide table scrolls inside the panel with the
```

```quote crates/web/templates/partials/result_evidence.html:7
{% if !panel.blocks.is_empty() %}<p class="evidence-tabs__swipe">← swipe the table</p>{% endif %}
```

```quote specs/features/v2-result-fidelity/spec.md:146
12. **Heading order.** The page has one h1 (the domain) and an h2 per section: "Since run <k−1>"
```

```quote crates/web/src/pages/result/mod.rs:1039
        diff_heading: Some(since_heading(entries.len()).unwrap_or_else(|| "This run".to_string())),
```

### Refutation

| finding | refuter | confidence | outcome |
|---|---|---|---|
| BLOCKER mod.rs:628 score rounds half up across a threshold | CONFIRMED | 8 | kept |
| MAJOR pages.css:299 no 3 px bar on the hard-fail banner | CONFIRMED | 9 | kept, downgraded by nothing |
| MAJOR trace.css:83 phone hides the edge dates | REFUTED (board M draws none) | 6 | below 7: MINOR |
| MAJOR report.md:400 V3, V4, V14 closed as proposals | UNCERTAIN | 5 | MINOR |

### Refuted

None at confidence 7 or above.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| mod.rs:628 rounding | CONFIRMED | 8 | held | `sed -n 625,630p crates/web/src/pages/result/mod.rs` (round(), thresholds use >=) |
| pages.css:299 bar | CONFIRMED | 9 | held | `sed -n 299,307p crates/web/static/pages.css` (no border or pseudo-element) |
| trace.css:83 edge dates | REFUTED | 6 | not held | `grep -n "10-08" M.dc.html` (board M draws no edge dates beside its trace) |
| report.md:400 proposals | UNCERTAIN | 5 | not checked as MAJOR | MINOR |

### Summary

Before refutation: reader 1/0/0, spec reader 0/3/2 (REQUIREMENTS met=40 partial=3 missed=0 contradicted=0). After refutation: blockers 1, majors 1, minors 4 (reader 1/0/0; spec reader 0/1/4). Verified 2, held 2. The blocker is fixed on this branch (implementer under the operator's autonomy mandate) and read again in a second pass.

## 8554424c..080f1174

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | crates/web/src/pages/result/mod.rs:629 | When a run's score is a whole number by the scoring rules, the score line shows one less, because `floor()` is applied to an f64 that division leaves just under that whole number. Before this change `round()` showed the right number. | Complete run under profile v3 whose exact weighted mean is 80: `scoring.rs:141` yields 79.99999999999999 in f64, so `floor` gives "score 79 / 100"; a dns-only run of exactly 58 yields 57.999999999999986 and shows 57.

```quote crates/web/src/pages/result/mod.rs:629
    Some(score.overall_percentage.floor().clamp(0.0, 100.0) as u8)
```

```quote crates/engine/src/scoring.rs:141
    let overall_percentage = weighted_sum / total_weight as f64;
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=6 partial=0 missed=0 contradicted=0

No findings.

### Refutation

Not run: the single BLOCKER was verified directly (below), no MAJOR.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| mod.rs:629 floor under float error | not asked | n/a | held | `score_number(79.99999999999999, B, ..)` row in `crates/web/tests/it/fidelity_score_number.rs` failed against plain floor |

### Summary

Before and after: reader 1/0/0, spec reader 0/0/0 (REQUIREMENTS met=6). Verified 1, held 1. Fixed on this branch: `views::score_number` rounds down with a 1e-9 guard and caps the number below the next grade's threshold unless a hard fail set the grade; table-tested.

## 080f1174..f384a74f

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result found: the cap only lowers float-error values (the engine's grade comes from the same percentage and thresholds), the 1e-9 guard cannot lift a genuine non-integer score (true scores lie at least about 5e-8 from an integer under v3), A+, incomplete and hard-fail cases return the plain number, and each table row breaks when its branch is removed.

### Spec reader

skipped: the range changes no spec text; its requirement (5) was read in the previous pass.

### Summary

Reader 0/0/0; spec reader skipped. Verified 0, held 0. No blocker remains.
