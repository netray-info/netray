# Review: v2-site-nav

## 39ccc6e9..0cb8ce55

### Reader

COUNTS blockers=0 majors=1 minors=0
LENSES Engineering, Security, Testing
MAJOR | crates/web/src/pages/result/mod.rs:1507 | When a selector run is refused and the request came without the stream island, the 429 page drops the refusal line. In its place it says a run is underway: "DKIM selectors … · not recorded · the stored run shows until this run ends". No run follows, because autostart is false. | Setup: `example.com` already has a stored entry, and the client is over its per-IP run limit. Request: `POST /example.com/run` with body `trigger=recheck&dkim_selectors=s1,s2` and no `Accept: text/event-stream`. `refused()` calls `full_page(…, TOO_MANY_REQUESTS, …, Some(Refusal{lead, retry}), Some("s1,s2"))`. `stored_parts` puts the refusal into `shown.run_line`, but `stored_shown` has set `recorded: true`. The guard in `assemble` (`Some(list) if shown.recorded`) runs before `with_selectors` and replaces the refusal. The same request without selectors still shows it.

```quote crates/web/src/pages/result/mod.rs:1507
        Some(list) if shown.recorded => RunLineBody::Line(RunLineView {
```

```quote crates/web/src/pages/result/run.rs:193
        full_page(
```

### Spec reader

COUNTS blockers=0 majors=1 minors=3
REQUIREMENTS met=19 partial=1 missed=0 contradicted=0
LENSES requirements (spec v2-site-nav R1–R20, Phases 1–7)
MAJOR | crates/web/templates/pages/evidence.html:16 | The evidence view's no-entry state still shows only the A-label in its heading and title, so this evidence-view state has no IDN display | R16: `GET /xn--bcher-kva.example/email` with no stored entry goes through `latest` → `Ok(None)` and renders `<h1>No entry for xn--bcher-kva.example</h1>` with the title "xn--bcher-kva.example - netray" (crates/web/src/pages/evidence.rs:177). The required "bücher.example (xn--bcher-kva.example)" never appears. The history page's empty state ("not checked yet") was converted; this one was not.
MINOR | crates/web/src/pages/inspect.rs:622 | A blank `ip=` now autostarts the visitor's own-address lookup on `/inspect/ip`, a change to inspector behaviour that no requirement asks for | Before the range, `GET /inspect/ip?ip=` dropped the empty prefill and did not autostart. Now `own_ip` is true and `autostart` fires (line 665). The change was added so the vpn-leak guide's blank CTA works; R2 only asks that the CTA target `/inspect/<tool>`.
MINOR | crates/web/src/entries/routes.rs:92 | Every entries page (`/privacy`, `/r/<id>`, the 410 gone page) now gets the pages' CSP and the seven fixed security headers, which the spec never mentions | `GET /r/<id>` used to answer with only content-type and cache-control. Now it goes through `pages::headers::secure`. R8 asks for the frame, footer and stylesheet on `/privacy`, not for its response headers or those of `/r/<id>`.
MINOR | crates/web/src/pages/errors.rs:99 | The selector refusal page adds a second sentence with the raw parser message to the one-sentence rule that R15 names | `GET /example.com?dkim_selectors=bad.sel` shows the rule sentence followed by "This one was refused: dkim_selectors: <netray_email validation error>." That leaks the parameter name and internal error text into page copy that R15 specifies as "the rule in one sentence".

```quote specs/features/v2-site-nav/spec.md:178
16. **Unicode beside the A-label.** One function in netray-web returns a name's Unicode form when
```

```quote crates/web/templates/pages/evidence.html:16
<h1>No entry for {{ domain }}</h1>
```

```quote crates/web/src/pages/evidence.rs:177
                    &format!("{domain} - netray"),
```

```quote crates/web/src/pages/inspect.rs:622
    let own_ip = tool == Tool::Ip && query.get(tool.field()).is_some_and(|v| v.is_empty());
```

```quote crates/web/src/pages/inspect.rs:665
        autostart: (prefill.is_some() || own_ip) && result.is_none(),
```

```quote crates/web/src/entries/routes.rs:92
        Ok(body) => crate::pages::headers::secure(
```

```quote crates/web/src/pages/errors.rs:99
            "A DKIM selector list is 1 to 5 names of letters, digits and hyphens, separated by commas. This one was refused: {detail}."
```

### Refuted

None: both majors confirmed.

### Calibration

| finding | refuter answer | confidence | result | command |
|---|---|---|---|---|
| F1 mod.rs:1507 refused selector run loses the refusal line | CONFIRMED | 9 | held | `sed -n 180,205p crates/web/src/pages/result/run.rs; sed -n 1500,1520p crates/web/src/pages/result/mod.rs` (refused() passes the Refusal into full_page; the `shown.recorded` guard replaces it) |
| F2 evidence.html:16 no-entry evidence view without the IDN form | CONFIRMED | 8 | held | `sed -n 14,18p crates/web/templates/pages/evidence.html; sed -n 170,180p crates/web/src/pages/evidence.rs` |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename in added template, src or island lines; no new status enum |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no outbound call or module entry added |
| P13 | absence | `/tools` is static; limiter setup untouched |
| P18 | convergence | no reqwest client added, no config struct touched |
| P26 | absence | no check id added |
| P35 | convergence | the design-rules `click` key arrives with `test_design_manifest.sh` rows; the apex rule with its check and selftest |
| P36 | convergence | no generated file edited by hand; sitemap from routes |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/1/0, spec reader 0/1/3 (REQUIREMENTS met=19 partial=1 missed=0 contradicted=0). After refutation: 0 blockers, 2 majors (F1, F2), 3 minors; verified 2, held 2. Roll call: answered 9, convergence 4, divergence 0, absence 5. Both majors are fixed on this branch after the reading.
