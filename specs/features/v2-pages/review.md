
## cd0c2a12..c5664b55

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Security, Testing

I found no wrong result in cd0c2a12..c5664b55.

What I traced:
- **lens headers and routing.** The new `keep_inner` option in `crates/common/src/security_headers.rs` lets lens keep a header a handler already set. No handler in lens or `netray_web::entries` sets any of the eight security headers, so only the pages routes keep their own CSP and the other seven headers. The SPA, `/robots.txt`, `/privacy`, `/r/*` and the Scalar `/docs` are unchanged. In axum 0.8.9's `Router::merge`, `frontend_router_with` hands the SPA fallback to `app()` correctly. No lens route collides with the pages routes, and the lens Vite dist has no `static/` files that the new `/static/*` route would hide.
- **CSP hash and the 301 redirect.** The speculation rules are rendered with `|safe`, so the CSP hashes exactly the bytes on the page. In the `?d=` redirect, `normalise` follows requirement 15 and the `Location` value is percent-encoded.
- **Guides and compare.** All 32 guides match V1 on titles, descriptions and og images (the 6 PNGs are byte-identical to `site/og`). Related slugs, internal guide links and fragments resolve. The 9 guides with breadcrumbs match V1. Every `compare.toml` row has as many cells as its group has competitors, and the netray column is built only from the checks and depth each row lists.
- **Islands.** I traced the stream island's SSE parser through chunk splits, CR/LF, multi-line data, `end`, abort and SVG-context parsing. I also traced the status, theme and mount islands, the build.rs light override (it sits before the phone media block, so the phone overrides still win), the gate's new attribute parsing and style-write rules, and the repo checks and tests.
- **Already known, not reported.** The remaining issues I came across are in `specs/features/v2-pages/report.md` as DEFERRED. On the status page: an empty monitor list shows the all-clear, paused monitors count as affected, and one failed feed throws away a good monitor list. `?d=` drops `dkim_selectors`. Gate rule 1 does not catch `el["style"]` or `setAttributeNS`.
- **Not reported: one CLAUDE.md departure.** `tests/repo/test_web_crate.sh` reads Cargo.toml with `tomllib`, while CLAUDE.md says Cargo dependency checks must read `cargo metadata`. The only wrong outcome is a loud false refusal if base64 were ever renamed in `[workspace.dependencies]`. `cargo metadata` can't show whether a dependency comes from the workspace, so the manifest has to be read for this check.

### Spec reader

COUNTS blockers=0 majors=2 minors=0
REQUIREMENTS met=32 partial=2 missed=0 contradicted=0
MAJOR | crates/web/templates/pages/guide.html:5 | Every ported guide's `<h1>` is rendered from the `<title>` text in `guides.rs` instead of the V1 `<main>` heading, so eight guides show a heading V1's main content never had. | R17, "The body is the V1 `<main>` content". Example: `/guide/email-auth` shows "Email Authentication Guide: SPF, DKIM, and DMARC Explained", but V1's main has "Email Authentication Guide: SPF, DKIM, and DMARC". certificate-chain, certificate-management, dane-tlsa, encrypted-client-hello, ip-enrichment, record-types and tls-protocol differ the same way (each gets the longer `<title>` suffix). Codename swaps are excluded from the count.
MAJOR | crates/web/tests/pages_frame.rs:176 | The "exactly once" frame test runs only on the 404, 400 and 500 pages, so the guide, guide-index, compare and status pages never have header, footer, stylesheet, bundle and speculation rules counted. | R9, "A test asserts each one carries the header, footer, stylesheet, bundle and speculation rules exactly once". Example: a guide template that overrides `{% block scripts %}` or repeats the footer would still pass, because `pages_guides.rs:131`, `pages_compare.rs:198` and `pages_status.rs:72` check only for `<html lang="en">`. The code itself is correct: every page extends `base.html` without overriding the three blocks.

```quote specs/features/v2-pages/spec.md:63
17. `crates/web/src/pages/guides.rs` holds the guide list, the one source of each guide's slug, title, meta description, category, `og:image` and related guides; the index, every guide's head and the sitemap read it (P36). Each of the 32 `site/guide/<slug>.html` other than `index.html` becomes `templates/pages/guides/<slug>.html` extending `base.html`. The body is the V1 `<main>` content with its element ids kept. Each codename in prose is replaced: lens → netray, prism and mhost-prism → the DNS inspector, tlsight → the TLS inspector, spectra → the HTTP inspector, beacon → netray's email checks, ifconfig-rs → the IP inspector. Each of the 31 `tool-entry` forms becomes a `query_input` call with its name and placeholder. Links and actions map as follows:
```

```quote crates/web/templates/pages/guide.html:5
<h1>{{ guide.title }}</h1>
```

```quote site/guide/email-auth.html:68
  <h1 class="page-title">Email Authentication Guide: SPF, DKIM, and DMARC</h1>
```

```quote crates/web/src/pages/guides.rs:32
        title: "Email Authentication Guide: SPF, DKIM, and DMARC Explained",
```

```quote specs/features/v2-pages/spec.md:55
9. `templates/base.html` is the one page frame (merged with v2-entries' per requirement 32). It holds `<html lang="en">`, `<meta charset>` and viewport, a title and a meta description per page, `<link rel="canonical">` with the absolute `https://netray.info` URL, `og:title`, `og:description` and `og:image`, and three overridable blocks with these defaults: `styles` = one stylesheet link `/static/app.css` and the favicon link `/static/favicon.svg`; `header` = a header with the logo as `<img src="/static/logo.svg" alt="netray">` linking `/`, the `theme` button (requirement 27) and the navigation listing Guides `/guide/`, Compare `/compare`, Status `/status` and the inspectors IP, DNS, TLS and HTTP at their subdomains of netray.info (S3: email keeps none); `scripts` = `<script type="module" src="/static/islands.js"></script>` and the speculation rules (requirement 12), last in the body. Between header and scripts come a `main` block and a footer with the tagline partial and links to Privacy `/privacy` and Status `/status`. Every page of this router extends it without overriding the three blocks. A test asserts each one carries the header, footer, stylesheet, bundle and speculation rules exactly once. (Navigation and footer entries: approved default.)
```

```quote crates/web/tests/pages_frame.rs:176
async fn pages_carry_the_frame_once() {
    let (status, not_found) = get_page("/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_frame("404 /nope", &not_found);

    let (status, bad) = get_page("/?d=foo").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_frame("400 /?d=foo", &bad);

    let server_error = body_of(pages::server_error()).await;
    assert_frame("500 server_error()", &server_error);
```

```quote crates/web/tests/pages_guides.rs:131
        assert!(body.contains(r#"<html lang="en">"#), "{uri}: frame");
```

Traced against the code with nothing found:
- **Frame and lens mount:** the light-theme block in `build.rs`, the three scoped SVGs, `base.html` with v2-entries' overrides, and lens's `frontend_router_with`. lens keeps headers the inner response already set (`keep_inner`), no V1 handler sets a security header, and the routes `/robots.txt` and the SPA fallback are unchanged.
- **CSP and headers (`pages/headers.rs`):** the hash comes from `SPECULATION_RULES`; the policy and the seven headers match `site.rs` byte for byte.
- **Static files and redirects (`assets.rs`, `errors.rs`):** `app.css` follows the R44 order of SDD §8.3; `normalise` and `LOCATION_SET` were checked for `?d=`.
- **Islands:** the stream, theme and status islands and the budget test. The stream wire encoder in `stream.rs` matches its fixture.
- **Guides:** all 31 tool forms map as required (dns 10, email 10, tls 9, http 8, ip 3, `/` with `d` 1). The `lens.netray.info` and `/api/lens` rewrites were applied. The index order and categories match `site/guide/index.html`. The six og PNGs are byte-identical to `site/og/`.
- **Compare:** all 41 rows of `compare.toml` checked against the catalogue.
- **Docs and repo checks:** the SDD and `CLAUDE.md` diffs, `comparison-page-rules.md`, `frontend-rules.md`, `test_web_crate.sh` and `test_no_codenames.sh`.

### Refuted

None: the refuter confirmed both MAJOR findings.

### Calibration

| finding | refuter answer | confidence | held | command |
|---|---|---|---|---|
| guide.html:5 h1 from the list title, not V1's main h1 | CONFIRMED | 9 | held | `curl -s localhost:18731/guide/email-auth \| grep -o "<h1>[^<]*</h1>"` → "…DMARC Explained"; `grep -o 'page-title">[^<]*' site/guide/email-auth.html` → "…DMARC" |
| pages_frame.rs:176 frame counted on 404/400/500 only | CONFIRMED | 8 | held | `grep -rln assert_frame crates/web/tests/` → pages_frame.rs, pages_errors.rs only |

### Roll call

| principle | answer | evidence |
|---|---|---|
| P03 | convergence | no codename on added lines in templates, compare.toml, crates/web/src; `pages_codenames.rs`, `test_no_codenames.sh` enforce it; no new `*Status`/`*Verdict` enum |
| P10 | absence | no check or scoring path changed |
| P12 | absence | no server outbound call or engine run added (island `fetch` is browser-side) |
| P13 | absence | no limiter change; pages routes merge beside the SPA fallback |
| P18 | convergence | no `reqwest::Client` added; every new serde struct in `compare.rs` denies unknown fields |
| P26 | absence | no new check id; compare ids validated against the catalogue |
| P35 | convergence | both rewritten rules docs and the SDD amendments arrive with repo checks |
| P36 | convergence | light-theme tokens from `build.rs`; netray column from the catalogue; sitemap from `pages::sitemap()` and the guide list |
| P40 | absence | no module added |

### Summary

Before refutation: reader 0/0/0, spec reader 0/2/0 (REQUIREMENTS met=32 partial=2 missed=0 contradicted=0). After refutation: reader 0/0/0, spec reader 0/2/0 — both MAJORs confirmed and held. verified 2, held 2. Roll call: answered 9, convergence 4, divergence 0, absence 5. The two MAJORs (guide `<h1>` from the list title instead of V1's main heading on eight guides; the exactly-once frame test covering the error pages only) are left as follow-ups for the content pass and the next pages test change.
