# Plan: v2-correctness

## Phase 1 — Email: truncated answers and the SPF pass

### Groups

G1: C1, C3, C4, C5, C6 · G2: C2, C7, C8, C9 (independent files).

### Plan

- G1 `crates/email/src/dns/resolver.rs`: `DnsResolver::new` collects the `NameServerConfig`s (system expanded via `NameServerConfigGroup::from_system_config`), builds the group from them, and keeps per UDP config a TCP twin `Resolver` for the same address and port (`tcp: Vec<Option<Resolver>>`, indexed like `resolvers`). `pick` returns the pair; every lookup threads it into `lookup_on`, which retries once on the twin when the cause contains "response was truncated"; a failed retry's cause is "TCP retry failed: <cause>".
- G2 `crates/email/src/checks/spf.rs` `check_spf_measured`: the `spf_ok` guard becomes "no Warn or Fail sub-check" instead of "no sub-check".
