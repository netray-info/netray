# Plan: v2-correctness

## Phase 1 — Email: truncated answers and the SPF pass

### Groups

G1: C1, C3, C4, C5, C6 · G2: C2, C7, C8, C9 (independent files).

### Plan

- G1 `crates/email/src/dns/resolver.rs`: `DnsResolver::new` collects the `NameServerConfig`s (system expanded via `NameServerConfigGroup::from_system_config`), builds the group from them, and keeps per UDP config a TCP twin `Resolver` for the same address and port (`tcp: Vec<Option<Resolver>>`, indexed like `resolvers`). `pick` returns the pair; every lookup threads it into `lookup_on`, which retries once on the twin when the cause contains "response was truncated"; a failed retry's cause is "TCP retry failed: <cause>".
- G2 `crates/email/src/checks/spf.rs` `check_spf_measured`: the `spf_ok` guard becomes "no Warn or Fail sub-check" instead of "no sub-check".

## Phase 2 — DNS: the zone of a name below the apex

### Groups

G1: C1-C16 (one production file, `crates/dns/src/api/check.rs`).

### Plan

- `run_check_on`, after phase 1 (all batches in): if the SOA batch holds no SOA record owned by the queried name, find the zone by `fan_out_lookup` of SOA at each ancestor (drop one label; stop before a single-label name). Those lookups are not sent as batches and not merged into `all_lookups`. An ancestor whose SOA lookup has no definite answer (every lookup an error or non-definite negative, as `module::batch_error` judges) stops the climb: zone unknown, carrying a finding "SOA lookup of <ancestor> failed: <causes>". All definite negatives: zone = the name (today's behaviour).
- Zone found below the name: `fan_out_lookup` NS, DNSKEY, DS at the zone; send each as a `CheckEvent::Batch` with record_type "NS"/"DNSKEY"/"DS" and merge into the lookups the lints read (`all_lookups`); the lame/consistency checks run with the zone name over `unique_records` of the merged lookups.
- Zone unknown: emit the six zone checks (`ns`, `ns_lame`, `ns_delegation`, `dnssec`, `dnskey_algorithm`, `dnssec_rollover`) as `unmeasured` with that finding. The translation in `module::translate` takes check verdicts from lint events and batch errors: either send a failed batch event the translator turns into unmeasured, or add an event/lint path that carries the unmeasured status; pick the smallest change that keeps `translate` the single place statuses are made (e.g. a `CheckEvent::Unanswered { category, silent, none_answered: true }` per check carrying the finding, which `translate` already maps to Unmeasured).
- CAA: when the CAA batch at the name holds no CAA record, `fan_out_lookup` CAA at each ancestor (same stop rule) until one answers CAA records; send that as a "CAA" batch and merge it. An ancestor CAA lookup with no definite answer stops: `dns.caa` unmeasured naming "CAA lookup of <ancestor> failed: ..." (same Unanswered path).
- Apex names: no extra lookup at all.
