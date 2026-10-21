# Plan: v2-tls-ui

## Phase 1 — Seam, addresses and certificate

### Groups

G1: C18 · G2: C1–C17, C19–C25 (G1 first; the `it` binary compiles only once G2 lands).

### Plan

G1
1. `crates/tls/src/quality/checks.rs`: `pub const EXPIRY_WARN_DAYS: i64 = 30;`, read by `check_expiry_window` in place of the literal.
2. `crates/tls/src/validate/chain_trust.rs`: `pub const TRUST_STORE: &str` naming the webpki-roots set ("the Mozilla root program").

G2
1. `crates/web/src/evidence.rs`: `unix_time` split out of `display_time`; `slot` takes the sibling blocks; `evidence_view` and `panel` pass the protocol's blocks.
2. `crates/web/src/pages/views/mod.rs`: `render_with(block, ctx, siblings)`; `render` = `render_with(.., &[])`; `claims` takes siblings; `pub mod validity`.
3. `crates/web/src/pages/views/validity.rs`: the pure validity layout.
4. `crates/web/src/spectrum.rs`: `mini_band` over the catalogue layout, `None` without statuses.
5. `facts_grid` component: per-cell trailing markup (`facts_grid_with`), output unchanged without it.
6. `crates/web/src/pages/views/tls.rs`: one pure `taken` function; the port fragment at `tls.addresses`; `Some("")` for taken chain, handshake and ct blocks; distinct values; addresses, certificate sections, facts, OCSP chip, chain pills, crt.sh, trust text; a "Handshake" facts grid for the handshake facts Phase 2's grid will hold.
7. `templates/partials/tls_addresses.html`, `tls_certificate.html`.
8. `crates/web/src/pages/inspect.rs`: `part_html` passes the part's blocks; re-stream check with siblings.
9. `crates/web/static/pages.css`: `tls-*` rules, tokens only.
10. `specs/sdd/v2.md` §6.1 and the facts_grid row amended.

Decisions: the trust text on warn or fail is the catalogue `finding` of `tls.chain_trusted` in the check's tone (the view holds statuses, not findings; requirement 9 amended); the handshake facts render in a "Handshake" grid until Phase 2.

## Phase 2 — Protocols, notices and related records

### Groups

G1: C1–C12 (one production file carries every criterion: `views/tls.rs`; the related box adds `inspect.rs` and its template).

### Plan

1. `crates/web/src/pages/views/tls.rs`: `taken` also takes `tls.protocols[@port]`, every `tls.probes@<addr>:<port>` of an ok row and `tls.notices`; the port fragment gains the notices banner (first), the "Protocols and features" section (after the certificate sections; `frame` with the protocols block when present) with the cell grid and the "Probe answers" details; the Phase 1 "Handshake" grid goes (its facts are cells); `pub fn tls_related_summary(blocks)` re-exported from `views/mod.rs`.
2. `templates/partials/tls_protocols.html`, `tls_notices.html`.
3. `crates/web/src/pages/inspect.rs` and `partials/inspect_start.html`: the related box gets `id="inspect-related"`; the stream sends an `inspect-related` fragment when the band port's TLS part arrives; `shared_html` renders it filled.
4. `crates/web/static/pages.css`: `tls-protocols*` (board G: grid `repeat(auto-fill, minmax(250px, 1fr))`, 1 px line-coloured gaps, cells padding 12 16 on the surface, bar 3/5/6 px wide × 22 px by tone, name 14 px 600, state mono 12 px muted), `tls-notices` (warn tone), `tls-probes`.

## Phase 3 — Per-port parts and tabs

### Groups

G1: C1–C8 (one seam: `pages/inspect.rs` and its templates; `api/jobs.rs` attaches the checks).

### Plan

1. `crates/web/src/pages/inspect.rs`: `Job::with_part_checks(part, checks)`; `stream_job` awaits a part's checks with it, renders the part with them (and, with several checked parts, its own band region `inspect-band-<n>` / `inspect-checks-<n>` inside the panel), sends the tab meta fragment `inspect-port-meta-<n>`, and leaves such parts out of the band-end re-stream; the start fragment with several checked parts renders the tab row and the panel placeholders and no `inspect-band`; `SharedRun` stores per-part checks (`shared.rs`) and `shared_html` renders panels, bands and metas from them.
2. `templates/partials/inspect_start.html`, `inspect_part.html`, `inspect_band.html` (id suffix), a `inspect_port_meta.html` partial.
3. `crates/web/src/api/jobs.rs` `port_jobs`: each part's checks from its port's outcome.
4. `crates/web/static/inspector_shell.css` or `pages.css`: `.inspector-ports*` tab row (marker rule via `aria-current`), `.inspector-ports[data-ready] .inspector-port:not([data-shown]) { display: none }`.
5. `specs/sdd/v2.md` §6.1 (inspector re-stream) and the TLS inspector paragraph: per-part checks, panels.

## Phase 4 — Port chooser

### Groups

G1: C1–C9 (the server markup and the island; one group, two files of tests).

### Plan

1. `crates/web/src/pages/inspect.rs`: `InspectorJobs::max_ports` (default 7); `show` renders the chooser for TLS only; `Tool::syntax` for TLS names `host:443,465`. `crates/web/src/api/jobs.rs` (`InspectJobs`) returns the TLS inspector's `limits.max_ports`.
2. `templates/components/query_input.html`: a `between` slot (HTML) between the box and the submit; `components/inspector_shell.html` passes it; a `partials/port_chooser.html`.
3. `crates/web/islands/src/ports.ts` (`parsePorts`, `formatHost`, `ports`), registered in `main.ts`.
4. `crates/web/static/query_input.css` or `inspector_shell.css`: `.port-chooser*` per board G (label 12 px muted; toggles 44 px, mono 14 px, padding 0 14 0 18, 1 px line border, pressed: ink border, weight 700 and the marker; "+" 44 × 44 dashed; gap 6 px).

## Phase 5 — Copy and certificate changed

### Groups

G1: C1–C9 (the certificate section's markup in `views/tls.rs` and the `certdiff` island).

### Plan

1. `crates/web/src/pages/views/tls.rs` and `partials/tls_certificate.html`: copy buttons (the shell's `.inspector-copy` primitive of v2-ip-ui) as trailing markup of six facts cells; the `tls-certdiff` element with the leaf's facts and key and its banner template.
2. `crates/web/islands/src/certdiff.ts` (`describeChange`, `certdiff`), registered in `main.ts`.
3. `crates/web/static/pages.css`: `.tls-certdiff*` banner (warn tone, as the notices banner).
