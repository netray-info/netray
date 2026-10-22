# Report: v2-http-checks

## Phase 1 — Check fixes: Referrer-Policy lists, inspection duration, Set-Cookie rows

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 Referrer-Policy graded by its effective policy (last known comma-separated token) | green | crates/http/tests/referrer_policy_list.rs |
| C2 | R2 http.server carries inspection_ms in place of response_ms, volatile likewise; lens fills response_duration_ms from it | green | crates/http/tests/server_inspection_ms.rs |
| C3 | R3 http.headers holds one row per Set-Cookie value (set-cookie, set-cookie#n) | green | crates/http/tests/set_cookie_rows.rs |
| C4 | R4 a volatile row path covers the rows keyed key#n | green | crates/model/tests/volatile_row_repeats.rs |
| C5 | R13 SDD states the phase's changes | test-unwritable | specs/sdd/v2.md (prose) |
| C6 | origin-when-cross-origin, strict-origin-when-cross-origin passes | green | crates/http/tests/referrer_policy_list.rs |
| C7 | no-referrer, unsafe-url warns Permissive policy: unsafe-url | green | crates/http/tests/referrer_policy_list.rs |
| C8 | strict-origin, made-up-policy passes | green | crates/http/tests/referrer_policy_list.rs |
| C9 | foo, bar warns Unrecognized value: foo, bar | green | crates/http/tests/referrer_policy_list.rs |
| C10 | padded mixed-case Strict-Origin passes | green | crates/http/tests/referrer_policy_list.rs |
| C11 | fixture http.server holds inspection_ms, no response_ms, validates | green | crates/http/tests/server_inspection_ms.rs |
| C12 | volatile list holds http.server/inspection_ms, not response_ms | green | crates/http/tests/server_inspection_ms.rs |
| C13 | lens response_duration_ms read from inspection_ms | green | crates/lens/src/modules.rs |
| C14 | three Set-Cookie values give rows set-cookie, set-cookie#2, set-cookie#3 | green | crates/http/tests/set_cookie_rows.rs |
| C15 | two Vary values stay one joined row | already_implemented | crates/http/tests/set_cookie_rows.rs |
| C16 | JSON replay with one set-cookie gives one row | already_implemented | crates/http/tests/set_cookie_rows.rs |
| C17 | differing Set-Cookie rows leave fingerprint and diff unchanged | green | crates/model/tests/volatile_row_repeats.rs |
| C18 | rows keyed set-cookiex or set-cookie#a stay compared | already_implemented | crates/model/tests/volatile_row_repeats.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 25611 | 18 |
| G2 | 1 | sonnet | 33267 | 70 |
| G3 | 1 | sonnet | 35901 | 339 |

### Review

One BLOCKER (Referrer-Policy read only the first header line) fixed in the phase: every line is graded joined with `, `
(`referrer_policy_lines_are_one_list`). Two AMENDMENTs repaired in the phase: a received `set-cookie#<n>` header keeps its
row (`set_cookie_rows_skip_a_received_set_cookie_hash_header`); the decision log now says when the `response_ms` removal
shows in a diff. NIT: the catalogue finding now names a value not readable as text. NIT: the trailers for the reformatted
and superseded protected tests are on the phase commit.

### Behavioural verification

skipped: the HTTP inspector resolves its target through public DNS, blocked locally (Little Snitch); the phase's paths
(`translate`, the canonical form, lens's http section) run offline in the tests above.

## Phase 2 — Parsed evidence: CSP, HSTS, caching, CDN, isolation headers

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R5 CSP analysis yields structured issues (directive, severity); status from severities and report-only; V1 issue texts unchanged | green | crates/http/tests/csp_structured_issues.rs |
| C2 | R6 http.csp tree block (root report_only, issue_count, issues on absent directives; one child per directive) | green | crates/http/tests/csp_evidence.rs |
| C3 | R7 csp check references http.headers and http.csp; findings are the issue messages, report-only finding first | green | crates/http/tests/csp_evidence.rs |
| C4 | R8 http.hsts fields block; hsts check references it | green | crates/http/tests/hsts_evidence.rs |
| C5 | R9 http.caching fields block; caching check references it | green | crates/http/tests/caching_evidence.rs |
| C6 | R10 http.cdn fields block when a CDN was detected | green | crates/http/tests/cdn_isolation_evidence.rs |
| C7 | R11 http.isolation fields block with coop, coep, corp and missing | green | crates/http/tests/cdn_isolation_evidence.rs |
| C8 | R12 register declares the new keys; five new volatile paths | green | crates/http/tests/parsed_evidence_register.rs |
| C9 | R13 SDD states the phase's changes | test-unwritable | specs/sdd/v2.md (prose) |
| C10 | script-src only gives a fail issue on default-src and status fail; a clean policy but unsafe-inline gives one warn issue on script-src | green | crates/http/tests/csp_structured_issues.rs |
| C11 | security.csp.issues equals the structured issue messages in order | green | crates/http/tests/csp_structured_issues.rs |
| C12 | report-only CSP without issues warns | green | crates/http/tests/csp_structured_issues.rs |
| C13 | enforced CSP gives the tree with root issue on base-uri, children default-src then script-src | green | crates/http/tests/csp_evidence.rs |
| C14 | report-only CSP gives report_only true and the report-only finding first | green | crates/http/tests/csp_evidence.rs |
| C15 | no CSP header gives no http.csp, references headers only, finding No Content-Security-Policy header | already_implemented | crates/http/tests/csp_evidence.rs |
| C16 | two issues give two findings each equal to an issue in http.csp, references headers and csp | green | crates/http/tests/csp_evidence.rs |
| C17 | HSTS max-age and includeSubDomains in http.hsts; hsts check references headers and hsts | green | crates/http/tests/hsts_evidence.rs |
| C18 | HSTS without max-age gives no max_age; no header gives no http.hsts | green | crates/http/tests/hsts_evidence.rs |
| C19 | caching headers give directives, max_age, etag, etag_weak, last_modified, vary, age; caching check references both | green | crates/http/tests/caching_evidence.rs |
| C20 | unparsable Last-Modified omitted; no caching header gives no http.caching | green | crates/http/tests/caching_evidence.rs |
| C21 | cf-ray and cf-cache-status give http.cdn Cloudflare HIT and both indicators; none gives no block | green | crates/http/tests/cdn_isolation_evidence.rs |
| C22 | COOP only gives coop and two missing; all three give no missing | green | crates/http/tests/cdn_isolation_evidence.rs |
| C23 | refused HTTPS probe emits none of the five new blocks; evidence rule holds | already_implemented | crates/http/tests/parsed_evidence_register.rs |
| C24 | volatile list holds the five new paths naming registered keys; register types match | green | crates/http/tests/parsed_evidence_register.rs |
| C25 | catalogue and profile keep the same twelve HTTP check IDs | already_implemented | crates/http/tests/parsed_evidence_register.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 32577 | 35 |
| G2 | 1 | sonnet | 74861 | 1020 |

### Review

One BLOCKER fixed in the phase: Vary was read from its first header line only; Cache-Control and Vary now combine every
line (`list_headers_combine_their_lines`). AMENDMENTs repaired in the phase: `http.caching` stands whenever the probe answered
and `etag_weak` is volatile, so a volatile key alone never moves the block in a comparison (C20 rewritten under a test-change
trailer); Cache-Control over every line, logged. NITs acted on: integers above i64 omitted instead of wrapped; HTTP dates in all
three RFC 9110 forms (`http_date_reads_the_three_forms`); SDD says "not received or not readable". NITs left: the C11 assertion
reads the same function it checks; the C16 test checks membership, not order.

### Behavioural verification

skipped: the HTTP inspector resolves its target through public DNS, blocked locally (Little Snitch); `translate` over
assembled responses runs offline in the tests above.

### Checks to build

- A list-valued response header (`referrer-policy`, `cache-control`, `vary`) read with `HeaderMap::get` drops its later lines; a `tests/repo` check (`sg` over `crates/http/src/inspect/`) would refuse `.get("<list header>")` and require `get_all`.
