---
class: verdict-broader-than-the-check
repeat-of: specs/solutions/2026-10-09-info-ranked-above-pass.md
---
# A strict address lookup turned a working A answer into a failure

**What failed.** To keep a failed AAAA half from reading as a measured absence, `DnsResolver::lookup_ips` (`crates/email/src/dns/resolver.rs`) was made to return Err whenever either the A or the AAAA half errored. Its callers then lost the A answer that had worked:
- `FetchResolver` returned `[]`, so the MTA-STS policy fetch reported `email.mta_sts` fail and the BIMI logo fetch warned.
- FCrDNS reported an IPv4 MX IP that A confirmed as unmeasured.

The branch review caught it (`specs/features/v2-unmeasured/review.md`, range 4e37fe0..3cd101b). Each later fix narrowed it further: partial MX hosts went unnamed, SPF read the failed family from detail text, and `exists:` failures were misread as A-family errors.

**What worked.** `lookup_ips` returns `AddressLookup { ips, error }`: Ok when one family answered, Err only when both failed. Each caller decides under Rule M: fetches use the addresses; FCrDNS is unmeasured only when the IP is unconfirmed and its own family failed. SPF records unresolved families in a structured `spf_incomplete` marker that the coverage check alone reads (772970f, dd082a8, c5fdb6a, 3d6f9a5, 879848e).

**How to notice next time.** An error verdict covers more than the part that failed: a test passes the strict case, but the same input with one half answered goes red.
