# Spec: contract deliverables

Status: Active
Created: 2026-10-08

The infrastructure repository deploys `netray` as one container per subcommand and will drop the Traefik middlewares that today add the security headers and CORS. Before it can, the binary must deliver what the deployment relies on: a strict, checkable config per subcommand, the production headers set by the apps themselves, a 404 for lens snapshots that do not exist, and a lens that understands what its backends actually send. This spec delivers those four things.

## Decisions

| # | Decision |
|---|---|
| D1 | One config loader in `netray-common` serves all six subcommands. Each keeps its environment naming exactly as deployed: `PRISM_<SECTION>__<KEY>`, `TLSIGHT_…`, `IFCONFIG_…`, `LENS_…` (prefix separator `_`), `SPECTRA__…`, `BEACON__…` (prefix separator `__`). `<PREFIX>_CONFIG`, which names the config file, is never a config key. The prefix matches case-insensitively, as the `config` crate did before; an entry that is not valid UTF-8 is skipped. |
| D2 | `--check-config <path>` loads and validates, prints one line, and exits 0 or 1, for every subcommand. `netray ip --check` keeps its wider meaning (config and data files) and is not an alias. |
| D3 | Header parity means the apps emit exactly what production clients get from Traefik today: `secure-headers` on every response; `csp-tool-spa` on the five tools and lens, except under `/docs`, which keeps the relaxed CSP the Scalar reference needs; `cors-public-api` (`access-control-allow-origin: *`, methods `GET, POST, OPTIONS`, headers `Content-Type, Accept`, max-age 600, `cross-origin-resource-policy: cross-origin`) on the five tools and lens. ifconfig-rs's two-year HSTS moves into `netray-common` configuration (C9: at least equal; it gains `preload`); its extra `font-src 'self' data:` is dropped, because Traefik's CSP has replaced it in production all along (observed 2026-10-08, `curl -sI https://ip.netray.info/`). Production `/docs` receives `csp-tool-spa` today for the same reason; the relaxed `/docs` CSP takes effect once Traefik's middleware is removed. Tightening the CSP is not this spec (SDD M19). |
| D4 | `netray site` keeps its own header set, which already equals `secure-headers` plus `csp-netray-web`; this spec only asserts it. |
| D5 | lens parses what the backends send today; the backends' response shapes do not change (SDD M20). Golden files are written by each backend's own tests and read by lens's tests. |
| D6 | No Cloudflare sits in front of any service: neither `IpExtractor` nor ifconfig-rs's own `extract_client_ip` trusts `CF-Connecting-IP`, as lens already does since 0.12.0. |

## Requirements

1. `netray_common::config::load::<T>(path, prefix)` builds the config from an optional TOML file and the environment under D1's naming, deserialises `T`, and is the only loader any subcommand uses. The process environment is read without panicking on non-UTF-8 entries.
2. Every config struct reachable from a subcommand's top-level config, including `netray-common`'s shared ones, rejects unknown keys; a convention test fails when a `Deserialize` struct in a `config.rs` lacks `deny_unknown_fields`.
3. `netray <sub> --check-config <path>` for `lens dns tls http email ip`: exit 0 and `config ok: <path>` for a valid file; exit 1 and the loader's error for an unknown key, a missing file, or a value the service would refuse at startup — every subcommand's `validate()` covers what its startup rejects (zero rate limits and batch sizes, unparsable bind addresses, beacon's unparsable or zero `per_ip`, lens's zero badge TTL), plus tlsight's missing `custom_ca_dir`, which is a startup-only check so that SIGHUP reload keeps tolerating it. Data files on the host (ifconfig-rs GeoIP, lens scoring profile, snapshot database) are not checked; `netray ip --check` covers ifconfig-rs's. It starts no listener.
4. `tests/fixtures/<service>.production.toml` exists for all six services, mirrors the infrastructure repository's template for that service with placeholders for secrets and hosts, and a test per service loads it through the subcommand's loader. Differences between a fixture and its template are reported to the operator, never resolved by editing the infrastructure repository.
5. `IpExtractor` and ifconfig-rs's `extract_client_ip` use the peer address, and `X-Real-IP` / `X-Forwarded-For` from trusted proxies only; `CF-Connecting-IP` is ignored.
6. Every response of `lens dns tls http email ip` carries the `secure-headers` set of D3 and no `Server` or `X-Powered-By` header; every non-`/docs` response carries `csp-tool-spa` verbatim; every response carries `access-control-allow-origin: *` and `cross-origin-resource-policy: cross-origin`; a CORS preflight (`OPTIONS` with `Origin` and `Access-Control-Request-Method: POST`) answers 2xx with the D3 methods, headers and `access-control-max-age: 600`.
7. ifconfig-rs's HSTS difference from the shared layer is a field of `netray-common`'s `SecurityHeadersConfig`, not code in ifconfig-rs; ifconfig-rs sets no security header of its own.
8. The acceptance suite asserts 6 per host (HSTS `max-age` at least one year; `/docs` CSP not asserted, since Traefik overrides it until cutover), and `netray site`'s set (D4) for the apex paths; it runs against production and against a locally started `netray` with no proxy in front. It also asserts that `https://mta-sts.netray.info/.well-known/mta-sts.txt` serves `version: STSv1` and `mode: enforce`, replacing the TODO in `tests/acceptance/static-site/assets.spec.ts`.
9. lens `/r/<id>` answers 404 with the "expired or unknown" HTML page for an unknown id, an expired id and a malformed id.
10. Each backend's tests write a golden response from its real response types to `tests/fixtures/contracts/<backend>.*` in the workspace: prism's batch SSE stream, tlsight's and spectra's `InspectResponse`, beacon's SSE stream including its summary, ifconfig-rs's `/network/json` body. A test fails when the written golden differs from the committed one.
11. lens's backend parsers read the committed goldens and produce a non-default result from each: a grade from beacon's `verdicts`, the network type and location from ifconfig-rs's bare network body, checks from prism, tlsight and spectra.
12. lens ends a beacon stream at beacon's summary event, as beacon actually frames it, not at stream end.

## Phase 1 — Config loader and check

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

### Test Scenarios

- GIVEN a config struct with `deny_unknown_fields` WHEN `load` is given a file with an unknown key THEN it errors naming the key.
- GIVEN `PRISM_CONFIG=/x.toml` and `PRISM_LIMITS__PER_IP_PER_MINUTE=60` in the environment WHEN prism's config loads THEN `limits.per_ip_per_minute` is 60 and `config` is not reported as unknown; the same for `SPECTRA__…` and `BEACON__…` with their `__` prefix separator.
- GIVEN an environment entry that is not valid UTF-8 WHEN any loader runs THEN it does not panic.
- GIVEN the workspace WHEN the convention test scans every `config.rs` THEN each `#[derive(…Deserialize…)]` struct carries `deny_unknown_fields`.
- GIVEN each production fixture WHEN `netray <sub> --check-config <fixture>` runs THEN it exits 0; GIVEN the fixture plus one unknown key THEN it exits 1; GIVEN a missing path THEN it exits 1.
- GIVEN the spectra fixture with `per_ip_per_minute = 0`, the beacon fixture with `per_ip = "0/min"` or `"ten"`, or the tlsight fixture with a nonexistent `custom_ca_dir` WHEN `--check-config` runs THEN it exits 1.
- GIVEN each of the six services WHEN its fixture test runs THEN the fixture loads through that service's loader.
- GIVEN a request from a trusted proxy with `CF-Connecting-IP: 198.51.100.1` and `X-Real-IP: 198.51.100.2` WHEN the client IP is extracted THEN it is `198.51.100.2`. The same holds for ifconfig-rs's `extract_client_ip`.
- GIVEN `prism_limits__per_ip_per_minute=60` (lower-case prefix) WHEN prism's config loads THEN the value is applied, as before.

## Phase 2 — Header and CORS parity

**Depends on:** none
**Requirements:** 6, 7, 8

### Test Scenarios

- GIVEN each of `lens dns tls http email ip` WHEN `/` and a JSON API route are requested in-process THEN the response carries the D3 `secure-headers` values, `csp-tool-spa` verbatim, `access-control-allow-origin: *` and `cross-origin-resource-policy: cross-origin`, and no `server` header.
- GIVEN each service WHEN `/docs` is requested THEN its CSP is the relaxed one that allows the Scalar script, and the other D3 headers are present.
- GIVEN each service WHEN a CORS preflight for `POST` is sent THEN it answers 2xx with methods `GET, POST, OPTIONS`, headers `content-type, accept` and max-age 600.
- GIVEN ifconfig-rs WHEN its security headers are built THEN its HSTS comes from the `SecurityHeadersConfig.hsts` field, and its crate sets no `Strict-Transport-Security` or `Content-Security-Policy` of its own (its `Vary` and `Cache-Control` stay).
- GIVEN each service's `/docs` page WHEN its CDN scripts are checked against the relaxed `script-src` by CSP source-matching rules THEN every one is allowed.
- GIVEN the acceptance suite WHEN its header spec runs against production THEN it passes (operator-observed; network, outside the gate).
- GIVEN the acceptance suite WHEN its `mta-sts` spec runs against production THEN the policy has `version: STSv1` and `mode: enforce`.

## Phase 3 — lens snapshot 404

**Depends on:** none
**Requirements:** 9

### Test Scenarios

- GIVEN no snapshot with id `abcdefgh` WHEN `/r/abcdefgh` is requested THEN the status is 404 and the HTML says the snapshot is expired or unknown.
- GIVEN a snapshot past its expiry WHEN its `/r/<id>` is requested THEN the status is 404 with the same page.
- GIVEN a malformed id (`/r/x`, `/r/not-a-valid-id!`) WHEN requested THEN the status is 404 with the same page, not 400 JSON.

## Phase 4 — lens and its backends agree

**Depends on:** none
**Requirements:** 10, 11, 12

### Test Scenarios

- GIVEN each backend's response types WHEN its golden test runs THEN the written golden equals the committed one under `tests/fixtures/contracts/`.
- GIVEN a field in beacon's summary renamed WHEN the beacon golden test runs THEN it fails.
- GIVEN beacon's committed golden WHEN lens parses it THEN the email result carries the grade the golden's `verdicts` imply, not the default.
- GIVEN ifconfig-rs's committed `/network/json` golden WHEN lens parses it THEN `network_type` and the location are the golden's values, not `unknown`.
- GIVEN prism's, tlsight's and spectra's goldens WHEN lens parses each THEN the result has at least one check and no parse error.
- GIVEN a beacon stream whose summary arrives before the connection closes WHEN lens collects it THEN it returns at the summary.

## Open decisions

None.
