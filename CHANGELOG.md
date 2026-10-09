# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
