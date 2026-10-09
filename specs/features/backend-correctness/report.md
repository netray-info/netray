# Report: backend correctness

## Phase 2 — IP reputation

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R3: lens scores an address from ifconfig-rs's booleans (`is_spamhaus`, `is_c2`, `is_tor` → Fail; `is_vpn` → Warn; else Pass); enriches only public addresses (`netray_common::target_policy`), a non-public one not sent and counted as not checked; a failed or timed-out public enrichment makes the section Errored; at most four IPv4 and four IPv6, each family sorted; "checked N of M addresses" when fewer checked than resolved | green | crates/lens/tests/ip_reputation_flags.rs |
| C2 | R4: ifconfig-rs classifies with one function in `/json`'s order (internal > c2 > bot > cloud > vpn > tor > spamhaus > datacenter > residential), used by `/json`, `/network` and `/range` (on the range's network address) | green | crates/ifconfig-rs/src/classify_tests.rs |
| C3 | GIVEN the ifconfig golden with `type: cloud` and `is_spamhaus: true` WHEN lens scores it THEN reputation Fail | green | crates/lens/tests/ip_reputation_flags.rs |
| C4 | GIVEN `is_vpn: true` and no other flag WHEN scored THEN Warn; GIVEN no flag THEN Pass | green | crates/lens/tests/ip_reputation_flags.rs |
| C5 | GIVEN every enrichment call answering 500 WHEN scored THEN the IP section is Errored and the result incomplete | green | crates/lens/tests/ip_enrichment_errors.rs |
| C6 | GIVEN one of two addresses timing out WHEN scored THEN Errored | green | crates/lens/tests/ip_enrichment_errors.rs |
| C7 | GIVEN ten A and two AAAA addresses WHEN lens enriches THEN four A (lowest four sorted) and two AAAA are enriched and the detail says "checked 6 of 12 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C8 | GIVEN five A addresses WHEN lens enriches THEN the lowest four, detail "checked 4 of 5 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C9 | GIVEN three A addresses WHEN lens enriches THEN all three, no "checked" detail | green | crates/lens/tests/ip_sampling.rs |
| C10 | GIVEN A 198.51.100.7 and A 10.0.0.5 WHEN lens enriches THEN only 198.51.100.7 is sent, the section is scored (not Errored), detail "checked 1 of 2 addresses" | green | crates/lens/tests/ip_sampling.rs |
| C11 | GIVEN one fixture address WHEN ifconfig-rs answers `/json`, `/network` and `/range` for `<addr>/32` THEN all three report the same type | green | crates/ifconfig-rs/src/classify_tests.rs |
| C12 | R7: `[rate_limit] exempt_cidrs` (default empty); a TCP peer inside skips the per-IP limiter, matched on the peer and never a forwarded header; a non-CIDR entry fails validation; per-target limiter still applies; the production fixture carries the key | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C13 | GIVEN `exempt_cidrs = ["172.30.0.0/24"]`, 172.30.0.0/24 trusted, burst 1 WHEN peer 172.30.0.5 sends three requests with XFF 198.51.100.1 THEN none is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C14 | GIVEN the same config WHEN a non-exempt peer (trusted 172.31.0.2 or untrusted 203.0.113.9) sends two requests with XFF 172.30.0.5 THEN the second is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C15 | GIVEN no `exempt_cidrs`, burst 1 WHEN peer 172.30.0.5 sends two requests THEN the second is 429 | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C16 | GIVEN `exempt_cidrs = ["nope"]` WHEN validated THEN it fails naming `rate_limit.exempt_cidrs` | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |
| C17 | GIVEN `ifconfig.production.toml` WHEN loaded THEN it carries `rate_limit.exempt_cidrs` and validates | green | crates/ifconfig-rs/src/rate_limit_exempt_tests.rs |

RED was confirmed at 7b4bcfa. Every new test failed to compile against the missing interface: the `allow` parameter and field, and `NetworkFlags`/`classify_network_type`.

Two harness defects in `classify_tests.rs` surfaced only once it compiled. I repaired them, and the production commit carries `ADLC-Test-Change` for them:
- The bare router has no `requester_info_middleware`, so the test now inserts `RequesterInfo` itself.
- `/network` negotiates HTML without an `Accept` header.

`lens_golden.rs` now serves the DNS golden's A record 192.0.2.10 as 1.1.1.1. 192.0.2.10 is a documentation address, which the production policy does not enrich. With the substitution, the IP section stays scored and no lens golden moves.

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 (C2, C11) | 1 | sonnet (after the two test-harness repairs above) | 28873 | 74 |
| G2 (C1, C3–C10) | 3 | sonnet | 38208 | 104 |
| reader fixes (second pass) | 1 | sonnet | 35329 | 99 |
| G3 (C12–C17) | 1 | sonnet | 30368 | 84 |

### Review

The reader raised three BLOCKERs. The second pass fixed all three:
- **`/range`'s datacenter flag.** `/range` left out the `asn_info` Hosting term, so its classifier got different inputs than `/json`'s. It now includes it (`crates/ifconfig-rs/src/routes.rs` `range_handler`).
- **NotApplicable rendered as error.** A NotApplicable section showed as status `error`, so `overall` read `error` while `complete` was true. It now shows as `skip` (`crates/lens/src/routes.rs` `section_status_from_verdicts`, with the unit test `not_applicable_ip_section_is_skip_and_not_overall_error`).
- **PTR lookup.** lens asked for a PTR lookup it never reads, and that lookup could exceed lens's 2 s ip timeout and error the section. lens now sends `&dns=false` (`crates/lens/src/backends/ip.rs`).

AMENDMENTs:
- AMENDMENT | crates/lens/src/backends/ip.rs | The spec is silent on the rate budget between lens and ifconfig-rs. One check now sends up to 8 `/json` calls (5 before), and ifconfig-rs allows a burst of 10 and 60 a minute per forwarded client IP, so two quick checks of an 8-address domain can hit 429. A 429 now makes the section Errored. | affected_phase: 2 | repaired_in_phase: yes. The operator decided on 2026-10-09 to repair it through requirement 7; see "Requirement 7" below.
- AMENDMENT | spec.md Context | "No lens golden moves through … IP" holds only because the golden harness substitutes a public address for prism.sse's documentation address 192.0.2.10. | affected_phase: 2 | repaired_in_phase: yes
- AMENDMENT | crates/lens/src/backends/ip.rs | The spec is silent on a domain with no public address. Implemented as `NotApplicable("no public addresses")`: the section is excluded from scoring, `complete` stays true and the status is `skip`. This follows the decision log's "would be incomplete forever"; P10's list of N/A-by-design cases gains this one. | affected_phase: 2 | repaired_in_phase: yes

DEFERRED:
- The frontend shows a NotApplicable section's reason only for status `error`. A skipped IP or email section shows a grey dot with no reason, and the IP section's link has `href=""` (`IpSection.tsx`, `EmailSection.tsx`). This is UI work (V2).

NIT:
- No test pins that production lens wires `is_allowed_target` (`crates/lens/src/state.rs`). The tests inject a policy that admits documentation addresses.

### Behavioural verification

skipped: lens's entry point (`POST /api/check`) needs prism, tlsight, spectra, beacon and ifconfig-rs running. `lens_golden` and the `ip_*` tests drive it in-process against stubs. ifconfig-rs's `/json`, `/network` and `/range` are driven through its real router in-process by `json_network_and_range_report_the_same_type`.

### Requirement 7 — rate-limit exemption

**How it works today.** lens sends every backend call with `X-Forwarded-For: <visitor IP>` (`crates/lens/src/backends/mod.rs` `forward_headers`). lens reaches ifconfig-rs from the docker `backend` network, 172.30.0.0/24. ifconfig-rs lists that network in `server.trusted_proxies`, so `extract_client_ip` honours the header, and the per-IP limiter (`middleware::rate_limit`) keys on the visitor. A check's up to eight enrichment calls therefore come out of the visitor's own budget: a burst of 10, then 60 a minute.

**The change.**
- **The key.** ifconfig-rs gains `[rate_limit] exempt_cidrs`, empty by default; `deny_unknown_fields` stays and `test_config_strict.sh` passes.
- **The match.** A request whose TCP peer (`ConnectInfo`) lies in one of those networks skips the middleware's per-IP limiter. The match uses the connection peer and never a forwarded header, so a public client arriving through Traefik (peer 172.31.0.2) cannot claim the exemption with `X-Forwarded-For` (C14).
- **Validation.** An entry that is neither a CIDR nor an IP fails `Config::validate`.
- **What still applies.** The per-target limiter still applies. `/batch` and `/diff` charge tokens in their handlers and stay limited; no internal caller uses them, and the spec now says so.
- **Fixtures and examples.**
  - The production fixture carries `exempt_cidrs = ["172.30.0.0/24"]`, the subnet its `trusted_proxies` already names.
  - `ifconfig.example.toml` documents the key.

Spec amended in 694984b: requirement 7 and its scenarios, plus the decision-log entry.

**Reader findings.**
- AMENDMENT (`/batch` and `/diff` stay limited for an exempt peer): repaired in the spec wording.
- AMENDMENT (the fixture exempted nothing): repaired with the fixture value, and `c17` now pins it.
- NIT, not acted on: an IPv4 exempt CIDR does not match an IPv4-mapped peer on a dual-stack `[::]` bind. This fails closed, `trusted_proxies` has the same gap, and production binds `0.0.0.0`.

**New K item for the planning session (argus-oci).**

> **K — ifconfig-rs exempts lens from its per-client limit.** `ansible/roles/app_stack/templates/ifconfig.toml.j2`, `[rate_limit]`: add `exempt_cidrs = {{ ifconfig_rs_rate_limit_exempt | to_json }}`, with `ifconfig_rs_rate_limit_exempt: ["{{ backend_subnet }}"]` (172.30.0.0/24, the network lens reaches ifconfig on) in `ansible/inventory/group_vars/all/main.yml`. This is safe only while Traefik has no address on `backend`; today it sits on `edge`, `traefik-proxy`, `observability` and `socket-proxy`. A tighter alternative is a fixed `ipv4_address` for lens on `backend` and a `/32` exemption. Needs the release carrying backend-correctness requirement 7. Until then the key is unknown to the deployed ifconfig-rs, and `deny_unknown_fields` would refuse to start.
