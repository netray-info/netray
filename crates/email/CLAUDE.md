# CLAUDE.md -- netray-email

The email module of `netray serve` (V1 codename beacon). Repo-wide rules, including the email
ones (verdict order, goldens, `--features testing`, DKIM selectors in recorded runs), are in the
root [`CLAUDE.md`](../../CLAUDE.md); this file holds only what is specific to this crate.
[`README.md`](README.md) lists the check ids and config keys.

## Layout

- `src/module.rs` -- `EmailModule` (the engine module), `translate` (category results onto the
  `email.*` checks and blocks), `REGISTER`, `BUCKETED`, `EXCLUDED`, the cross-validation routing.
- `src/job.rs` -- `EmailInspector` and the job types netray-web runs for API v1 and the page.
- `src/checks/` -- one file per category plus `cross_validation`; `mod.rs` holds `run_all_checks`,
  the orchestration.
- `src/quality/` -- `Verdict`, `Grade`, `CheckResult`, `SseEvent` and `compute_grade`.
- `src/dns/` -- `DnsLookup` and `DnsResolver`; `test_support.rs` the test resolver.
- `src/input.rs` -- domain and DKIM selector validation.
- `src/config.rs` -- `ModuleConfig`, the `[modules.email]` table.

## Working rules

- **Three phases.** `run_all_checks` runs phase 0 (domain-only checks) in parallel, phase 1
  (MX-dependent checks) in parallel after phase 0 has drained, then phase 2 (cross-validation and
  grade) sequentially; each phase uses a `JoinSet`. Results travel as `SseEvent`s over an mpsc
  channel; `EmailModule::run` collects them until the `Summary` event.
- **The 30 s cap.** `run_all_checks` wraps the inspection in a 30 s timeout; on expiry it emits a
  `Summary` with every category `Skip` and `Grade::Skipped`. `[inspect.email] deadline_ms` defaults
  past it.
- **Two resolvers.** `[modules.email.dnsbl] resolvers` (default `system`) is used only for DNSBL
  queries: public DNSBLs block queries from open resolvers. Every other lookup goes through
  `[modules.email.dns] resolvers`.
- **Lookup errors.** `DnsLookup` returns `Result<_, LookupError>`: NXDOMAIN/NODATA is `Ok(empty)`,
  SERVFAIL, REFUSED or a timeout is `Err`. A check whose own answer failed emits the `unmeasured`
  sub-check, which `translate` maps to `Unmeasured`; a failed side lookup is an Info
  `lookup_failed`, never a Warn.
- **Busy is incomplete.** `EmailModule::run` takes a slot from `[modules.email.inspections]
  max_concurrent` without waiting; none free returns `SectionOutcome::Incomplete` ("busy").
- **Volatile rows.** `email.mx_addresses`, `email.fcrdns` and `email.dnsbl` rows are volatile
  (rotating addresses); see the root rules on volatile paths before adding one.
- **Fetches.** MTA-STS fetches the policy over HTTPS with no redirect and a 64 KiB body cap; BIMI
  follows up to 4 redirects. Both go through `netray_common::fetch`.
- **SPF expansion** is recursive with a depth cap of 10, a visited set against loops and a
  saturating void-lookup count.
- **DKIM selectors** come from a static provider map keyed by MX host plus the caller's selectors
  (at most `max_user_selectors`); no enumeration.
- **DNSSEC** is DNSKEY presence only, and is scored in the DNS section, not as an email check.
- **The V1 grade** (`compute_grade`: 0F/0W A, 0F/1-2W B, 0F/3+W C, 1F D, 2+F F; Skip excluded) is
  carried as the `email.summary` block.

## Rules

- [architecture-rules](../../specs/rules/architecture-rules.md) when changing the API v1 email
  inspection, rate limits or outbound fetches.
- [logging-rules](../../specs/rules/logging-rules.md) when changing tracing or metrics.
