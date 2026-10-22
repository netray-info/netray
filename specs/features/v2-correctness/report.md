# Report: v2-correctness

## Phase 1 — Email: truncated answers and the SPF pass

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: the email resolver retries a truncated UDP answer once over TCP to the same server address and port (address, address with port, predefined provider); a failed retry names the TCP failure | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C2 | R2: an SPF record with no Warn or Fail sub-check is a Pass with spf_ok whatever Info sub-checks it carries; email.spf is pass | green | crates/email/tests/spf_info_only.rs |
| C3 | GIVEN a responder answering TXT over UDP with TC and over TCP with a 3000-byte SPF record WHEN DnsResolver built with its address and port looks up TXT THEN it returns the SPF record | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C4 | GIVEN that responder and a provider-like UDP config to it WHEN the TXT lookup runs THEN the TCP answer is returned and the responder saw one UDP and one TCP query | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C5 | GIVEN a responder that sets TC over UDP and refuses TCP WHEN the TXT lookup runs THEN the error cause names the TCP failure | green | crates/email/tests/it/resolver_tcp_retry.rs |
| C6 | GIVEN a responder answering over UDP without TC WHEN the TXT lookup runs THEN no TCP query is made | already_implemented | crates/email/tests/it/resolver_tcp_retry.rs |
| C7 | GIVEN v=spf1 a:mail.example.com a:www.example.com -all with both names at 192.0.2.10 WHEN the SPF check runs THEN Pass with spf_ok and overlapping_cidrs (Info), email.spf pass | green | crates/email/tests/spf_info_only.rs |
| C8 | GIVEN overlapping prefixes and a ptr: mechanism WHEN the SPF check runs THEN Warn without spf_ok | already_implemented | crates/email/tests/spf_info_only.rs |
| C9 | GIVEN an SPF record with no Info, Warn or Fail sub-check WHEN the SPF check runs THEN Pass with spf_ok | already_implemented | crates/email/tests/spf_info_only.rs |

C6, C8 and C9 held before the change; their tests are kept as protection.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 3 | sonnet | 65006 | 123 |
| G2 | 1 | sonnet | 30422 | 27 |

### Review

- BLOCKER (fixed): `retries(0)` on the UDP group dropped hickory's one re-send for every failure, not only truncation. `lookup_on` now re-sends any non-truncated failure once on the same resolver.
- BLOCKER (fixed): the TCP twin kept `retries: 1`, so a failing retry sent two TCP queries; the twin now runs with `retries: 0`.
- NIT: no test covers the re-send of a non-truncated failure.

### Behavioural verification

skipped: a binary built in the worktree has no egress, and the live email path is the domain run; the loopback responder tests drive the real `DnsResolver` over UDP and TCP. The live recheck of amazon.com and github.com SPF runs after the last DNS phase.
