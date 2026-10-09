# Plan: backend correctness

## Phase 2 — IP reputation

### Groups

- G1: C2, C11 (ifconfig-rs)
- G2: C1, C3, C4, C5, C6, C7, C8, C9, C10 (lens)

The groups share no production file. G2 does not depend on G1: the lens tests read the committed `tests/fixtures/contracts/ifconfig-json.json`, which G1 leaves unchanged (`/json`'s order stays).

### Plan

#### G1 (test: crates/ifconfig-rs/src/classify_tests.rs)

1. `crates/ifconfig-rs/src/backend/mod.rs`: add `NetworkFlags` and `classify_network_type(NetworkFlags) -> &'static str` (internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential); `get_ifconfig` uses it in place of its inline chain. `/network` shares `get_ifconfig` with `/json`, so it needs no change of its own. `infra_type` stays.
2. `crates/ifconfig-rs/src/routes.rs` `range_handler`: look up Tor exit nodes and the bot DB for the network address, derive `is_internal`, fold the ASN VPN heuristic into `is_vpn`, and classify through `classify_network_type`.

#### G2 (tests: crates/lens/tests/ip_reputation_flags.rs, ip_enrichment_errors.rs, ip_sampling.rs, contract_ip.rs, unknown_verdicts.rs)

1. `crates/lens/src/backends/ip.rs`:
   - `NetworkInfo` gains the required `is_spamhaus`, `is_c2`, `is_tor`, `is_vpn`, and `network` loses its serde default.
   - `IpBackend` gains `allow: fn(IpAddr) -> bool`, and `check_ip` takes it as its sixth parameter.
   - The sample is the public addresses, split by family, each family sorted, at most four of each, IPv4 first. `MAX_IPS` goes.
   - A failed or timed-out enrichment returns Err, and the `unknown` arm goes.
   - The verdict comes from the flags, and `network_type_verdict` goes.
   - The messages are `<ip>: <flag>`, plus `checked N of M addresses` when N < M.
   - No public address: `check_ip` returns Ok with no checks, and `IpBackend::run` returns `NotApplicable { reason: "no public addresses" }`.
2. `crates/lens/src/state.rs`: `allow: netray_common::target_policy::is_allowed_target`.
3. `crates/lens/src/routes.rs` (cfg(test) only): `ip_backend_uses_json_path` moves to the new signature and serves a body that carries the flags.
4. `tests/fixtures/contracts/lens-*.json`: regenerated. The only address in `prism.sse` is the documentation address 192.0.2.10, which the production policy does not enrich, so the ip section of each golden becomes N/A.

#### G3 — requirement 7, C12–C17 (test: crates/ifconfig-rs/src/rate_limit_exempt_tests.rs)

Added after the reader's rate-budget AMENDMENT and the operator's decision. One group, planned inline: it touches one crate's config, state and middleware.

1. `crates/ifconfig-rs/src/config.rs` `RateLimitConfig`:
   - Add `#[serde(default)] pub exempt_cidrs: Vec<String>`, with an empty default; `deny_unknown_fields` stays.
   - `Config::validate` rejects an entry that does not parse as an `IpNetwork`, with a message naming `rate_limit.exempt_cidrs`.
2. `crates/ifconfig-rs/src/state.rs`: `AppState` gains `rate_limit_exempt: Arc<Vec<IpNetwork>>`, parsed the way `trusted_proxies` is.
3. `crates/ifconfig-rs/src/middleware.rs` `rate_limit`: the per-IP check is skipped when the request's `ConnectInfo<SocketAddr>` peer lies in `rate_limit_exempt`. The match uses the peer and never the forwarded `RequesterInfo.remote`. The per-target limiter in `routes.rs` is untouched.
4. `crates/ifconfig-rs/tests/fixtures/ifconfig.production.toml`: `exempt_cidrs = []` under `[rate_limit]`, so argus's key-path comparison sees the key.
