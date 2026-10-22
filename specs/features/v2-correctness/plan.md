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

## Phase 3 — DNS: TTLs from the authoritative servers

### Groups

G1: C1-C5 (`crates/dns/src/api/check.rs`).

### Plan

- After the zone phase, before the lints: collect the (name, type) pairs whose highest TTL in the unique records is below 60 s (and above 0, as `check_ttl` judges). None: nothing more. Otherwise resolve the zone's nameserver addresses as the lame check does (`resolve_missing_glue`, `build_server_list`), query each pair at those servers (RD=0 raw queries, `RawOutbound::parallel_queries`) and, for answers with AA=1, take the highest TTL per record. Build mhost `Lookups` for those answers through the serde form (as `unique_records` rewrites lookups), send them as a batch of the pair's type and merge them into what the lints read, so `check_ttl` and the records block see the authoritative TTL. No answer: nothing is sent (the recursive TTL stays).
- Bounded by the stream deadline, like the zone phase.
- Review: names compare as A-labels; every lookup is searched for the pair's records; only names at or below the zone are re-read; the authoritative TTL covers the RRset.

## Phase 4 — DNS inspector: reverse names and `@system`

### Groups

G1: C1, C3, C4, C5 (`crates/dns/src/parser.rs`, where an IP domain becomes a PTR query) · G2: C2, C6, C7 (`crates/dns/src/api/query.rs` `build_resolver_group`).

### Plan

- G1: when the parsed domain is an IP address, the query's lookup name becomes its reverse name (`hickory_proto::rr::Name::from(IpAddr)` gives `…in-addr.arpa.` / nibble `ip6.arpa.`); keep the record-type defaulting. Check every consumer of `ParsedQuery.domain` that expects the IP form (warnings, policy checks, page labels) and keep their behaviour.
- G2: after `builder.build()`, one breaker key per resolver: for `@system`, push "system" once per resolver the system entry added (compute from the group's resolver count minus the keys of the other entries, or build the system configs explicitly via `NameServerConfigGroup::from_system_config` and add each with its own "system" key).
