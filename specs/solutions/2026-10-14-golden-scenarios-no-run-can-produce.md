---
class: neighbour-artefact-left-inconsistent
repeat-of: specs/solutions/2026-10-11-narrowing-a-check-dropped-its-hard-fail.md
---
# Hand-written golden frames described runs no check could produce

**What failed.** The beacon goldens are literal scenarios in `crates/email/tests/contract_golden.rs`. Once v2-evidence derived blocks from what the checks measured, two scenarios turned out to be self-contradictory:
- `scenario_healthy` said "FCrDNS checked for 4 IP(s)" but held one `fcrdns_pass`, although `check_fcrdns` emits one sub-check per IP. Its SPF covered only `ip4`, while `mx_ok` needs IPv6 MX addresses, so a real run would have emitted `spf_mx_coverage`.
- `mx_cname` had the same two inconsistencies.

No consistent `Measured` could be written for them. The coder first marked all four FCrDNS addresses confirmed, which would have published evidence that contradicts the frame.

**What worked.** The scenarios were corrected so a real run could produce them, with the same verdicts (ADLC-Test-Change): four `fcrdns_pass` for four IPs, and SPF `ip6:2001:db8::/32` or a second `ip4` covering every MX address. The goldens were regenerated, and the lens goldens did not move (33392da).

**How to notice next time.** A golden scenario that cannot be reproduced through `run_all_checks` with a `TestDnsResolver` is a hand-written claim, not a recording; check its counts against the code that emits them.
