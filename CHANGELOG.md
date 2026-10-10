# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.24.0] - 2026-10-10

Grades and results are unchanged; this release changes how lens runs and how every service is configured.

### Changed

- **lens runs the five checks in-process.** lens no longer calls the five check services for its sections; it runs the check modules itself through the new engine (`netray-engine`), which resolves A, AAAA, MX, CAA, NS and HTTPS once per run and streams each section as it finishes. The services keep running as `netray dns|tls|http|email|ip` for their own pages and APIs. The DNS, TLS, HTTP and email modules inside lens still enrich addresses over HTTP from `netray ip` (`[modules.dns.backends.ip] url`, `[modules.tls.backends.ip] url`, `[modules.http.enrichment] ip_url`, `[modules.email.backends] ip_url`), so lens keeps its route to the IP service.
- **lens configuration.** `[backends.<section>]` keeps only `timeout_ms`; a `url` is refused. The HTTP and email sections run when their `[backends.http]` / `[backends.email]` table is present (in 0.23 the `url` switched them on): keep the table after removing `url`. DNS and TLS always run; IP runs when `[modules.ip]` is present. Each section's check settings move to `[modules.dns|tls|http|email|ip]` with the services' own key names (see `crates/lens/lens.example.toml`). `[backends] dns_servers` moves to `[modules.dns] servers`. New `[backends] resolve_timeout_ms` (default 1500) bounds the shared lookup stage; it must stay below `[backends.ip] timeout_ms`, because the IP section waits for it inside its own window.
- **lens needs the IP data.** With `[modules.ip]`, lens loads the GeoIP and reputation data itself (both GeoIP databases are required; the lists warn when missing) and reloads it on SIGHUP, so the lens container needs the same read-only data mount as `netray ip` (`/netray/data`). Without `[modules.ip]`, lens has no IP section and says so at startup.
- **`netray lens --check-config` validates the `[modules.*]` tables** (unknown keys, required keys, values) and builds the modules, but reads no IP data files, as `netray ip --check-config`; lens's startup refuses a missing or unparsable GeoIP file and names its key. A configured `[modules.tls.validation] custom_ca_dir` is still checked.
- **Environment variables.** Every check service reads `NETRAY_<SERVICE>_` (nesting `__`) and `NETRAY_<SERVICE>_CONFIG`: `NETRAY_DNS_`, `NETRAY_TLS_`, `NETRAY_HTTP_`, `NETRAY_EMAIL_`, `NETRAY_IP_`. The old prefixes (`PRISM_`, `TLSIGHT_`, `SPECTRA__`, `BEACON__`, `IFCONFIG_`) refuse startup with a message naming the new one. lens keeps `LENS_`.
- **Crates renamed to their protocols:** `crates/dns`, `tls`, `http`, `email`, `ip` (packages `netray-dns` …). Metric names, config files and subcommands are unchanged, except that `lens_unknown_verdict_total{section}` is no longer emitted (the sections' verdicts are typed in-process; an unknown one cannot reach lens). Log targets follow the crate names (`netray_dns`, `netray_tls`, `netray_http`, `netray_email`, `netray_ip`): a `RUST_LOG` that names `prism`, `tlsight`, `spectra`, `beacon` or `ifconfig_rs` needs the new names.
- lens's `/ready` no longer probes backend services (there are none) and answers 200.

### Added

- `netray-model` (the V2 check vocabulary) and `netray-engine` (the `Module` and `FactsProvider` traits, the registry and the run).

## [0.23.1] - 2026-10-10

### Added

- lens metrics for admission planning: `lens_check_requests_total{result}` (`fresh`, `cache_hit`, `rate_limited`), `lens_runs_in_flight`, `lens_run_duration_seconds` and `lens_client_hourly_runs` (fresh runs per client per hour, as a distribution; no client address is exported).
- lens's badge and rate-limit counters (`lens_badge_requests_total`, `lens_rate_limit_hits_total`) exist at zero from startup.

## [0.23.0] - 2026-10-09

Grades change in this release. Old snapshots keep the grade they were stored with.

### Changed

- **Incomplete results.** A lens result with an errored or timed-out section, or a scored section with no weighted check, has grade `incomplete` and `complete: false`; finished sections keep their own scores. It is never cached, snapshotted, or shown as a letter: the badge and the OG card show `?`. (R3.2)
- **TLS reachability.** tlsight emits `tls_reachable` per port; lens weights it 10 and makes it a hard fail, so a site without HTTPS grades F. A connect error raised on the inspecting host (`ENETUNREACH`, `EADDRNOTAVAIL`) is `NOT_TESTED_FROM_HERE` and not held against the target. (R3.1, R5.3)
- **Deadlines.** Every lens backend call has one deadline over connect, send and body; the 20 s hard deadline keeps finished sections. lens refuses to load a configuration whose backend timeouts do not stay below the hard deadline; the shipped configs use 15000 ms (dns, tls, http, email) and 2000 ms (ip). (R3.3)
- **Unknown verdicts and truncated streams** make a section errored; unknown verdicts are counted in `lens_unknown_verdict_total{section}`. (R3.4, R3.5)
- **HSTS and the HTTPS redirect** are scored once, in HTTP; the TLS section no longer repeats them. (R3.6)
- **One target policy** for every service: in addition to the earlier ranges, `0.0.0.0/8`, `240.0.0.0/4`, `198.18.0.0/15`, `192.0.0.0/24` and all of `2002::/16` are refused. A check whose resolved addresses include one in these ranges is no longer inspected by spectra (the HTTP section errors and the result is incomplete), also when the other addresses are public; tlsight skips only that address. (R3.7)
- **Email scoring** follows what beacon sends: a beacon timeout or a check that did not run makes the email section incomplete; Null MX is treated like no MX; a bucket with no configured category (for example no BIMI) is not applicable; cross-validation findings count in the bucket that owns their topic; for a domain that sends no mail, `reject_no_dkim` and `spf_mx_coverage` do not count. (R4.1–R4.4)
- **beacon:** an empty DKIM `p=` (revoked key) is information, not a failure; a domain that sends only revoked keys gets a warning, a parked domain none. A category with a passing check and an informational hint reads pass. A check that did not run is reported as skipped. beacon reports `sends_no_mail`. (R4.5)
- **IP reputation** comes from ifconfig-rs's flags (`is_spamhaus`, `is_c2`, `is_tor` → fail, `is_vpn` → warn); lens enriches up to four IPv4 and four IPv6 public addresses, sorted, and says "checked N of M addresses" when it sampled; a failed enrichment, or a domain with no A or AAAA record, makes the IP section errored and the result incomplete; a domain whose addresses are all non-public has the IP section not applicable. ifconfig-rs classifies `/json`, `/network` and `/range` with one function. (R5.2)
- **prism lints** count each record once, however many resolvers answered it, and emit identical lines once. (R5.1)
- **Advisories:** `time` 0.3.55, `lru` 0.18, mhost 0.12.0 with hickory 0.26.3, resvg 0.48 and fontdb 0.24; tlsight parses PEM with `rustls-pki-types`. Only the `paste` advisory remains ignored. Minimum Rust 1.88. (R2.1–R2.6)
- **prism:** `@system` is offered in the query UI and by `POST /api/parse` only when the server allows it.
- **prism nameserver checks:** a nameserver address that is not public is not queried; `ns_lame`, `ns_delegation` and authcompare report it as a warning ("nameserver address not public, not queried"), and the DNSSEC chain walk ends that branch with the same warning. Public nameservers keep their results. (R5.8)
- **ifconfig-rs:** `[rate_limit] exempt_cidrs` exempts configured peers (matched on the TCP peer) from the per-client limit.
- **Exports:** the Markdown exports of all five tools put messages and target values in code spans.

### Fixed

- prism `POST /api/parse` no longer fails on a cursor inside a multi-byte character.
- ifconfig-rs no longer caches a `dns=false` answer for later callers.
- lens's export carries the HTTP and email sections.

### Security

- Unchecked DNS queries to addresses from the checked domain's data, fixed in 0.23.0. prism sent raw DNS queries to the addresses of a domain's nameservers without the shared target policy:
  - the lame-delegation check, whose findings named the address, and the delegation-consistency check, which returned the NS names it received;
  - the authoritative comparison (`+auth`, `POST /api/authcompare`), which returned the records it received and listed each queried address;
  - the DNSSEC chain walk, which follows referral glue and returned the records it received.

  Missing glue was resolved through the system resolver, which inside a container answers service names with container addresses. A domain could therefore make prism send queries to a non-public address on port 53, return what that address answered, and reveal the container address behind a service name. Every such query now goes through one outbound policy, and a refused address is never queried. The NS checks and the authoritative comparison report it as "nameserver address not public, not queried"; the DNSSEC walk drops it and, when no public server remains for a level, ends that branch with the same warning. Found in an internal review. Every earlier release is affected; upgrade to 0.23.0.
- prism's `@system` resolver exposed container addresses. `@system` queries the resolvers in the host's `/etc/resolv.conf`; inside a container that is the container runtime's embedded resolver, which answers service names with container addresses. On netray.info `@system` is now off, and the query UI and `POST /api/parse` offer it only when the server allows it. The default stays `allow_system_resolvers = true`: a containerised deployment should set `[dns] allow_system_resolvers = false`.

## [0.22.2] - 2026-10-09

### Changed

- One outbound fetch policy: beacon's MTA-STS and BIMI fetches, spectra's redirect following, tlsight's live OCSP request and prism's MTA-STS policy fetch go through `netray_common::fetch`, which applies the shared target policy to every resolved address and every redirect hop.
- spectra sends each redirect hop to the port of its own URL, so a same-host redirect that changes scheme or port is followed correctly.
- Outbound fetch failures are reported with fixed detail texts.

### Added

- `SECURITY.md`: report vulnerabilities privately through GitHub.

### Security

- Server-side request forgery in five outbound fetches, fixed in 0.22.2. Each fetched a URL that a checked domain's DNS records, its certificate or its redirects control:
  - beacon's MTA-STS policy fetch;
  - beacon's BIMI logo fetch;
  - spectra's redirect following;
  - tlsight's live OCSP request;
  - prism's MTA-STS policy fetch.

  Before 0.22.2 such a URL could make the service connect to a non-public address, through DNS, an IP literal, userinfo, a bracketed IPv6 host, DNS rebinding or a redirect. Parts of the response came back in the result. Every such fetch now resolves through one checking resolver, connects only to the checked public addresses, and re-checks every redirect hop. A refused target is never contacted, and its response is never echoed. Found in an internal review. Every earlier release is affected; upgrade to 0.22.2.

## [0.22.1] - 2026-10-08

### Changed

- `just acceptance` runs against production only the Playwright project `prod`: health, ready, security headers and CORS, TLS handshakes, assets and MTA-STS, meta, docs. The full suite probes unknown paths and crawls the sitemap, which the production host's fail2ban bans. Run the full suite with `just acceptance-local`.

### Fixed

- `crates/ifconfig-rs/data/fetch.sh` follows upstream moves. The Googlebot ranges now come from `common-crawlers.json` and the GPTBot ranges from `gptbot.json`; the old URLs answer 301 and 403. Every download follows redirects, so a 3xx body is never saved as data.

### Security

- Known advisories are listed in `deny.toml` with their reason and deferred until the next release: `hickory-proto` RUSTSEC-2026-0118/0119, `time` -0009, `lru` -0253, and the unmaintained `rustybuzz` and `ttf-parser`.

## [0.22.0] - 2026-10-08

First release from the monorepo: the six services and the static site ship as one binary `netray` in one image, `ghcr.io/netray-info/netray`.

### Added

- `netray <lens|dns|tls|http|email|ip|site>`: one binary, one subcommand per service; `netray site` serves the static site, including the MTA-STS policy.
- `netray <service> --check-config <path>`: validates a config file and exits 0 or 1. It rejects unknown keys, a missing file and every value the service refuses at startup.
- Production config fixtures for all six services, plus contract goldens in `tests/fixtures/contracts/` that each backend writes and lens reads.
- `just acceptance-local`: runs the header and asset acceptance specs against a locally started stack on free ports.

### Changed

- Every config struct rejects unknown keys, and all services load config through one loader. The environment variable names are unchanged.
- The services emit the production security headers and CORS themselves: HSTS with `preload`, the tool CSP, Permissions-Policy, COOP, CORP and `Access-Control-Allow-Origin: *`. `X-Request-Id` is now on CORS preflights too.
- lens reads what the backends send. Email grades now come from beacon's real verdicts and reasons, and the receiving buckets are N/A only when beacon reports no MX. IP results show network type, organisation and location, because lens now requests ifconfig-rs `/json?ip=`.
- lens `/r/<id>` answers a 404 "expired or unknown" page for unknown, expired and malformed snapshot ids.

### Fixed

- No service trusts `CF-Connecting-IP` any more. In ifconfig-rs a client could use it to choose the address it was reported and rate-limited under.
- lens no longer waits for beacon's stream to close after the summary, and it no longer corrupts non-ASCII text split across network reads.
- A backend timeout is reported as a timeout, not as a generic backend error.
- `crates/ifconfig-rs/data/fetch.sh` fails closed. An HTTP error makes it exit non-zero instead of saving the error page, every data file is written atomically, and a run never removes another run's files.

### Removed

- Per-service images, releases and deploy workflows. GeoIP data is no longer part of any published image; the deployment provides it.
