# Report: raw query policy

## Phase 1 — Outbound context

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1 One policy for raw queries: one context in `dns_raw` (allow, glue resolver, sender); `build_server_list` and glue resolution apply `allow`; no refused address reaches the sender; refused glue is not resolved again; every raw query, root queries included, goes through the sender | green | `dns_raw_outbound_tests.rs` |
| C2 | R2 Every domain-derived path uses it: `ns_lame`, `ns_delegation`, `authcompare`, the DNSSEC walk and `dns_trace` send through the context | green | `all five *_outbound_tests.rs` |
| C3 | R3 NS checks report refused addresses: one `Warning("nameserver address not public, not queried")` per category, remaining addresses judged as today; all refused replaces the "could not resolve" / "no servers responded" lines | green | `api/check_ns_outbound_tests.rs` |
| C4 | R4 DNSSEC walk ends the branch: empty IPv4 list because of refusal emits a level with the warning finding and stops; refused among public dropped | green | `dns_dnssec_outbound_tests.rs` |
| C5 | R5 authcompare's warnings carry "nameserver address not public, not queried" once when an auth address is refused | green | `api/authcompare_outbound_tests.rs` |
| C6 | R6 Public nameservers unchanged: same addresses, same findings | green | `all five *_outbound_tests.rs` |
| C7 | GIVEN an NS name mapped to `10.0.0.1` WHEN `ns_lame` runs THEN no address is recorded and the category holds the warning | green | `api/check_ns_outbound_tests.rs` |
| C8 | GIVEN NS names mapped to `10.0.0.1` and `192.0.2.53` (AA=1) WHEN `ns_lame` runs THEN only `192.0.2.53` is recorded, the category holds the warning plus today's line | green | `api/check_ns_outbound_tests.rs` |
| C9 | GIVEN the same two NS names WHEN `ns_delegation` runs THEN only `192.0.2.53` is recorded and the warning appears once | green | `api/check_ns_outbound_tests.rs` |
| C10 | GIVEN NS names mapped to `10.0.0.1` and `10.0.0.2` WHEN `ns_lame` runs THEN the warning appears exactly once | green | `api/check_ns_outbound_tests.rs` |
| C11 | GIVEN the only NS name mapped to `10.0.0.1` WHEN `ns_delegation` runs THEN the category holds only the warning | green | `api/check_ns_outbound_tests.rs` |
| C12 | GIVEN glue resolving to `172.16.0.5` WHEN `authcompare` resolves auth servers THEN it is not recorded and the warnings carry the refusal once | green | `api/authcompare_outbound_tests.rs` |
| C13 | GIVEN auth glue `192.0.2.53` WHEN `authcompare` runs THEN `192.0.2.53` is recorded | green | `api/authcompare_outbound_tests.rs` |
| C14 | GIVEN a DNSSEC referral whose only IPv4 glue is `10.0.0.1` (AAAA beside it) WHEN the walk reaches that level THEN nothing is recorded for it and the level carries the warning | green | `dns_dnssec_outbound_tests.rs` |
| C15 | GIVEN a DNSSEC referral NS with glue `10.0.0.1` whose name the stub maps to `192.0.2.53` WHEN the walk reaches that level THEN nothing is recorded for it | green | `dns_dnssec_outbound_tests.rs` |
| C16 | GIVEN a DNSSEC referral with glue `10.0.0.1` and `192.0.2.53` WHEN the walk reaches that level THEN only `192.0.2.53` is recorded | green | `dns_dnssec_outbound_tests.rs` |
| C17 | GIVEN a trace referral with glue `127.0.0.1` WHEN the trace walk follows it THEN it is not recorded; glue `192.0.2.53` is recorded | green | `dns_trace_outbound_tests.rs` |
| C18 | GIVEN only public-standing NS addresses WHEN `ns_lame` and `ns_delegation` run THEN recorded addresses and findings equal today's, as sets | green | `api/check_ns_outbound_tests.rs` |
| C19 | GIVEN the production context WHEN built THEN its `allow` is `is_allowed_target` | green | `dns_raw_outbound_tests.rs` |
| C20 | GIVEN any walk in a test WHEN it queries the root servers THEN the recording sender sees them | green | `dns_dnssec_outbound_tests.rs, dns_trace_outbound_tests.rs` |

### Red

The tests were committed in `a02cd74` under `ADLC-Baseline: 5a614c0`. Red was a compile failure for the missing context (`unresolved import crate::dns_raw::NOT_PUBLIC`, `RawOutbound`, `RawSend`, `SendFuture`, `resolve_auth_servers`, `query_auth_servers`, `walk_with`, `walk_chain_with`): every test is a module of package `prism`, so the API had to be fixed before the tests (a contract handed to every writer; the shared recording sender, stub resolver and test `allow` live in `dns_raw_outbound_tests.rs`). Writers ran one per test file, not one per criterion, so that no two wrote the same file. No criterion was already implemented.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C1–C20) | 2 | sonnet | 83604 | 273 |

`cargo test -p prism --lib outbound`: 30 passed. After green, two warnings in the test build (an unused import in `api/check_ns_outbound_tests.rs`, the unused helper `RecordingSender::recorded_ips_for`) were removed in the test files (`ADLC-Test-Change` trailers on the production commit).

### Reader

BLOCKER 0, AMENDMENT 1, DEFERRED 0, NIT 2.

- AMENDMENT | `dns_dnssec.rs` | The level the walk adds when every address is refused carried `is_final: false` even when its zone is the target; the spec is silent on the flag. Repaired in phase: the flag is set by position as on every other level. | affected_phase: 1 | repaired_in_phase: yes
- NIT | `api/authcompare.rs` | No test proves the refusal warning reaches the done event's `warnings` (`all_warnings.extend(auth.warnings)`); the tests stop at `resolve_auth_servers`. Correct today.
- NIT | `dns_raw.rs` | A failed glue lookup no longer logs its cause at warn level: `SystemResolver` turns the error into an empty list, so only the debug line remains. Findings unchanged.

### Behavioural verification

`./target/debug/netray dns crates/mhost-prism/prism.dev.toml`, then

```
curl -s -X POST 'localhost:8081/api/dnssec?stream=false' -d '{"domain":"example.com"}'  -> chain level 1 "." servers_queried 13, finding "No DNSKEY records found at .", then done
curl -s -X POST 'localhost:8081/api/trace?stream=false' -d '{"domain":"example.com"}'   -> hop 1 "." servers_queried 13, every root "timeout after 3s", then done
```

The routes run through the production context (13 root queries sent through the UDP/TCP sender, each with its 3 s timeout), but this machine's sandbox does not let a process reach port 53: a plain Python UDP query to `198.41.0.4:53` times out the same way. Delegation past the root, and so the public-nameserver results end to end, could not be observed here; the offline tests carry R6.

`adlc verify`: contract passed.
