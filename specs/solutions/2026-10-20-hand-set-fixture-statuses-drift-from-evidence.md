---
class: neighbour-artefact-left-inconsistent
repeat-of: specs/solutions/2026-10-14-golden-scenarios-no-run-can-produce.md
---
# Hand-set history fixtures held statuses their own evidence contradicts

**What failed.** The seven runs under `tests/acceptance/fixtures/history/` were captured once and
edited by hand for the design boards: header checks passed without their headers, DMARC warned
"p=none" beside `p=reject`, the three shop runs shared identical blocks with different statuses, and
module changes since (COOP, DANE/FCrDNS/DNSBL blocks, `inspection_ms`) never reached them.
Recomputing them through the modules (v2-fixture-consistency report, "Contradictions found and
fixed") changed 15 to 20 statuses per fixture and every grade to F before the evidence was fixed.

**What worked.** Each module's `testing::from_evidence(domain, blocks)` rebuilds its input from its
own blocks and runs its real analysis; the lens guard
`crates/lens/tests/it/history_fixtures_consistent.rs` compares every fixture with lens's run over
those outcomes, and `UPDATE_FIXTURES=1` rewrites them. Story statuses were kept by editing the
evidence (headers, chain roots, an older certificate, a weak DKIM key), never the status.

**How to notice next time.** A fixture whose status reads differently from the block beside it
on the result page, or a module change whose new block no fixture carries.
