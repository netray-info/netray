---
class: check-passes-without-observing
repeat-of: specs/solutions/2026-10-15-render-tests-saw-classes-not-drawing.md
---
# Router and component tests passed while the evidence views scrolled sideways at 390 px

**What failed.** Phases 1–7 of v2-inspectors rendered every evidence block through tested templates and the CSS gate, and no test laid a page out. The first browser run (Phase 8, `just acceptance-local tests/acceptance/v2/inspectors.spec.ts`) found `/example.com/dns` at scrollWidth 942 and `/example.com/tls` at 941 for a 390 px viewport: DNSKEY base64, the HTTPS record and the TLS addresses table never wrap.
**What worked.** A browser assertion per view (`document.documentElement.scrollWidth <= clientWidth` at 390 px) over the seeded entry, and `crates/web/static/evidence_block.css` letting a too-wide table scroll inside its block (`overflow-x: auto`) while fields and chain values wrap. A first fix with `overflow-wrap: anywhere` on table cells passed the same assertion and broke short tokens at 1280 px, which only the screenshots showed.
**How to notice next time.** A layout requirement (S16, a viewport width) is marked green by a test that never launches a browser.
