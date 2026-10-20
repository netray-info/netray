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
