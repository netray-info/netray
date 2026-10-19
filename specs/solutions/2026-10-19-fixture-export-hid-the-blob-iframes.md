---
class: environment-decides-the-outcome
repeat-of: specs/solutions/2026-10-18-debuginfo-filled-the-ci-runner-disk.md
---
# The board side passed on a fixture export and timed out on the real one

**What failed.** `tests/acceptance/design/board.spec.ts` built its export with `about:blank` iframes filled by `contentDocument.write`, so `openExport` waited on `iframe.contentDocument.body` from the parent and passed. The real canvas export (private) swaps each board to a sandboxed `blob:null/…` iframe: the parent sees `contentDocument === null`, and `just design-check` exited 2 with `page.waitForFunction: Timeout 30000ms exceeded`.
**What worked.** Wait inside each board's own frame: `handle.contentFrame()` then `frame.waitForFunction(() => document.body?.children.length > 0)` (`tests/acceptance/design/board.mjs`, `openExport`). Playwright reaches cross-origin frames over CDP where page script cannot.
**How to notice next time.** A browser step green on a hand-built fixture times out on the artefact it imitates; compare the fixture's frame origin with the real one before trusting it.
