# Report: backend correctness

## Phase 1 — prism lints

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C2 | Requirement 2: prism lints through one function over the unique records of all resolvers (one per name, type, data; highest TTL kept); identical lint lines within a category emitted once | green | crates/mhost-prism/tests/lint_results_table.rs |
| C4 | GIVEN a signed zone whose DNSKEY answer carries its RRSIG (expiring in 3 days) WHEN prism lints it THEN mhost's near-expiry line appears | green | crates/mhost-prism/tests/lint_results_table.rs |
| C5 | GIVEN a signed zone answered identically by two resolvers WHEN prism lints it THEN "Found 1 KSK(s) and 1 ZSK(s)" | green | crates/mhost-prism/tests/lint_results_table.rs |
| C6 | GIVEN two resolvers returning different records for one name WHEN prism lints THEN both records count | green | crates/mhost-prism/tests/lint_results_table.rs |
| C7 | GIVEN one A record with TTL 45 from one resolver and 300 from another WHEN prism lints THEN no low-TTL warning | green | crates/mhost-prism/tests/lint_results_table.rs |
| C8 | GIVEN two identical lint lines in one category WHEN prism emits the category THEN the line appears once | green | crates/mhost-prism/tests/lint_results_table.rs |
| C9 | GIVEN one resolver WHEN prism lints THEN the lines equal today's, except identical lines collapse (two RSASHA1 keys give one deprecation line) | green | crates/mhost-prism/tests/lint_results_table.rs |

The table first carried requirement 1 (DO bit) and its scenario as C1 and C3. The operator dropped them on 2026-10-09: prism keeps the explicit RRSIG question for 0.23.0, because mhost 0.12.0 offers no DO bit (see `### Halt`); the spec is amended accordingly. The ids C2 and C4–C9 are kept as they appear in the commits.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 30176 | 62 |
| G1 (BLOCKER fix, 8.1) | 1 | sonnet | 34892 | 29 |

### Review

- BLOCKER (fixed in the second pass) | `crates/mhost-prism/src/api/check.rs` `unique_records` | the first version round-tripped the whole `Lookups` through serde and fell back to the raw input on any error, so one name that does not round-trip (a `\000` label, as in a compact-denial NSEC next name) undid the deduplication for every record type | now per `Lookup`: an untouched lookup is kept as is, a fully duplicated one is dropped without deserialising, only a partially duplicated one is round-tripped, and on error that one lookup alone is kept. Not covered by a test: such a name cannot be built through the serde fixtures the tests use, and mhost's own constructors are private.
- Sound per the reader: category order (`ns_lame`, `ns_delegation` after `ns`), highest TTL kept at the first copy's position, `done` counts taken after collapsing, the prism contract golden unchanged (literal events).

### Behavioural verification

skipped: `POST /api/check` against a locally started `netray dns crates/mhost-prism/prism.dev.toml` returned `{"events":[],"truncated":true}`; the log shows every lookup to 8.8.8.8 and 8.8.4.4 ending in `Received Timeout error` (no outbound DNS from this sandbox).

### Halt (resolved)

Resolved 2026-10-09 by the operator: requirement 1 is amended to keep the RRSIG question. Reason given: open-decisions. Requirement 1 cannot be built as specified on mhost 0.12.0: mhost's `Resolver` gives no way to set the DO bit. `ResolverOpts::to_proto` (`mhost-0.12.0/src/resolver/mod.rs:427-439`) never sets hickory's `validate`, and hickory-resolver 0.26.3 sets `edns_set_dnssec_ok` only from `validate` (`hickory-resolver-0.26.3/src/resolver.rs:358-361`). mhost's public `raw_dnssec_query` is non-recursive (RD=0, `resolver/raw.rs:236-246`), meant for authoritative servers, and the constructors that would turn raw hickory records into mhost `Lookup`s (`Lookup::from_records`, `Record::from_proto`) are `pub(crate)`.

## Phase 2 — IP reputation

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: lens scores an address from ifconfig-rs's booleans (`is_spamhaus`, `is_c2`, `is_tor` → Fail; `is_vpn` → Warn; else Pass); enriches only public addresses (`netray_common::target_policy`), a non-public one not sent and counted as not checked; a failed or timed-out public enrichment makes the section Errored; at most four IPv4 and four IPv6, each family sorted; "checked N of M addresses" when fewer checked than resolved | green | crates/lens/tests/ip_reputation_flags.rs |
| C2 | R4: ifconfig-rs classifies with one function in `/json`'s order (internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential), used by `/json`, `/network` and `/range` (on the range's network address) | green | crates/ifconfig-rs/src/classify_tests.rs |
| C3 | GIVEN the ifconfig golden with `type: cloud` and `is_spamhaus: true` WHEN lens scores it THEN reputation Fail | green | crates/lens/tests/ip_reputation_flags.rs |
| C4 | GIVEN `is_vpn: true` and no other flag WHEN scored THEN Warn; GIVEN no flag THEN Pass | green | crates/lens/tests/ip_reputation_flags.rs |
| C5 | GIVEN every enrichment call answering 500 WHEN scored THEN the IP section is Errored and the result incomplete | green | crates/lens/tests/ip_enrichment_errors.rs |
| C6 | GIVEN one of two addresses timing out WHEN scored THEN Errored | green | crates/lens/tests/ip_enrichment_errors.rs |
| C7 | GIVEN ten A and two AAAA addresses WHEN lens enriches THEN four A (lowest four sorted) and two AAAA are enriched and the detail says "checked 6 of 12 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C8 | GIVEN five A addresses WHEN lens enriches THEN the lowest four, detail "checked 4 of 5 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C9 | GIVEN three A addresses WHEN lens enriches THEN all three, no "checked" detail | green | crates/lens/tests/ip_sampling.rs |
| C10 | GIVEN A 198.51.100.7 and A 10.0.0.5 WHEN lens enriches THEN only 198.51.100.7 is sent, the section is scored (not Errored), detail "checked 1 of 2 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C11 | GIVEN one fixture address WHEN ifconfig-rs answers `/json`, `/network` and `/range` for `<addr>/32` THEN all three report the same type | green | crates/ifconfig-rs/src/classify_tests.rs |
| C12 | R7: `[rate_limit] exempt_cidrs` (default empty); a TCP peer inside skips the per-IP limiter, matched on the peer and never a forwarded header; a non-CIDR entry fails validation; per-target limiter still applies; the production fixture carries the key | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C13 | GIVEN `exempt_cidrs = ["172.30.0.0/24"]`, 172.30.0.0/24 trusted, burst 1 WHEN peer 172.30.0.5 sends three requests with XFF 198.51.100.1 THEN none is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C14 | GIVEN the same config WHEN a non-exempt peer (trusted 172.31.0.2 or untrusted 203.0.113.9) sends two requests with XFF 172.30.0.5 THEN the second is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C15 | GIVEN no `exempt_cidrs`, burst 1 WHEN peer 172.30.0.5 sends two requests THEN the second is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C16 | GIVEN `exempt_cidrs = ["nope"]` WHEN validated THEN it fails naming `rate_limit.exempt_cidrs` | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C17 | GIVEN `ifconfig.production.toml` WHEN loaded THEN it carries `rate_limit.exempt_cidrs` and validates | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |

RED was confirmed at 7b4bcfa. Every new test failed to compile against the missing interface: the `allow` parameter and field, and `NetworkFlags`/`classify_network_type`.

Two harness defects in `classify_tests.rs` surfaced only once it compiled. I repaired them, and the production commit carries `ADLC-Test-Change` for them:
- The bare router has no `requester_info_middleware`, so the test now inserts `RequesterInfo` itself.
- `/network` negotiates HTML without an `Accept` header.

`lens_golden.rs` now serves the DNS golden's A record 192.0.2.10 as 1.1.1.1. 192.0.2.10 is a documentation address, which the production policy does not enrich. With the substitution, the IP section stays scored and no lens golden moves.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C2, C11) | 1 | sonnet (after the two test-harness repairs above) | 28873 | 74 |
| G2 (C1, C3–C10) | 3 | sonnet | 38208 | 104 |
| reader fixes (second pass) | 1 | sonnet | 35329 | 99 |
| G3 (C12–C17) | 1 | sonnet | 30368 | 84 |

### Review

The reader raised three BLOCKERs. The second pass fixed all three:
- **`/range`'s datacenter flag.** `/range` left out the `asn_info` Hosting term, so its classifier got different inputs than `/json`'s. It now includes it (`crates/ifconfig-rs/src/routes.rs` `range_handler`).
- **NotApplicable rendered as error.** A NotApplicable section showed as status `error`, so `overall` read `error` while `complete` was true. It now shows as `skip` (`crates/lens/src/routes.rs` `section_status_from_verdicts`, with the unit test `not_applicable_ip_section_is_skip_and_not_overall_error`).
- **PTR lookup.** lens asked for a PTR lookup it never reads, and that lookup could exceed lens's 2 s ip timeout and error the section. lens now sends `&dns=false` (`crates/lens/src/backends/ip.rs`).

AMENDMENTs:
- AMENDMENT | crates/lens/src/backends/ip.rs | The spec is silent on the rate budget between lens and ifconfig-rs. One check now sends up to 8 `/json` calls (5 before), and ifconfig-rs allows a burst of 10 and 60 a minute per forwarded client IP, so two quick checks of an 8-address domain can hit 429. A 429 now makes the section Errored. | affected_phase: 2 | repaired_in_phase: yes. The operator decided on 2026-10-09 to repair it through requirement 7; see "Requirement 7" below.
- AMENDMENT | spec.md Context | "No lens golden moves through … IP" holds only because the golden harness substitutes a public address for prism.sse's documentation address 192.0.2.10. | affected_phase: 2 | repaired_in_phase: yes
- AMENDMENT | crates/lens/src/backends/ip.rs | The spec is silent on a domain with no public address. Implemented as `NotApplicable("no public addresses")`: the section is excluded from scoring, `complete` stays true and the status is `skip`. This follows the decision log's "would be incomplete forever"; P10's list of N/A-by-design cases gains this one. | affected_phase: 2 | repaired_in_phase: yes

DEFERRED:
- The frontend shows a NotApplicable section's reason only for status `error`. A skipped IP or email section shows a grey dot with no reason, and the IP section's link has `href=""` (`IpSection.tsx`, `EmailSection.tsx`). This is UI work (V2).

NIT:
- No test pins that production lens wires `is_allowed_target` (`crates/lens/src/state.rs`). The tests inject a policy that admits documentation addresses.

### Behavioural verification

skipped: lens's entry point (`POST /api/check`) needs prism, tlsight, spectra, beacon and ifconfig-rs running. `lens_golden` and the `ip_*` tests drive it in-process against stubs. ifconfig-rs's `/json`, `/network` and `/range` are driven through its real router in-process by `json_network_and_range_report_the_same_type`.

### Requirement 7 — rate-limit exemption

**How it works today.** lens sends every backend call with `X-Forwarded-For: <visitor IP>` (`crates/lens/src/backends/mod.rs` `forward_headers`). lens reaches ifconfig-rs from the docker `backend` network, 172.30.0.0/24. ifconfig-rs lists that network in `server.trusted_proxies`, so `extract_client_ip` honours the header, and the per-IP limiter (`middleware::rate_limit`) keys on the visitor. A check's up to eight enrichment calls therefore come out of the visitor's own budget: a burst of 10, then 60 a minute.

**The change.**
- **The key.** ifconfig-rs gains `[rate_limit] exempt_cidrs`, empty by default; `deny_unknown_fields` stays and `test_config_strict.sh` passes.
- **The match.** A request whose TCP peer (`ConnectInfo`) lies in one of those networks skips the middleware's per-IP limiter. The match uses the connection peer and never a forwarded header, so a public client arriving through Traefik (peer 172.31.0.2) cannot claim the exemption with `X-Forwarded-For` (C14).
- **Validation.** An entry that is neither a CIDR nor an IP fails `Config::validate`.
- **What still applies.** The per-target limiter still applies. `/batch` and `/diff` charge tokens in their handlers and stay limited; no internal caller uses them, and the spec now says so.
- **Fixtures and examples.**
  - The production fixture carries `exempt_cidrs = ["172.30.0.0/24"]`, the subnet its `trusted_proxies` already names.
  - `ifconfig.example.toml` documents the key.

Spec amended in 694984b: requirement 7 and its scenarios, plus the decision-log entry.

**Reader findings.**
- AMENDMENT (`/batch` and `/diff` stay limited for an exempt peer): repaired in the spec wording.
- AMENDMENT (the fixture exempted nothing): repaired with the fixture value, and `c17` now pins it.
- NIT, not acted on: an IPv4 exempt CIDR does not match an IPv4-mapped peer on a dual-stack `[::]` bind. This fails closed, `trusted_proxies` has the same gap, and production binds `0.0.0.0`.

**New K item for the planning session (argus-oci).**

> **K — ifconfig-rs exempts lens from its per-client limit.** `ansible/roles/app_stack/templates/ifconfig.toml.j2`, `[rate_limit]`: add `exempt_cidrs = {{ ifconfig_rs_rate_limit_exempt | to_json }}`, with `ifconfig_rs_rate_limit_exempt: ["{{ backend_subnet }}"]` (172.30.0.0/24, the network lens reaches ifconfig on) in `ansible/inventory/group_vars/all/main.yml`. This is safe only while Traefik has no address on `backend`; today it sits on `edge`, `traefik-proxy`, `observability` and `socket-proxy`. A tighter alternative is a fixed `ipv4_address` for lens on `backend` and a `/32` exemption. Needs the release carrying backend-correctness requirement 7. Until then the key is unknown to the deployed ifconfig-rs, and `deny_unknown_fields` would refuse to start.

## Phase 3 — Messages as text

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Requirement 5: lint/check messages render as text in both frontends, the snapshot HTML and SVG/OG text; the Markdown exports of lens, prism, beacon, tlsight and spectra put every message, every target-read value and the domain in an inline code span with a fence longer than any backtick run; a `tests/repo` convention test refuses raw-HTML sinks in the frontends | green | covered by C2–C12; OG/SVG text: existing `svg_domain_is_escaped`, `svg_label_is_escaped` in crates/lens/src/og/render.rs |
| C2 | GIVEN a prism lint result with the hostile message WHEN the lint tab renders THEN no `img` element, the text shows | already_implemented | crates/mhost-prism/frontend/src/components/LintTab.test.tsx |
| C3 | GIVEN a lens DNS check with the hostile message WHEN `CheckList` renders THEN no `img` element | already_implemented | crates/lens/frontend/src/components/CheckList.test.tsx |
| C4 | GIVEN a snapshot with the hostile message WHEN `/r/<id>` renders THEN the HTML contains `&lt;img` and no `<img src=x` | already_implemented | crates/lens/tests/snapshot_routes.rs (`snapshot_page_renders_finding_message_as_text`) |
| C5 | GIVEN the hostile message WHEN lens's `toMarkdown` runs THEN it appears only inside a code span | green | crates/lens/frontend/src/lib/export_code_spans.test.ts |
| C6 | GIVEN `MX www.example.com. has no A/AAAA records` WHEN exported THEN the host is inside a code span | green | crates/lens/frontend/src/lib/export_code_spans.test.ts |
| C7 | GIVEN a message containing a backtick WHEN exported THEN the fence is longer and the message survives verbatim | green | crates/lens/frontend/src/lib/export_code_spans.test.ts |
| C8 | GIVEN the hostile value in a TXT record WHEN prism's Markdown export runs THEN it appears only inside a code span | green | crates/mhost-prism/frontend/src/lib/export.test.ts |
| C9 | GIVEN a frontend file assigning `innerHTML` WHEN the convention test runs THEN it fails; GIVEN the tree THEN it passes | already_implemented | tests/repo/test_no_raw_html_sinks.sh (fixture tests/repo/fixtures/raw-html-sink/sink.ts) |
| C10 | GIVEN the hostile value as a category detail and a sub-check detail WHEN beacon's Markdown export runs THEN only inside code spans, the domain too | green | crates/beacon/frontend/src/components/SummaryCard.export.test.tsx |
| C11 | GIVEN the hostile value as a quality check detail, a certificate subject and an error message WHEN tlsight's Markdown export runs THEN only inside code spans, the hostname too | green | crates/tlsight/frontend/src/components/ExportButtons.test.tsx |
| C12 | GIVEN the hostile value as a quality check message, a header value, a redirect location and a CORS message WHEN spectra's Markdown export runs THEN only inside code spans, the URL too | green | crates/spectra/frontend/src/components/ExportButtons.test.tsx |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C5, C6, C7) | 1 | sonnet | 19312 | 29 |
| G2 (C8) | 1 | sonnet | 17517 | 16 |
| G3 (C10, amendment) | 2 | sonnet (after the test mock was fixed) | 22426 | 44 |
| G4 (C11, amendment) | 1 | sonnet (after the test mock was fixed) | 27331 | 47 |
| G5 (C12, amendment) | 2 | sonnet (after the test mock was fixed) | 22298 | 34 |

### Review

The first reading found three BLOCKERs, fixed through section 7 once (1 coder run, sonnet, 18519 tokens, 20 s). New tests: `packages/common-frontend/src/inlineCode.test.ts`, `crates/lens/frontend/src/lib/export_line_endings.test.ts`, `crates/mhost-prism/frontend/src/lib/export_line_endings.test.ts`.

- BLOCKER (fixed) | crates/lens/frontend/src/lib/export.ts:54,78 | a message with a line ending ended the list item, so the rest of it was block-level Markdown outside any code span | a DMARC `rua` detail carrying `\n` and an HTML line. `inlineCode` now turns `\r\n`, `\r` and `\n` into one space.
- BLOCKER (fixed) | crates/mhost-prism/frontend/src/lib/export.ts:137 | a lone `\r` (a CommonMark line ending) split the table row | a TXT value `a\r<div>…</div>`. Handled by `inlineCode`, so `mdCode` only escapes `|`.
- BLOCKER (fixed) | packages/common-frontend/src/utils.ts | `inlineCode('')` gave two backticks, which is literal text and not a code span | an empty TXT record. It now returns ''.
- AMENDMENT | crates/beacon/frontend/src/components/SummaryCard.tsx:63,65 | the Goal says messages are text "everywhere … the Markdown export", but requirement 5 names only lens's and prism's exports. beacon's Markdown export and the tlsight and spectra `ExportButtons.tsx` exports still insert details and messages raw | a beacon DMARC detail that carries an `<img …>` tag renders as HTML | affected_phase: 3 | repaired_in_phase: no

### Behavioural verification

skipped: the only entry point is the browser's export button. `toMarkdown` is a pure function, and the vitest runs above call it directly with the hostile fixture. No dev server runs in this worktree.

### Amendment: beacon, tlsight and spectra exports

The operator decided on 2026-10-09 that the AMENDMENT above joins requirement 5. The spec was amended in `2f2d624` and gained one scenario per export (C10–C12).

The three tests first mocked `@netray-info/common-frontend/utils` with only `copyToClipboard` and `downloadFile`. That hid `inlineCode` from the components, so the coders could not go green from production code alone. The mocks now spread `importOriginal()`. The coders' production edits were right as first written; all three suites went green once the mocks were fixed.

The reading of the amendment diff:
- BLOCKER (fixed) | crates/spectra/frontend/src/components/ExportButtons.tsx:72 | the cookie's `SameSite` value came from the target and went out raw | `Set-Cookie: sid=1; SameSite=[x](javascript:alert(1))` produced a live link. It is now in a code span. The spectra fixture gained a cookie with the hostile name and SameSite (1 coder run, sonnet).
- DEFERRED | crates/spectra/frontend/src/components/ExportButtons.tsx:26 | the `**IP**` line has no trailing hard break, so it renders on one line with the Org/Category line that follows | the same at HEAD; layout only.

## Phase 4 — `@system`

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 6: `/api/config` reports whether system resolvers are allowed; the UI lists and documents `@system` only when it said so (hidden before the answer and on failure); the server keeps refusing it; the production fixture sets `[dns] allow_system_resolvers = false` | green | covered by C2–C7 |
| C2 | GIVEN `allow_system_resolvers = false` WHEN `/api/config` is read THEN it reports system resolvers off | green | crates/mhost-prism/tests/system_resolvers.rs |
| C3 | GIVEN the config off WHEN the query input suggests servers and the help opens THEN no `@system` suggestion and no help row | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C4 | GIVEN the config on WHEN the same THEN both appear | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C5 | GIVEN `/api/config` not yet answered or failing WHEN the input renders THEN no `@system` suggestion | green | crates/mhost-prism/frontend/src/lib/servers.test.ts |
| C6 | GIVEN `allow_system_resolvers = false` WHEN a query names `@system` THEN `SYSTEM_RESOLVERS_DISABLED`, as today | already_implemented | crates/mhost-prism/tests/system_resolvers.rs |
| C7 | GIVEN `prism.production.toml` WHEN loaded THEN `allow_system_resolvers` is false | green | crates/mhost-prism/tests/production_fixture.rs |
| C8 | GIVEN `allow_system_resolvers = false` WHEN `POST /api/parse` completes `example.com @sy` or `example.com ` THEN no `@system` completion; GIVEN it true THEN `@system` is offered (added by the operator's amendment of req 6) | green | crates/mhost-prism/tests/parse_system.rs |

C6 held before the phase; its test `system_server_is_refused_when_system_resolvers_disabled` (GET `@system` and POST `"system"`) stays as a pin. C3–C5 test the pure helpers in `frontend/src/lib/servers.ts`, which `QueryInput` and the help modal now render from: prism's vitest runs in `node` without jsdom.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C2) + G2 (C7), one coder | 1 | sonnet | 17042 | 19 |
| G3 (C3, C4, C5) | 1 | sonnet | 25132 | 41 |
| G5 (C8) | 1 | sonnet | 18519 | 25 |

### Review

- BLOCKER (fixed) | `crates/mhost-prism/frontend/src/components/QueryInput.tsx:4` | the `@codemirror/autocomplete` import was corrupted by an orchestrator re-indent edit after the coder's green run; `tsc` failed and the embedded dist would have stayed stale. Restored; `tsc --noEmit` clean, vitest 72/72, `npm run build` green, the rebuilt bundle reads `allow_system_resolvers`.
- DEFERRED (raised as BLOCKER, refuted) | `crates/mhost-prism/frontend/src/lib/servers.test.ts:2` | C3–C5 test the pure helpers, not `QueryInput`/`App` rendering, and the gate runs no `tsc` on prism's frontend. Not a wrong result: the pure-function test was chosen because prism's vitest has no jsdom; a rendering test belongs with the jsdom setup another branch adds. The gate not type-checking the frontend is a gate gap outside this phase.
- AMENDMENT | `crates/mhost-prism/src/api/parse.rs:89` | `POST /api/parse` (public: "Parse a query string and get completions. Useful for building UIs.") always offers `@system` as a completion; its handler takes no `State`. With the production config, `{"input":"example.com @sy"}` completes to `@system`, which the query then refuses. Requirement 6 named only `/api/config` and the UI. | affected_phase: 4 | repaired_in_phase: yes — the operator amended req 6 (2026-10-09, `a427d44`); C8 covers it, and `parse_handler` takes `State` and drops `@system` when disallowed.
- Second reading, over the `/api/parse` diff: no BLOCKER, no AMENDMENT. The filter works on constant labels, so `@System`/`@SY` still yield and lose `@system`; `@public`/`@all` never expand to system resolvers; `completions_at` has no other caller; completion and refusal read the same `state.config` field.
- DEFERRED | `crates/mhost-prism/src/api/parse.rs:264` | older than this phase: `cursor_pos` is a byte offset clamped only to `input.len()`, so a cursor inside a multi-byte character panics on the slice and drops the connection (`{"input":"example.com @é","cursor_pos":14}`).
- Traced sound by the reader: hot reload (both `/api/config` and `QueryPolicy` read the startup `state.config`); Solid props are read lazily in the completion source; `For` over `helpServerRows(allowSystem())` is reactive; the signal starts false and a failed fetch leaves it false; no other place lists `@system` (frontend, README, site/, `/api/servers`).

### Behavioural verification

`netray dns` with the production fixture, then with defaults (`PRISM_SERVER__BIND=127.0.0.1:18089`):

```
--- netray dns crates/mhost-prism/tests/fixtures/prism.production.toml
$ curl -s localhost:18089/api/config
{"site_name":"prism","version":"0.22.2","ifconfig_url":"https://ip.example.com","tls_url":"https://tls.example.com","allow_system_resolvers":false,"ecosystem":{...}}
$ curl -s "localhost:18089/api/query?q=example.com%20A%20@system"
{"error":{"code":"SYSTEM_RESOLVERS_DISABLED","message":"system resolvers disabled"}}
--- netray dns (defaults)
$ curl -s localhost:18089/api/config
{"site_name":"prism","version":"0.22.2","ifconfig_url":null,"tls_url":null,"allow_system_resolvers":true}
```

`/api/parse` after the amendment, same two configs:

```
--- netray dns crates/mhost-prism/tests/fixtures/prism.production.toml
$ curl -s -X POST -H 'content-type: application/json' -d '{"input":"example.com @sy"}' localhost:18089/api/parse | jq -c '[.completions[].label]'
[]
--- netray dns (defaults)
$ curl -s -X POST -H 'content-type: application/json' -d '{"input":"example.com @sy"}' localhost:18089/api/parse | jq -c '[.completions[].label]'
["@system"]
```

After `npm run build` and a rebuild, the served bundle `/assets/index-D1f3bPxI.js` contains `allow_system_resolvers===!0`, the strict check in `systemResolversAllowed`. No headless click: this worktree has no Playwright install (`tests/acceptance` has no `node_modules`), so the suggestion list and the help rows are verified through `servers.test.ts` only.
