# Report: backend correctness

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

