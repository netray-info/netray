# Review: v2-http-ui

## 0b17f07e..6231077d

### Reader

COUNTS blockers=0 majors=0 minors=2
LENSES Engineering, Security, Testing
MINOR | crates/web/tests/it/views_nothing_lost.rs:92 | The `issue_count` check in the nothing-lost test cannot fail. Its needle is the bare text "2", and the escaped issue text in the same fixture always contains a "2". | Delete the `<p class="csp-view__count">` line from `templates/partials/csp_view.html` and the test still passes. The root issue `object-src allows <object> & 'unsafe-eval'` (line 94) renders escaped as `&#60;object&#62;`, and `&#62;` contains "2". (`views_http_csp.rs:87` checks "2 issues" separately, so only this row of requirement 3 is empty.)
MINOR | crates/web/tests/it/views_nothing_lost.rs:114 | The HSTS row cannot catch a dropped `preload` or `include_subdomains` cell. Both fixture values are `true`, so both needles are the same text "yes". | Remove the `("preload", flag("preload"))` cell from `hsts()` in `crates/web/src/pages/views/http.rs`. The includeSubDomains cell still renders "yes", so the needle check at line 173 passes and the fixture still covers every registered key.

```quote crates/web/tests/it/views_nothing_lost.rs:92
                r.pair("issue_count", Value::Int(2)),
```

```quote crates/web/tests/it/views_nothing_lost.rs:114
                r.pair("preload", Value::Bool(true)),
```

I found no wrong result in the code itself. Traced against the HTTP backend's writers in `crates/http/src/module.rs` and found sound: claiming and fallback; feeds and tones in `part_html`, `build_section` and `panel`; the live re-send after the band; display values (time conversion checked against Python's `datetime` over 200,000 random times with offsets); escaping of every `|safe` body; the slim band and its token.

Process gap, not a code defect: report.md Phase 4 C9 says the inspector page at 390 px was measured with the 4 kB policy; the probes timed out locally, so the policy width was only measured on the evidence view.

### Spec reader

COUNTS blockers=0 majors=2 minors=0
REQUIREMENTS met=16 partial=2 missed=0 contradicted=0
MAJOR | crates/web/src/evidence.rs:80 | A duration from 59 950 ms to 59 999 ms is below 60 s but is shown as "1 min 0 s" where the requirement asks for "<s.s> s". | Req 6, trace for 59 960 ms: `tenths = (59960+50)/100 = 600`. The check `600 < 600` fails, so the code goes to the minute branch, gets `secs = (59960+500)/1000 = 60` and prints "1 min 0 s".
MAJOR | specs/features/v2-http-ui/report.md:186 | The HTTP inspector was read and measured with only the port-80 probe and the server view on the page, so the inspector never showed the 4 kB policy, the CSP, caching or isolation views, or a tone bar. C9 still claims "/inspect/http at 390 px with the 4 kB policy" is green. | Req 18 / Phase 4: the 390 px width on `/inspect/http` (report.md:218, "inspector 390 light: scrollWidth 390") was measured against `https://1.1.1.1/` with the probes timed out. Only the evidence view `/csp.example.com/http` carried the 8 kB policy.

```quote specs/features/v2-http-ui/spec.md:125
6. **Display values.** The evidence block (and every view) shows values through a display
```

```quote crates/web/src/evidence.rs:80
fn display_duration(ms: u64) -> String {
```

```quote specs/features/v2-http-ui/spec.md:186
18. **Design gate.** `just design-check` exits 0, and the HTTP inspector at 1280 px and 390 px,
```

```quote specs/features/v2-http-ui/report.md:182
| C9 | S /inspect/http at 390 px with the 4 kB policy is not wider than the viewport | green | scratchpad shots.mjs, measured below |
```

```quote specs/features/v2-http-ui/report.md:186
A reader with no part in the build compared the HTTP inspector (`/inspect/http?url=https://1.1.1.1/`; the probes timed out locally, so only the port-80 probe and the server view render) and the evidence view of a seeded fixture holding every HTTP view (`/csp.example.com/http`, a policy of 8 kB) at 1280 and 390 px, light and dark, with boards 17 and 18.
```

Requirement 17 counts as met although the range does not contain it: the manifest rows are in the private `netray.info` repo, commit bdf1de3.

### Refuted

None.

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| evidence.rs:80 duration 59 950 to 59 999 ms reads "1 min 0 s" | CONFIRMED | 8 | held | `cargo test -p netray-web --test it -- display_value_formats` with 59 960 ms: left "1 min 0 s", right "59.9 s" |
| report.md:186 C9 measured on the evidence view, not the inspector | CONFIRMED | 7 | held | report.md Phase 4 reading line: the inspector probes timed out locally, no CSP view rendered on `/inspect/http` |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no lens/codename or new *Status/*Verdict in the added lines |
| P10 | absence | rendering only; no check or scoring path |
| P12 | absence | no outbound call or engine entry added |
| P13 | absence | no route or limiter added |
| P18 | convergence | no reqwest::Client constructed; no config struct changed |
| P26 | absence | no check ID added; views render existing blocks |
| P35 | convergence | frontend-rules.md component list arrives with component_files.rs, component_gate.rs, facts_grid.rs |
| P36 | convergence | tokens.json gains bandPlateHeight; the CSS token is generated by build.rs |
| P40 | absence | no module added; crates/engine untouched |

### Summary

Before refutation: reader 0/0/2, spec reader 0/2/0 (REQUIREMENTS met=16 partial=2 missed=0
contradicted=0). After refutation: blockers 0, majors 2, minors 2; verified 2, held 2. Roll call:
answered 9, convergence 4, divergence 0, absence 5. Both majors are fixed on this branch after the
reading (duration boundary; C9 re-scoped with the inspector limitation logged).
