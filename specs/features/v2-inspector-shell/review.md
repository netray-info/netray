## d43b0f9d936b77aa0ce0fa0ddb8db9ca9404839c..5299457f84570767bc2eaf3f7368ca8fe6171d20

### Reader

COUNTS blockers=0 majors=1 minors=1
LENSES Engineering, Security, Testing
MAJOR | crates/web/src/pages/result/run.rs:159 | When the head is fresh, the result page's answer to a visit stream swaps the header, spectrum, issues and evidence for the store's current head but always leaves out the `inspect-export` fragment. If a different entry was recorded after the page rendered, the page then shows the new entry while the Markdown, CSV and JSON exports still hold the old one, or there are no export controls at all. | 1) `GET /example.com` while the head E1 is stale. The page sets `autostart` and `entry_export(E1)` goes into `#inspect-export` (mod.rs:624). 2) Another client's run of example.com records E2 with different results. 3) The page's autostart sends `POST /example.com/run` with `trigger=visit` and SSE. `is_fresh` is now true, so `stored_answer` streams E2's fragments without `inspect-export`. The page shows E2, but "JSON" downloads E1's `/api/v1/entries/<E1>` body. With no stored entry at step 1, the page keeps the hidden empty placeholder and shows E2 with no export controls.
MINOR | crates/web/islands/test/shell-export.test.ts:67 | The test "ignores clicks while disabled" still passes when the disabled guard it names (controls.ts:49) is deleted. | The disabled slot is built with `slot(true, false)` (line 29), which has no `<template>` elements. Remove `|| exporter.disabled` and `exportClick` still returns at `text == null` before it calls `writeText`, so `expect(writeText).not.toHaveBeenCalled()` holds either way.

```quote crates/web/src/pages/result/run.rs:159
                .filter(|(id, _)| id != "inspect-export")
```

```quote crates/web/src/pages/result/run.rs:144
/// already holds that head's export.
```

```quote crates/web/src/pages/result/mod.rs:624
    if let Some(export) = entries.first().and_then(entry_export) {
```

```quote crates/web/islands/test/shell-export.test.ts:67
  it("unhides the buttons at mount and ignores clicks while disabled", async () => {
```

```quote crates/web/islands/test/shell-export.test.ts:29
  `<div id="inspect-result"><div class="inspector-run__head">${slot(true, false)}</div></div></div>`;
```

```quote crates/web/islands/src/controls.ts:49
  if (!el.contains(exporter) || exporter.disabled) return;
```

### Spec reader

COUNTS blockers=0 majors=4 minors=1
REQUIREMENTS met=25 partial=4 missed=0 contradicted=0
MAJOR | crates/web/islands/src/shell.ts:80 | The shell exposes no provider hook. The DNS suggestion logic is written into `shell.ts`, and `data-suggest` only switches it on or off; the name it carries never selects a provider. | R16. A view that sets `data-suggest="http"` together with `data-suggest-types` runs the same `@`/`+`/record-type logic, because nothing looks the name up.
MAJOR | crates/web/src/export.rs:137 | `md_escape` misses some Markdown markup. `&` (entity references) and list markers at the start of a line (`-`, `+`, `1.`) pass through unchanged. | R19, which requires that "Markdown escapes every character a renderer would read as markup". An `email.mta_sts_policy` Text block (a body the domain controls) starting with `- mode` or `1. x` turns into a list in the export, and `&lt;b&gt;` renders as `<b>`.
MAJOR | crates/web/islands/src/page.ts:40 | On evidence views, `r` re-checks nothing. The handler only submits `#recheck`, which exists only in the result header, and the evidence help text admits this ("where the page offers it"). | R25 mounts "`r` re-checks" on every evidence view. Pressing `r` on `/example.com/tls` or `/r/<id>/tls` finds no `#recheck` and nothing happens.
MAJOR | specs/features/v2-inspector-shell/spec.md:600 | The design manifest leaves out two elements that R28 lists: the back link and the related-records box. The board is measured on an IP target (`1.1.1.1:443`), which renders neither. | R28 lists "back link" and "related-records box" among the manifest's G/GDark elements. The decision log and report row V4 say both "leave the manifest", which is not an operator acceptance of a measured row.
MINOR | crates/web/src/evidence.rs:380 | Every check row on stored evidence views now gets an extra "evidence →" link in its meta line, which no requirement asks for. | The stored view `/example.com/tls` already lists each row's block links (`evidence_section.html`). `build_section` now also sets `evidence_href`, so the stored rows' markup changes.

```quote crates/web/islands/src/shell.ts:80
    if (!el.dataset.suggest || start === 0 || start > caret || !tok) return close();
```
```quote specs/features/v2-inspector-shell/spec.md:228
16. **Autocomplete hook.** The shell exposes one hook: a provider named by the tool's view returns
```
```quote crates/web/src/export.rs:137
            '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '<' | '>' | '|' | '~' | '#' => {
```
```quote specs/features/v2-inspector-shell/spec.md:260
    JSON `errors` entry). Markdown escapes every character a renderer would read as markup. No
```
```quote crates/web/islands/src/page.ts:40
        const form = document.querySelector<HTMLFormElement>("#recheck");
```
```quote crates/web/src/pages/mod.rs:194
            (&["r"], "Check again, where the page offers it"),
```
```quote specs/features/v2-inspector-shell/spec.md:305
25. **Shared parts beyond inspectors.** The help dialog (with content for the page), the keyboard
    shortcuts (`/` focuses the idle page's or not-found page's domain input, `r` re-checks, `j` and
```
```quote specs/features/v2-inspector-shell/spec.md:600
  ends (a manifest `wait` selector): outbound DNS is blocked locally, so the back link and the
  related-records box (host targets only) leave the manifest and stay pinned by
```
```quote specs/features/v2-inspector-shell/spec.md:322
28. **Design gate.** The private manifest names the shell elements of boards G and GDark (header
    title and tagline, tabs, form card, label, input, submit, run title, h1, back link, band and its
```
```quote crates/web/src/evidence.rs:380
                evidence_href: links.first().map(|l: &BlockLink| l.href.clone()),
```

### Refuted

None: the refuter confirmed all five majors (confidence 6 to 8).

### Calibration

| finding | refuter | confidence | result | check |
|---|---|---|---|---|
| run.rs:159 export filtered from a fresh-head visit | CONFIRMED | 7 | held | read `stored_answer`: `.filter(|(id, _)| id != "inspect-export")` |
| shell.ts:80 no provider hook | CONFIRMED | 6 | held | read `suggest()`: `data-suggest` only gates the DNS logic |
| export.rs:137 md_escape misses `&` and line-start markers | CONFIRMED | 8 | held | read `Shape::Text` path: `md_escape(text)` as its own paragraph |
| page.ts:40 `r` on evidence views | CONFIRMED | 7 | held | `templates/pages/evidence.html` has no `#recheck` |
| spec.md:600 back link and related box not in the manifest | CONFIRMED | 7 | held | manifest boards G/GDark list 28 elements without them |

### Roll call

| principle | answer |
|---|---|
| P03 | convergence |
| P10 | absence |
| P12 | absence |
| P13 | convergence |
| P18 | convergence |
| P26 | absence |
| P35 | divergence: the design check's `wait` key has no tests/repo row |
| P36 | convergence |
| P40 | absence |

### Summary

Before refutation: reader 0/1/1, spec reader 0/4/1 (REQUIREMENTS met=25 partial=4 missed=0
contradicted=0). After refutation: 0 blockers, 5 majors, 2 minors; verified 5, held 5. Roll call:
answered 9, convergence 4, divergence 1, absence 4. The majors and the P35 divergence are fixed on
this branch in a follow-up range.

## 5299457f84570767bc2eaf3f7368ca8fe6171d20..218bb309

### Reader

COUNTS blockers=1 majors=0 minors=0
LENSES Engineering, Testing
BLOCKER | crates/web/src/export.rs:178 | `md_line` escapes a list marker only at byte 0, so evidence text with 1 to 3 leading spaces still renders as a Markdown list. | An MTA-STS policy body `"\n- mode: enforce"` stored as `Shape::Text`: `md_escape` turns the newline into a space, the output ` - mode: enforce` renders as a list (`pandoc -f commonmark`); `" 1. first"` as an ordered list.

```quote crates/web/src/export.rs:178
    if out.starts_with(['-', '+']) {
```

### Spec reader

skipped: a fix range of the review findings; the spec reader read the whole feature above.

### Summary

1 blocker, held (confirmed by reading `md_line`; the new rows in `tests/it/shell_review_final.rs`
failed before the fix). Fixed in the next commit (`md_line` trims leading whitespace before the
marker check); the fix range is read below.

## 218bb309..ec559a03

### Reader

COUNTS blockers=0 majors=0 minors=0
LENSES Engineering, Testing

No wrong result: `md_line` trims leading whitespace (which includes the spaces `md_escape` makes
of `\r` and `\n`) before the marker check; both new test rows fail on the old code.

### Spec reader

skipped: a fix range; the spec reader read the whole feature above.

### Summary

0/0/0; verified 0, held 0.
