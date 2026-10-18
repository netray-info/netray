## 309c6611..d0f4a3d9

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing

MAJOR | crates/web/src/pages/inspect.rs:479 | When a posted target is refused (invalid input at :479, 429 at :468, busy at :462), the route sends only the `inspect-result` fragment (`notice`, :428). The `#inspect-backlink` element therefore keeps the previous target's registrable-domain link, which breaks requirement 12: the page should link to the registrable domain of the name being inspected. | Steps: open `/inspect/tls?h=www.example.com`. The autostart job runs and the backlink fragment reads "Back to example.com". Then submit `mail.example.org:143` from the form. `start_tls` returns `NO_STARTTLS` and `notice()` replaces only `inspect-result`. The page now shows "STARTTLS for IMAP and FTP is not supported" next to "Back to example.com" (href `/example.com`), not `/example.org`. The same happens with `http://other.org/` on `/inspect/http` (the `parse_url` refusal), and with a 429 or busy answer after an earlier run.

MINOR | crates/web/tests/inspect_no_store.rs:146 | This "store unchanged" assertion cannot fail. The router under test (:126) is built with a `Stub` and `InspectDeps` that hold no `HistoryStore`, so nothing reachable from the request can touch the store the test snapshots. | Change `Stub::start` to anything, or delete the no-write guarantee from the routes: the `assert_eq!(snapshot(&pool).await, before)` still passes. Requirement 11 is actually exercised only by `crates/web/tests/inspect_no_store_real.rs`, which hands the store to `ApiState`.

```quote crates/web/src/pages/inspect.rs:479
        Err(message) => return notice(StatusCode::OK, message),
```

```quote crates/web/src/pages/inspect.rs:428
        Ok(html) => sse(status, vec![fragment(RESULT_ID, &html), end()]),
```

```quote crates/web/src/pages/inspect.rs:468
        return too_many_requests(retry_after);
```

```quote crates/web/src/pages/inspect.rs:462
        return notice(
```

```quote crates/web/tests/inspect_no_store.rs:146
    assert_eq!(snapshot(&pool).await, before);
```

```quote crates/web/tests/inspect_no_store.rs:126
    let app = inspect_router(InspectDeps::new(Arc::new(Stub), InspectConfig::default()));
```

Traced and found sound: the stream state machine (deadline, timed-out parts, band ordering, permit release); TLS, HTTP, IP and DNS admission and the limiters shared with `/api/v1`; the DNS band's spawn and abort-on-drop; the evidence view model, anchors and explore-live encoding; route precedence against `/r/{id}/og.png`, `/inspect/{tool}`, `/guide/{slug}` and the SPA fallback; the `[inspect]` config merge and validation; the CSP hash of the speculation rules and SSE excluded from compression; the seed's validation-before-write; the `ADLC-Test-Change` trailers on every modified protected test.

### Spec reader

COUNTS blockers=1 majors=1 minors=1
REQUIREMENTS met=23 partial=1 missed=0 contradicted=1

BLOCKER | crates/web/src/api/jobs.rs:340 | The HTTP inspector validates with `parse_url`, which refuses every `http://` URL, so it does not accept "any `http` or `https` URL". | Req 16: posting `url=http://example.com/` runs `parse_url`, which returns Err("Submit the URL with https:// scheme; port-80 probing is automatic."). The route shows that as the invalid-input notice and starts no job. The decision log (spec.md:383) records this choice, but requirement 16's own text was never changed to match.

MAJOR | tests/acceptance/v2/inspectors.spec.ts:161 | The Playwright spec checks only each view's own explore-live link and the per-row links. It never checks a per-block link (`dns.<type>`, `email.dmarc`, `email.mta_sts`), so it falls short of requirement 24's "each explore-live href". | Req 24: on `/example.com/dns` the seeded fixture has 14 `dns.<type>` blocks, and each renders a `/inspect/dns?q=<owner> <TYPE>` link. ROW_LINKS has no `dns` key, so only `explore[0]` is checked and a wrong `dns.mx` href would still pass. The same applies to the `_dmarc`/`_mta-sts` links on `/example.com/email`.

MINOR | crates/web/templates/components/query_input.html:6 | Every stream form now has a hidden error line, "The live run failed. Try again.", and `query_input.css` gains `.query-input__error` styling. No requirement or the Goal names this output. | Every `/inspect/<tool>` page renders `<p class="query-input__error" data-stream-error hidden>`. The stream island un-hides it when a response is not an event stream or ends without `end`.

```quote specs/features/v2-inspectors/spec.md:76
16. The HTTP inspector accepts any `http` or `https` URL, path included, and renders the redirect chain, headers and the other blocks of `netray_http::module::translate` over the job's `InspectResponse`; its band is that translation's checks (I1).
```

```quote crates/web/src/api/jobs.rs:340
        let url = netray_http::input::parse_url(&req.input).map_err(|e| e.to_string())?;
```

```quote crates/http/src/input.rs:28
        "http" => Err(AppError::InvalidUrl(
```

```quote specs/features/v2-inspectors/spec.md:84
24. A Playwright spec `tests/acceptance/v2/inspectors.spec.ts` (project `local`) opens every evidence view of the seeded entry and every inspector page at 1280 px and 390 px in light and dark, asserts the run line, the check-row order, the shared-evidence line, each explore-live href, no horizontal page scroll at 390 px outside the band (S16), and that `/inspect/email` is `404`, and saves a screenshot per view, width and theme under `tests/acceptance/test-results/` (gitignored; listed in `report.md`, not committed; default, not interviewed). With the argument `--grep @live` it also runs one job per inspector against `netray.info` and `example.com` and asserts "live · not recorded", no grade and a rendered band.
```

```quote tests/acceptance/v2/inspectors.spec.ts:161
        expect(explore[0]).toBe(VIEW_LINK[protocol]);
```

```quote tests/acceptance/v2/inspectors.spec.ts:39
const ROW_LINKS: Partial<Record<Protocol, string[]>> = {
```

```quote crates/web/templates/components/query_input.html:6
  {% if !stream_url.is_empty() %}<p class="query-input__error" data-stream-error hidden>The live run failed. Try again.</p>{% endif %}
```

### Refuted

- BLOCKER crates/web/src/api/jobs.rs:340 (spec reader): REFUTED, confidence 7 — `http://` is refused as the Decision log records (spec.md:383, V1's behaviour); requirement 16's wording drifted from that decision. Checked: the code matches the decision, so no wrong result; requirement 16's text is amended to the decision (implementer under the operator's autonomy mandate, 2026-10-18).

### Calibration

| finding | refuter | confidence | result | command |
|---|---|---|---|---|
| jobs.rs:340 `http://` refused | REFUTED | 7 | not held | `sed -n 383p specs/features/v2-inspectors/spec.md`; `sed -n 20,32p crates/http/src/input.rs` |
| inspect.rs:479 stale backlink on refusal | CONFIRMED | 6 | held | `sed -n 432,490p crates/web/src/pages/inspect.rs`: `notice`/`too_many_requests` emit only `inspect-result`; `link` is computed after the busy and 429 returns |
| inspectors.spec.ts:161 per-block explore links unasserted | CONFIRMED | 8 | held | `grep -n "ROW_LINKS\|VIEW_LINK" tests/acceptance/v2/inspectors.spec.ts`: only `explore[0]` and row links for tls, email, ip |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename or new status enum in templates, site/ or API serializers; `inspect_gate.rs` rejects codenames in copy |
| P10 | absence | no check or scoring path changed |
| P12 | convergence | no new reqwest client; job paths run under `timeout_at(deadline, …)` |
| P13 | convergence | one semaphore per inspector tool; `max_concurrent = 0` is a startup reject |
| P18 | convergence | no new client; `[inspect]` structs carry `deny_unknown_fields` |
| P26 | absence | no new check ID |
| P35 | absence | no `specs/rules/` change |
| P36 | absence | no generated file changed |
| P40 | absence | no module added or removed |

### Summary

Before refutation: reader 0/1/1, spec reader 1/1/1 (blockers/majors/minors). After: reader 0/1/1, spec reader 0/1/1 (`REQUIREMENTS met=23 partial=1 missed=0 contradicted=1`; the contradiction is requirement 16's wording, amended). Total 0/2/2; verified 3, held 2. Roll call: answered 9, convergence 4, divergence 0, absence 5. Held majors: the per-block explore-live links are now asserted in `tests/acceptance/v2/inspectors.spec.ts` (43 passed); the stale back-link on a refused job is deferred to v2-cutover, since sending a second fragment contradicts requirement 10's one-fragment refusal and five router tests (spec Decision log; implementer under the operator's autonomy mandate, 2026-10-18).
