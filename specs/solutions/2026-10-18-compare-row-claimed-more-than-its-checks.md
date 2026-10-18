---
class: verdict-broader-than-the-check
repeat-of: specs/solutions/2026-10-13-strict-address-lookup-dropped-the-answered-half.md
---
# A generated compare cell claimed full coverage its checks do not give

**What failed.** `crates/web/compare.toml` rows took V1's netray symbol as their `depth`: SOA record ✓ on `dns.ttl` (judges only the SOA minimum TTL), Protocol versions ✓ on `tls.tls_version` (the TLS inspector negotiates 1.2/1.3 only, so it never sees a deprecated version), NS records ✓ on checks that read the child's NS set and never the parent's. The generated column is honest only when each row claims no more than its checks judge; three reader passes found one more such row each.
**What worked.** Reading each listed check's code before choosing `depth`: SOA and NS records `partial`, Protocol versions `checks = []`, Table 1 DNS checks `partial` because it lists a partial check; `crates/web/tests/pages_compare.rs` `generated_cells_claim_no_more_than_the_checks_judge` pins the five rows.
**How to notice next time.** A row's `depth = "full"` next to a competitor note that names more than the listed checks' catalogue texts.
