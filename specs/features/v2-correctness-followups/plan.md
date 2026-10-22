# Plan: v2-correctness-followups

## Phase 1 — TLS: legacy-version alerts refuse

## Groups

- G1: C1 to C9 (one production file).

## Plan

- G1: `crates/tls/src/tls/probe.rs`, `decide`: a second alert list `REFUSING_VERSION_ALERTS`
  (10, 40, 47, 50, 70, 71, 86, 112) applies to version probes, with close_notify at either level;
  suite groups (`ProbeKind::is_suite_group`) keep `REFUSING_ALERTS`. The protected
  `probe_alerts.rs` table moves to a suite-group kind, because its Tls10 rows for 50, 112 and 0
  now refuse. Planned by the implementer directly: one function in one file.

## Phase 2 — DNS: domain runs pay no per-target limit

## Groups

- G1: C1, C3, C4 (C2 and C5 already hold).

## Plan

- G1: `crates/netray/src/main.rs`, `lens_registry`: the registry's `DnsModule` is built
  `.admitted_by_caller()` like the band's; it stays its own instance (circuit breakers, query
  semaphore). `crates/dns/src/module.rs`: doc comments of `DnsModule` and `admitted_by_caller`.
  Planned by the implementer directly.
