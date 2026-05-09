# Comparison Page Rules

Applies to: `site/compare/index.html`  
Read this when: adding or removing a tool from the suite, adding new checks to an existing tool, or doing the quarterly review pass.

---

## When the page must be updated

Flag `site/compare/index.html` for update whenever any of the following happens:

| Event | What to update |
|-------|---------------|
| New tool added to the suite (e.g. `regis.netray.info`) | Add a new axis section to Table 2; if it is also a unified checker, add a column to Table 1 |
| Existing tool gains a new check (e.g. beacon adds DANE, tlsight adds OCSP stapling) | Find the corresponding row(s) in Table 2 and flip `✗` → `✓` or `~`; update the `<details>` text |
| Existing tool drops or renames a check | Reverse of the above |
| A competitor gains or loses a feature (verified against their docs) | Update the affected cell and `<details>` text |
| Quarterly review pass | Re-verify all cells, bump "Last reviewed" date in `<footer>` |

**Do not silently update cells.** If a cell changes, explain why in the commit message so the diff is auditable.

---

## How the comparison was compiled

### Methodology

1. **Four-symbol system** — every cell holds exactly one of:
   - `✓` full coverage — the tool does this well by default
   - `~` partial or lower depth — the tool touches this but with meaningful gaps
   - `✗` not done — the tool does not cover this check at all
   - `—` out of scope — the check is conceptually outside what this tool does (e.g. asking badssl.com to scan your server)

2. **Source of truth** — claims were verified against vendor documentation and hands-on checks using `example.com` as the test domain. Where documentation was ambiguous, the tool was queried directly and the result observed.

3. **Admitted gaps** — the "Where lens trades depth for breadth" section names concrete limitations of lens vs. SSL Labs, hardenize, and mxtoolbox. This is intentional product positioning: honesty builds more trust than parity claims.

4. **No Registration row** — the `regis.netray.info` axis is deferred until that tool ships to production (see `specs/sdd/domain-registration-inspector.md`). A "coming soon" row ages badly in screenshots.

### Table structure

- **Table 1 — Unified domain checkers**: peer tools that also give a single broad domain health picture. Columns: `Feature | lens | hardenize | internet.nl | observatory.mozilla.org`.
- **Table 2 — Per-axis specialists**: low-level or single-axis tools grouped by the axis they cover (DNS, TLS, HTTP, Email, IP). Each axis has its own column set because the tools are not comparable across axes.

The split matters: `dig` is not a competitor to lens; it is a building block. Conflating the two tables muddies the headline story.

### Adding a new tool to the suite

When a new netray.info tool ships:

1. Add its axis section to Table 2 with `✗` in all cells initially, then fill based on actual capability.
2. Decide whether it also belongs in Table 1 (only if it offers a *unified* multi-check view; single-axis tools belong only in Table 2).
3. If it belongs in Table 1, add a new `<th scope="col">` column and fill every row.
4. Add the new tool's sub-domain link to `<p class="page-footer__suite">` at the bottom of the page.
5. Bump "Last reviewed" to today's date.
6. Commit with `feat(site): add <toolname> axis to comparison page`.

### Quarterly review pass

1. Open each competitor's documentation and re-verify cells that are likely to have changed (new features, deprecations).
2. Run a quick hands-on check of `example.com` through each unified tool.
3. Update any changed cells and expand/update the `<details>` text.
4. Update the "Where lens trades depth for breadth" section if any gap has closed.
5. Bump "Last reviewed: YYYY-MM-DD" in `<footer>`.
6. Commit with `docs(site): comparison page quarterly review YYYY-MM-DD`.

---

## Page implementation notes

- **Self-contained HTML** — no external stylesheet or JS. Guide CI is inlined in `<style>`. This is intentional: the page must render correctly when opened as a local `file://` for review.
- **No JS** — row expansion uses native `<details>`/`<summary>`. Do not add JavaScript.
- **Sitemap** — after any edit, run `node site/scripts/build-sitemap.mjs` to keep `sitemap.xml` current (though for content-only edits the URL is already present; this matters only when adding/removing pages).
- **"Last reviewed" date** — always update it. It is the only date visible to readers; stale dates erode trust faster than stale data.
