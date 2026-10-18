# Intent: v2-pages

## Problem

V2's static pages exist only as hand-written HTML under `site/`, served by `netray site` (`crates/netray/src/site.rs`) with a CSP that allows `'unsafe-inline'` scripts (`site.rs:19`), inline theme and status scripts (`site/404.html:318`, `site/status/index.html:295`), codenames in their copy (lens, beacon, tlsight, spectra, prism, ifconfig-rs: about 160 whole-word occurrences across `site/guide`, `site/compare`, `site/status`, `site/404.html`), and a comparison page whose netray column is typed by hand. `netray-web` (Phase 4a) has tokens, components and a gate, but no page frame, no route, no islands toolchain, and four items its report deferred "to the pages phase": `[data-theme="light"]` under a dark OS, inlined SVG styles restyling the host, the `query_input` form target, and an unquoted askama attribute that hides a following event handler from gate rule 3. `specs/rules/frontend-rules.md` still describes the SolidJS apps.

## Proposed outcome

SDD criteria V4.8–V4.12 as far as the static pages go: guides, compare (its netray column generated from the catalogue), status and the error pages are askama templates on one `base.html` in `netray-web`, served by a `pages` router under a CSP without `unsafe-eval` or executable inline scripts except the hash-allowed speculation rules (`application/ld+json` data blocks are not executable); prerender covers only guides and `/r/`; view transitions run behind reduced motion; an islands bundle (with the shared `stream` island for v2-result-page and v2-inspectors, and `theme` and `status`) builds with esbuild, stays under 30 KB gzipped and runs its vitest suite in the gate; `frontend-rules.md` is rewritten for V2; a repo check keeps codenames out of templates, static files, islands and rendered pages. The four 4a deferrals are closed.

## Affected users and systems

No visitor sees a change before the cutover: nothing ships before the single 0.30.0 release, lens mounts the `pages` router ahead of its SPA fallback (X1) only so local runs reach it, and `netray site` keeps serving `site/` unchanged until v2-cutover deletes both (V5.1). The gate gains the islands' type check, vitest suite and bundle budget, the codename check and new `netray-web` tests. `netray-web` gains axum and `netray-engine`; routes are allowed under `src/entries/`, `src/pages/` and `src/api/` (X1). `/privacy` stays with v2-entries, and `base.html` is shared with it (separate `entries` and `pages` modules).

## Constraints

Part of the autonomous overnight build to a local 0.30.0 (operator, 2026-10-17); runs beside the history/API lane and depends on nothing it builds. No release of its own. SDD `specs/sdd/v2.md` S7, S8, S25, §5.1, §8.1, §8.4–§8.7; `specs/rules/comparison-page-rules.md` for compare. Guide content moves as is, apart from names and frame.

## Open questions

None open. Three were decided by the operator on 2026-10-17 (spec Decision log): the guide URL segment (`guide` in the catalogue and production, `guides` in SDD §5.1); whether the theme switch and the live status page get islands that §8.4 does not list; whether the guides keep their JSON-LD blocks, which gate rule 3 refuses today.
