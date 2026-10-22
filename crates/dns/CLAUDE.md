# CLAUDE.md — netray-dns

The DNS module of `netray serve` (V1 codename prism): `DnsModule` (engine `Module` and
`FactsProvider`) and the `DnsJobs` netray-web runs. Repo-wide rules, verbs and the gate are in the
root [CLAUDE.md](../../CLAUDE.md); module-level rules already stated there (Rule M, the catalogue,
`--features testing`, `named_record_types`, `parser::parse` is no validation, the DNS goldens) are
not repeated here. What the module measures and reads: [README.md](README.md).

## Working rules

- **The open-resolver surface is load-bearing.** The DNS jobs query user-chosen servers for
  user-chosen names; keep every policy layer below intact when changing a job or the check.
- **The parser is the single source of truth for query semantics** (`parser.rs`): ANY, AXFR and
  IXFR are refused there, at most 10 record types and 4 servers.
- **Two record-type caps stay equal**: `parser::MAX_RECORD_TYPES` and `config::HARD_CAP_RECORD_TYPES`;
  a compile-time assert holds them together.
- **Hard caps clamp, zero limits refuse** (`Config::validate`): `max_timeout_secs` at most 10,
  `max_servers` at most 4, the trace's hops and query timeout at most 20 and 10 s. Config cannot
  raise a hard cap.
- **Server input goes through `QueryPolicy`**: `validate` for jobs, `validate_for_check` for the
  module (no type count). It refuses server IPs in blocked ranges (`check_target_ip`, over
  `netray_common::ip_filter::is_blocked_ip`), and system resolvers and arbitrary servers unless
  `[modules.dns.dns]` allows them.
- **Raw queries to addresses taken from the checked domain's data** (NS checks, authcompare, the
  DNSSEC walk, trace) go through `dns_raw::RawOutbound`, whose `allow` is `is_allowed_target`.
- **Job costs** (`jobs/mod.rs`): query `record_types x servers`, compare `x transports`,
  authcompare `record_types x servers x 2 + 16`, trace and dnssec a flat cost against per-IP and
  global limiters only. A change to a cost moves the minimum burst `[api.dns]` validates
  (`DNS_MIN_JOB_BURST` in `crates/web/src/api/config.rs`).
- **The module's instances are admitted by their caller** (`admitted_by_caller`): the registry
  builds both without a per-target limiter, so `[modules.dns.limits]`' per-target keys are parsed
  but not read. Do not add a refusal inside a run; admission belongs to netray-web.
- **Each `DnsModule` owns its circuit breakers and query semaphore**; `jobs()` shares them with
  the jobs it builds.
- **A check id lives in three places here**: `CHECKS` in `module.rs` (what `checks()` reports;
  an id it does not report becomes `not_applicable`, or `unmeasured` after a stream timeout),
  `CHECK_INPUTS` (the record types whose failed lookup makes it `unmeasured`; `ttl` reads every
  base-name batch) and, for evidence-only inputs, `EVIDENCE_ONLY_INPUTS`. Plus the catalogue, per
  the root CLAUDE.md.
- **`translate` drops the check pipeline's email categories**; they belong to the email module.
- **A new records block** goes into the `records_blocks!` list, which defines the register and
  the volatile `/ttl` paths together (root CLAUDE.md: a new volatile path changes every stored
  fingerprint).

## Rules

- [`specs/rules/architecture-rules.md`](../../specs/rules/architecture-rules.md) when changing
  config, errors, rate limits or the jobs' protocol.
- [`specs/rules/logging-rules.md`](../../specs/rules/logging-rules.md) when changing log targets.
