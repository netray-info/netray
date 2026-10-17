# Comparison Page Rules

Applies to: `crates/web/compare.toml` and `templates/pages/compare.html`  
Read this when: adding or removing a tool from the suite, adding new checks to an existing tool, or doing the quarterly review pass.

The page is rendered by `netray-web` at `/compare`. `compare.toml` holds its data, `compare.html` its markup, and the netray column is generated. The sitemap entry is a line in `pages::sitemap()`.

---

## When the page must be updated

Flag `crates/web/compare.toml` for update whenever any of the following happens:

| Event | What to update |
|-------|---------------|
| New tool added to the suite (e.g. `regis.netray.info`) | Add a new axis section to Table 2; if it is also a unified checker, add a column to Table 1 |
| netray gains a new check (e.g. email adds DANE, TLS adds OCSP stapling) | Add the check id to the `checks` list of the corresponding row(s) in Table 2 and set `depth`; the generated column and its note follow |
| netray drops or renames a check | Reverse of the above; a listed id that is not in the check catalogue fails a test naming the row and the id |
| A competitor gains or loses a feature (verified against their docs) | Update the affected cell and its note in `compare.toml` |
| Quarterly review pass | Re-verify all cells, bump "Last reviewed" in `compare.toml` |

**Do not silently update cells.** If a cell changes, explain why in the commit message so the diff is auditable. A listed check id missing from the catalogue fails a test rather than flipping the cell: removing a check forces the compare edit.

---

## How the comparison was compiled

### Methodology

1. **Four-symbol system** — every cell holds exactly one of:
   - `✓` full coverage — the tool does this well by default
   - `~` partial or lower depth — the tool touches this but with meaningful gaps
   - `✗` not done — the tool does not cover this check at all
   - `—` out of scope — the check is conceptually outside what this tool does (e.g. asking badssl.com to scan your server)

2. **Source of truth** — claims were verified against vendor documentation and hands-on checks using `example.com` as the test domain. Where documentation was ambiguous, the tool was queried directly and the result observed.

3. **Admitted gaps** — the "Where netray trades depth for breadth" section names concrete limitations of netray vs. SSL Labs, hardenize, and mxtoolbox. This is intentional product positioning: honesty builds more trust than parity claims.

4. **No Registration row** — the `regis.netray.info` axis is deferred until that tool ships to production (see `specs/sdd/domain-registration-inspector.md`). A "coming soon" row ages badly in screenshots.

### The generated netray column

The netray column is not written by hand. Each row of `compare.toml` gives netray's basis as one of:

- `checks = [<check ids>]` with `depth = "full" | "partial"` — shown as `✓` for `full` and `~` for `partial`, and `✗` when the list is empty. The note is generated: the catalogue `title` of each listed check, in list order. The ids are those of the check catalogue (`crates/engine/catalogue/checks.toml`).
- `product = "yes" | "partial" | "no"` with a `note` — for rows no check covers (API access, login). The row shows its symbol and the written note.

The competitor cells and their notes are written by hand in `compare.toml`.

### Table structure

- **Table 1 — Unified domain checkers**: peer tools that also give a single broad domain health picture. Columns: `Feature | netray | hardenize | internet.nl | observatory.mozilla.org`.
- **Table 2 — Per-axis specialists**: low-level or single-axis tools grouped by the axis they cover (DNS, TLS, HTTP, Email, IP). Each axis has its own column set because the tools are not comparable across axes.

The split into two tables matters: `dig` is not a competitor to netray; it is a building block. Conflating the two tables muddies the headline story.

### Adding a new tool to the suite

When a new netray.info tool ships:

1. Add its axis section to Table 2 with `✗` in all cells initially (`checks = []`), then fill based on actual capability.
2. Decide whether it also belongs in Table 1 (only if it offers a *unified* multi-check view; single-axis tools belong only in Table 2).
3. If it belongs in Table 1, add the column in `compare.toml` and fill every row.
4. Add the new tool's sub-domain link to the suite links in `templates/base.html`.
5. Add the tool's inspector root to the fixed page list in `sitemap()` (`crates/web/src/pages/mod.rs`).
6. Bump "Last reviewed" to today's date.
7. Commit with `feat(web): add <toolname> axis to comparison page`.

### Quarterly review pass

1. Open each competitor's documentation and re-verify cells that are likely to have changed (new features, deprecations).
2. Run a quick hands-on check of `example.com` through each unified tool.
3. Update any changed cells and expand/update their notes in `compare.toml`.
4. Update the "Where netray trades depth for breadth" section if any gap has closed.
5. Bump "Last reviewed: YYYY-MM-DD" in `compare.toml`.
6. Commit with `docs(web): comparison page quarterly review YYYY-MM-DD`.

---

## Page implementation notes

- **Template, not a hand-edited file** — `templates/pages/compare.html` extends `templates/base.html` and renders `compare.toml`. Styles live in the shared stylesheet under the gate; do not inline a `<style>` or add one for this page.
- **No JS** — row expansion uses native `<details>`/`<summary>`. Do not add JavaScript or an island.
- **No codenames** — `compare.toml` and the template name the product netray and the inspectors by role; `tests/repo/test_no_codenames.sh` enforces it.
- **Sitemap** — `/sitemap.xml` is generated by `pages::sitemap()` (`crates/web/src/pages/mod.rs`) from the guide list plus its fixed page list, `/compare` included. Nothing runs after an edit; a new page adds its route and its entry in `sitemap()`.
- **"Last reviewed" date** — always update it. It is the only date visible to readers; stale dates erode trust faster than stale data.
