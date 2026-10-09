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
