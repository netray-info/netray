# Plan: backend correctness

## Phase 3 — Messages as text

### Groups

- G1: C5, C6, C7 (lens Markdown export)
- G2: C8 (prism Markdown export; reuses G1's helper)

C2, C3, C4 and C9 pass at the baseline: their tests pin what holds. C1 is covered by C2–C9 and by the existing OG/SVG escape tests in `crates/lens/src/og/render.rs`.

### Plan

**G1.**
- `packages/common-frontend/src/utils.ts`: add `inlineCode(value)`. It returns a CommonMark inline code span whose fence is one backtick longer than the longest backtick run in the value. It pads with one space when the value starts or ends with a backtick, or starts and ends with a space. It does not escape `|` or newlines; that is the caller's job. The package is a workspace member and is served from `src` through the existing `./utils` export, so it needs no publish and no version bump.
- `crates/lens/frontend/src/lib/export.ts` `toMarkdown`: put the heading's domain in `inlineCode`. Check messages in `renderSection` and the IP loop become `messages.map(inlineCode).join('; ')`.

**G2.**
- `crates/mhost-prism/frontend/src/lib/export.ts` `toMarkdown`: the heading becomes `inlineCode(ctx.query)`. A table-cell helper next to `mdEscape` collapses newlines, wraps the value with `inlineCode`, then escapes `|`. Name and Value cells use it.
- Test setup already in the tree, committed with this group: prism `package.json` gains the dev deps `jsdom` and `@solidjs/testing-library`; `vitest.config.ts` includes `src/**/*.test.tsx`; `package-lock.json` is updated.

### Groups (amendment, 2026-10-09)

- G3: C10 (beacon) · G4: C11 (tlsight) · G5: C12 (spectra). They share no production file. Each reuses `inlineCode` from `@netray-info/common-frontend/utils`.

### Plan (amendment)

**G3.** In `crates/beacon/frontend/src/components/SummaryCard.tsx` `ExportButtons.copyMarkdown`, the heading domain, `result.detail` and each `sc.detail` go through `inlineCode`.

**G4.** In `crates/tlsight/frontend/src/components/ExportButtons.tsx` `copyMarkdown`, the heading hostname, each chain subject, `ip.error.message` and every quality `c.detail` go through `inlineCode`.

**G5.** In `crates/spectra/frontend/src/components/ExportButtons.tsx` `copyMarkdown`, the following go through `inlineCode`: the heading URL, the enrichment IP, org and threat, quality `c.message`, header values and messages, CSP issues, cookie names (which replace their hand-written single backticks), `cors.message`, and each redirect hop's URL and location. The test setup ships with this group: spectra's `package.json` gains a `test` script and the dev deps vitest, jsdom and @solidjs/testing-library, and `package-lock.json` is updated.
