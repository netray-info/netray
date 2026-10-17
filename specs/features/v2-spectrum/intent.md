# Intent: v2-spectrum

## Problem

SDD V4.4 asks for one Rust function, `spectrum::layout`, that yields the spectrum's geometry for
the result page, `/r/` and the OG card, label placement included (S7, S20, §5.4, §5.5). Nothing
computes that geometry today. The 4a `spectrum` component (`crates/web/templates/components/spectrum.html`)
draws lines from x and height percentages its caller supplies, puts every label in one row at its
line's x (`spectrum.html:3-7`, `static/spectrum.css:66-71`), has no leaders, does not scroll, and
sets labels in `--font-size-caption`, not the 12 px `font.size.label` that §5.5 measures. The
OG card in lens is still the V1 grade card (`crates/lens/src/og/render.rs:98-143`) and draws no
spectrum. Without the layout, the result-page and `/r/` features each need their own geometry,
which is the second implementation S7 rules out.

## Proposed outcome

`netray_web::spectrum::layout` is a pure function. It takes the current run, the older runs (as
per-check statuses plus each run's profile), the catalogue and a container width. It returns one
line per catalogue check: shared x geometry, and per run the tone, height, label placement and row
count. A property test over random runs and widths holds S20's guarantees: no two labels in a row
overlap, every warn, fail and fixed line has exactly one label, and the plate is exactly the
container while the pitch is at least 8 px and n × 8 px below that. The 4a `spectrum` component
renders a run of that layout with label rows, leaders and sideways scroll, and the design-system
gate passes.

## Affected users and systems

- Nobody sees it yet. No route serves the component until `v2-result-page` (V4.5) and `v2-entries` (V2.7).
- `crates/web`: a new module `spectrum`; new dependencies on `netray-model` and `netray-engine`, which SDD §3.1 allows; `proptest` as a dev-dependency; the spectrum template, stylesheet and two new tokens.
- `specs/sdd/v2.md` §3.9, §5.4, §5.5, §8.1 and the Phase 4 table.
- lens, the `netray` binary and every wire stay unchanged.

## Constraints

- The overnight build runs in parallel with the history and API lane (`v2-history`, `v2-admission`, `v2-entries`, API v1) toward a local 0.30.0 (operator, 2026-10-17). This feature must not depend on that lane: no `Run` type, no `netray-store`, no history read.
- `tests/repo/test_web_crate.sh:51` forbids the `netray` binary from depending on `netray-web`, and `:44` forbids axum in `netray-web`.
- The gate (`crates/web/src/gate.rs`, 4a requirement 6) must pass. Every width, font and colour goes through tokens.
- `just adlc-verify` stays offline.

## Open questions

- Which width does the page lay the labels out at, given that the server does not know the viewport? (decided by the operator, 2026-10-17: once, at the narrowest plate, 432 px for 54 checks.)
- Should line heights be scaled by the run's measured weights, as SDD §3.9:454 says, or by the static profile weights? (decided in the spec: by the measured weights, as the SDD says, with a fallback.)
- Can the OG card switch now? (decided in the spec: no. It needs the predecessor from the history, and a lens dependency on `netray-web` that `test_web_crate.sh` forbids. It switches in `v2-result-page`, operator decision X4.)
