
## 3e8d7e14..32b3e6d5

### Reader

COUNTS blockers=1 majors=0 minors=2
LENSES Engineering, Testing
BLOCKER | crates/web/src/pages/inspect.rs:783 | The `+check` note only shows when the flag is typed in lowercase. The DNS parser and the web job layer treat flags case-insensitively, so a query with `+CHECK` runs the same job as `+check` but the note that explains it stays hidden. | `GET /inspect/dns?q=example.com%20%2BCHECK`: the prefill holds `+CHECK`, `t == "+check"` is false, so the page renders `<p class="dns-builder__note" hidden>`. Typing `example.com +Check` into the field gives the same result in the island (crates/web/islands/src/dns.ts:24, `includes("+check")`). `parse_flag` lowercases the flag (crates/dns/src/parser.rs:405) and `start_dns` lowercases flags (crates/web/src/api/jobs.rs:685), so the run is the plain query plus the DNS band, identical to the `+check` run.
MINOR | crates/web/tests/it/polish_evidence_view.rs:240 | The check that the evidence section's row links sit inside the rows card (C5) can never fail. | `section[card..last_link]` always contains the card's own `<div` plus the three `<div`s of the check row before the first link, so `matches("<div").count() > 1` always holds. A template that moved the `evidence-section__link` anchors after the card's `</div>` would still pass every assertion in the test.
MINOR | crates/web/tests/it/polish_check_card.rs:143 | The check that the shell check list's row links sit inside `.inspector-checks__card` (C14) can never fail. | The stub's checks carry `evidence: Vec::new()`, so neither the live band nor the shared page renders any `evidence-section__link`. Both counts are 0 and `assert_eq!(0, 0)` passes wherever the template puts the links. report.md records this as a kept NIT.

```quote crates/web/src/pages/inspect.rs:783
                .is_some_and(|q| q.split_whitespace().any(|t| t == "+check")),
```

```quote crates/web/islands/src/dns.ts:24
    if (note) note.hidden = !q.value.split(/\s+/).includes("+check");
```

```quote crates/dns/src/parser.rs:405
    let flag = name.to_ascii_lowercase();
```

```quote crates/web/src/api/jobs.rs:685
            .map(str::to_ascii_lowercase)
```

```quote crates/web/tests/it/polish_evidence_view.rs:240
        closing > last_link || section[card..last_link].matches("<div").count() > 1,
```

```quote crates/web/tests/it/polish_check_card.rs:143
        inner.matches(r#"class="evidence-section__link""#).count(),
```

### Spec reader

COUNTS blockers=0 majors=1 minors=1
REQUIREMENTS met=11 partial=1 missed=0 contradicted=0
MAJOR | crates/web/templates/partials/inspect_caller_notice.html:1 | The IP inspector's own-address run puts its refusal notice in the result region with a visible target h1 and no example chips, so it does not render the idle result frame. | R5. GET `/inspect/ip` with no `ip` while the IP slot is taken: `show()` sets `result = Some(html)` from `caller_run`, which replaces the shell's idle `<div id="inspect-result"><h1 class="visually-hidden">…` and its chips. The region then reads `<h1 class="inspector-run__target">203.0.113.7</h1><p class="inspector-notice" role="status" data-tone="warn">busy, try again in a moment</p>`. The same happens for "too many requests", "timed out" and invalid input on that path. The report gives "the caller page has no input to fill" as the reason, but this page renders the shell's query input and tabs. The stream path (`notice()`) does build the full frame.
MINOR | crates/web/src/pages/inspect.rs:1262 | An email "busy" refusal used to be answered with HTTP 200 and is now answered with 503; no requirement asks for a different status. | R5 asks only for the warn tone and unchanged text. Before the range, `EmailRefusal::Busy` went through `Err(other) => return Err(other.to_string())` into `notice(tool, StatusCode::OK, message)`. Now `jobs.rs` routes it through `Refusal::Busy`, which `run()` answers with `StatusCode::SERVICE_UNAVAILABLE`. The text is the same and the response code changed.

```quote specs/features/v2-ui-polish/spec.md:75
5. **Refused run frame.** A notice that replaces the inspector's result region (too many requests,
   busy, invalid input, timed out) renders the idle result frame: the visually hidden h1, the
   notice as the shell's notice card (board F's card frame with a warn-tone marker for "too many
   requests", "busy" and "timed out"; no tone for invalid input) with `role="status"`, and the
   inspector's example chips below it. The text of each notice is unchanged.
```

```quote crates/web/templates/partials/inspect_caller_notice.html:1
<div id="inspect-result"><h1 class="inspector-run__target">{{ target }}</h1><p class="inspector-notice" role="status"{% if let Some(tone) = tone %} data-tone="{{ tone }}"{% endif %}>{{ message }}</p></div>
```

```quote crates/web/templates/components/inspector_shell.html:9
{% if let Some(r) = shell.result %}{{ r|safe }}{% else %}<div id="inspect-result"><h1 class="visually-hidden">{{ shell.name }}</h1>{% if let Some(n) = shell.notice %}<p class="inspector-notice">{{ n }}</p>{% endif %}<ul class="inspector-examples">
```

```quote crates/web/src/pages/inspect.rs:863
    let caller_notice = |message: String, warn: bool| {
        (CallerNotice {
            target: target.clone(),
            message,
            tone: warn.then_some("warn"),
        })
```

```quote crates/web/src/api/jobs.rs:472
            Err(busy @ EmailRefusal::Busy) => {
                let message = busy.to_string();
                return Ok(Job::new(Vec::new())
                    .with_gate(Box::pin(async move { Err(Refusal::Busy(message)) })));
            }
```

```quote crates/web/src/pages/inspect.rs:1261
            Ok(Err(Refusal::Busy(message))) => {
                return notice(tool, StatusCode::SERVICE_UNAVAILABLE, message, true);
            }
```

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| inspect.rs:783 `+check` note case-sensitive | CONFIRMED | 8 | held | `curl -s 'http://127.0.0.1:18080/inspect/dns?q=example.com%20%2BCHECK'` renders `<p class="dns-builder__note" hidden>` |
| inspect_caller_notice.html:1 caller notice without the idle frame | CONFIRMED | 7 | held | `cat crates/web/templates/partials/inspect_caller_notice.html` shows the visible target h1 and no example chips |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence — no codename in copy, no new status enum |
| P10 | absence — no check or scoring path changed |
| P12 | absence — no outbound call added |
| P13 | absence — no route or limiter added |
| P18 | absence — no client or config struct added |
| P26 | absence — no check ID added |
| P35 | convergence — the design-rules change arrives with tests/repo/test_design_live_state.sh |
| P36 | convergence — no generated artefact changed without its source |
| P40 | absence — no module added or removed |

### Summary

Before refutation: reader 1 blocker, 0 majors, 2 minors; spec reader 0 blockers, 1 major, 1 minor (REQUIREMENTS met=11 partial=1 missed=0 contradicted=0). After refutation: blockers 1, majors 1, minors 3; verified 2, held 2. Roll call: answered 9, convergence 3, divergence 0, absence 6. The blocker is fixed on this branch (decision log), then the fix range is read again.

## 32b3e6d5..3f22eaaa

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Testing

MAJOR | crates/web/islands/src/dns.ts:25 | The island uses Unicode `toLowerCase()`, while the parser and the server-side note use ASCII-only folding. So the island shows the +check note for a token the parser rejects as an unrecognized flag, and the note contradicts the server render of the same query. | Type `example.com +checK` (U+212A KELVIN SIGN): in the island, `"+checK".toLowerCase() === "+check"` is true (checked in node), so the note shows. The parser's `to_ascii_lowercase` leaves `checK` unchanged, so it is not in `FLAGS` and becomes the warning `unrecognized flag: +checK`. `GET /inspect/dns?q=example.com%20%2Bchec%E2%84%AA` renders the note `hidden`, because `eq_ignore_ascii_case` is false. The island then unhides it on the first click in the shell. An ASCII-only comparison would agree with the parser, e.g. `/^\+check$/i` without the `u` flag.

```quote crates/web/islands/src/dns.ts:25
    if (note) note.hidden = !q.value.split(/\s+/).some((t) => t.toLowerCase() === "+check");
```

```quote crates/web/src/pages/inspect.rs:784
                    .any(|t| t.eq_ignore_ascii_case("+check"))
```

```quote crates/dns/src/parser.rs:405
    let flag = name.to_ascii_lowercase();
```

### Spec reader

COUNTS blockers=0 majors=0 minors=0
REQUIREMENTS met=2 partial=0 missed=0 contradicted=0

Requirements 3 and 5 (as amended) traced: the server and the island read `+check` case-insensitively, as the parser does; the caller's refusal gets the example chips, keeps its address h1 (v2-ip-ui requirement 3), warn tone on too many requests, busy and timed out, none on invalid input; texts unchanged.

### Refuted

None.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| dns.ts:25 Unicode case fold | CONFIRMED (refuter suggests MINOR) | 8 | held | `node -e` showed `"+chec\u212A".toLowerCase() === "+check"` true and `/^\+check$/i` false |

### Summary

Reader 0/1/0, spec reader 0/0/0 (REQUIREMENTS met=2). After refutation: blockers 0, majors 1, minors 0; verified 1, held 1. Fixed after this reading with an ASCII-only fold.
