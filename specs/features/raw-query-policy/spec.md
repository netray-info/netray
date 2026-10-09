# Spec: raw query policy

Status: Draft
Created: 2026-10-09

## Goal

Every raw DNS query prism sends to an address it took from the checked domain's data — NS addresses, referral glue, glue it resolves itself — goes through one outbound policy in `dns_raw`, the same `netray_common::target_policy` the fetch helper uses. A refused address is never queried. The NS checks report such an address as their own finding; the DNSSEC chain walk ends that branch as unverifiable. Public nameservers keep today's results.

## Non-goals

- Applying the policy to the root servers (a fixed public list, `dns_raw.rs:22`) or to resolvers the caller names (checked by `check_target_ip`, `security/query_policy.rs:108`, and `deny_non_global`, `api/query.rs:771`). Root queries still go through the injected sender, so tests stay offline.
- Any change to lint text for public nameservers.
- IPv6 nameserver queries (the paths are IPv4-only today and stay so).

## Context and constraints

Line numbers at `e408d7e`; paths under `crates/mhost-prism/src/`.

- The raw primitives `parallel_queries` (`dns_raw.rs:156`), `raw_query` (`:176`), `raw_query_dnssec` (`:194`), `send_udp` (`:229`), `send_tcp` (`:269`) check no address. `build_server_list(ns, ip_allowed)` (`:330`) is the only filter seam; `resolve_missing_glue` (`:346`) uses the system resolver through `tokio::net::lookup_host` (`:361`), keeps IPv4 (`:366`) and is not injectable.
- Callers without a policy: `check_ns_lame_delegation` (`api/check.rs:909`, queries `:942`, IPv4-only filter `:932`), `check_ns_delegation_consistency` (`:998`, `:1025`, `:1041`), `authcompare` (`api/authcompare.rs:237-244`, `:375`), the DNSSEC walk (`dns_dnssec.rs:207` filters IPv4 only; queries `:176`, `:240-257`). `dns_trace.rs:178-179` already filters with `check_target_ip`.
- Today's NS findings live in lint categories `ns_lame` and `ns_delegation` (`api/check.rs:480-481`) with the messages listed in `:909-1097` ("no servers responded to SOA query" `:970` is `ns_lame`'s; `ns_delegation`'s is "no NS records returned from direct authoritative query" `:1066`); `authcompare` drops the auth side silently when no auth address remains (`api/authcompare.rs:256`, `:532`); `ns_lame`'s lines follow HashMap and completion order (`api/check.rs:931`, `dns_raw.rs:170`); the DNSSEC walk has no status enum, a level carries `ChainFinding { severity: ok|warning|failed, message }` (`dns_dnssec.rs:28-46`), and an empty server list ends the walk with a log line only (`:84-87`).
- Prior art: `Outbound { settings, resolver, allow: fn(IpAddr) -> bool }` (`api/check.rs:1400-1416`) with tests injecting `allow` and a stub resolver (`api/check_mta_sts_tests.rs`).
- There are no tests for these query paths today. The port is fixed at 53, so a listener cannot observe a query; tests need a seam that records the addresses.
- Neutral wording in code, commits and this spec (security-correctness SC2): "outbound policy", no internal names.

## Requirements

1. **One policy for raw queries.** `dns_raw` holds one outbound context for queries to domain-derived addresses: an `allow: fn(IpAddr) -> bool` (production: `netray_common::target_policy::is_allowed_target`), an injectable glue resolver (production: the system resolver) and an injectable sender (production: the UDP/TCP primitives). `build_server_list` and the glue resolution apply `allow`; no refused address reaches the sender. A nameserver whose glue is all refused is not resolved again (refused glue counts as present). Every raw query, root queries included, goes through the sender.
2. **Every domain-derived path uses it.** The NS lame-delegation and delegation-consistency checks, `authcompare` and the DNSSEC chain walk (referral glue and resolved glue) send through the context; `dns_trace` moves onto it too, so one place decides.
3. **NS checks report refused addresses.** When a nameserver address is refused, `ns_lame` and `ns_delegation` each emit `Warning("nameserver address not public, not queried")` once per category, and judge the remaining addresses as today. When every address is refused, that warning replaces the "could not resolve" / "no servers responded" lines.
4. **DNSSEC walk ends the branch.** When the next level's IPv4 server list (the addresses the walk would query) is empty because its addresses were refused, the walk emits a level for that zone with finding `warning "nameserver address not public, not queried"` and stops; refused addresses among public ones are dropped.
5. **authcompare reports refused addresses.** When an auth address is refused, `authcompare`'s warnings carry "nameserver address not public, not queried" once.
6. **Public nameservers unchanged.** With only public addresses, every path sends to the same addresses and emits the same findings as today.

## Phase 1 — Outbound context

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5, 6

### Test Scenarios

Tests use a recording sender (it records the addresses and answers canned responses) and a stub glue resolver; a test `allow` admits documentation addresses (`192.0.2.0/24`) as stand-ins for public ones.

- GIVEN an NS name that the glue resolver maps to `10.0.0.1` WHEN `ns_lame` runs THEN no address is recorded and the category holds `Warning("nameserver address not public, not queried")`.
- GIVEN two NS names, one mapped to `10.0.0.1` and one to `192.0.2.53` (answering AA=1) WHEN `ns_lame` runs THEN only `192.0.2.53` is recorded, and the category holds the warning plus today's line for the answering server.
- GIVEN the same two NS names WHEN `ns_delegation` runs THEN only `192.0.2.53` is recorded and the warning appears once.
- GIVEN NS names mapped to `10.0.0.1` and `10.0.0.2` WHEN `ns_lame` runs THEN the warning appears exactly once.
- GIVEN the only NS name mapped to `10.0.0.1` WHEN `ns_delegation` runs THEN the category holds only the warning (no "could not resolve" line).
- GIVEN glue resolving to `172.16.0.5` WHEN `authcompare` resolves auth servers THEN that address is not recorded and the warnings carry the refusal once.
- GIVEN auth glue `192.0.2.53` WHEN `authcompare` runs THEN `192.0.2.53` is recorded, as today.
- GIVEN a DNSSEC referral whose only IPv4 glue is `10.0.0.1` (an AAAA glue beside it) WHEN the chain walk reaches that level THEN no address is recorded for it and the level carries the warning finding.
- GIVEN a DNSSEC referral NS with glue `10.0.0.1` whose name the stub maps to `192.0.2.53` WHEN the walk reaches that level THEN nothing is recorded for it (refused glue is not resolved again).
- GIVEN a DNSSEC referral with glue `10.0.0.1` and `192.0.2.53` WHEN the walk reaches that level THEN only `192.0.2.53` is recorded.
- GIVEN a trace referral with glue `127.0.0.1` WHEN the trace walk follows it THEN it is not recorded (as today, now through the context); glue `192.0.2.53` is recorded.
- GIVEN only public-standing NS addresses WHEN `ns_lame` and `ns_delegation` run THEN the recorded addresses and findings equal today's for the same canned answers, compared as sets (their order follows completion order).
- GIVEN the production context WHEN built THEN its `allow` is `is_allowed_target`.
- GIVEN any walk in a test WHEN it queries the root servers THEN the recording sender sees them (no real network).

## Decision log

- One context in `dns_raw` with injectable `allow`, glue resolver and sender, over a predicate per caller: one place decides, and tests can record addresses although the port is fixed.
- A refused address is a `Warning` in the NS categories, over `Failed`: the domain publishes a nameserver prism will not query, which is a finding, not proof of a lame server.
- `dns_trace` moves onto the context so no path keeps its own filter (P18 spirit).
- Refused glue is not resolved again, over resolving the name: the domain's own glue is what it published (independent reading).

## Open decisions

None.

## Out of scope

- IPv6 nameserver queries.
- The lens DNS section's weighting of `ns_lame`/`ns_delegation` (unchanged).
