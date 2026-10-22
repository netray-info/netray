# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

**0.30.0 is the breaking release of V2: one process, one config, and every V1 API and app gone.** `netray serve` serves the apex, `dns.`, `tls.`, `http.`, `ip.`, `email.`, `lens.` and `mta-sts.` by `Host` from one `netray.toml`. It carries SDD Phases 2, 3, 4, 4a and 5 together; 0.27.0, 0.28.0 and 0.29.0 are not released. The operator's migration is the configuration table under Changed; `netray serve --check-config <path>` checks a migrated file.

**Grades change (scoring profile v4).** Three new graded checks: `tls.legacy_versions` (SSLv3, TLS 1.0 or TLS 1.1 accepted fails), `tls.weak_suites` (RC4, 3DES, DES, export, NULL or anonymous suites fail; CBC or static RSA suites warn) and `http.coop` (a missing, `unsafe-none` or unrecognised Cross-Origin-Opener-Policy warns). Every domain's grade may change once; the first v4 run of each domain is recorded as a new history entry whose diff shows the three checks as added by release. The diff's grade split cannot attribute their effect to the profile: the stored v3 run never measured them, so it re-scores under v4 to its old grade, and a letter change they cause appears on the domain step beside the added-by-release rows that name its cause. A server that resets or closes the connection after a probe's ClientHello instead of answering with an alert (Windows SChannel refuses disabled versions this way) has refused that version or suite group: the evidence answers `closed` or `closed (reset)`, and `tls.legacy_versions` and `tls.weak_suites` grade it like an alert. A timeout, a partial answer before the close or a failure before the ClientHello was sent leaves that probe unanswered; the check is then unmeasured unless another probe decides it.

**Verdicts change (no profile change).** A name below its zone apex (`www.example.com`, `docs.example.com`) is judged by its zone for `dns.ns`, `dns.ns_lame`, `dns.ns_delegation`, `dns.dnssec`, `dns.dnskey_algorithm` and `dns.dnssec_rollover`, and `dns.caa` reads the closest ancestor's CAA records (RFC 8659): subdomains no longer fail the NS checks "Not found", and a subdomain in a signed zone is no longer "DNSSEC not deployed". `dns.ttl` judges a record that a resolver's cache shows below 60 s on the authoritative TTL, so a cached remainder no longer warns. An SPF record whose only remark is informational (overlapping prefixes) passes instead of leaving the score. Email lookups whose UDP answer is truncated (large TXT records) are retried over TCP instead of being unmeasured.

### Removed

- **Subcommands.** `netray lens`, `dns`, `tls`, `http`, `email`, `ip` and `site` are gone; `serve` is the only subcommand, and the image's entrypoint is `netray serve`.
- **The V1 apps and the static site.** The six Solid apps (`crates/*/frontend`), `@netray-info/common-frontend`, `site/` and the per-service OpenAPI docs pages. The V2 pages, the inspectors at `/inspect/<tool>` and on their hosts, and `/api/v1/docs` replace them.
- **lens.** The crate, its snapshot store (`snapshots.db`), its V1 wire (`/api/check`), its SPA and its MCP tools. Runs, history, badges and OG cards are served by netray-web, netray-engine and netray-store.
- **The per-service config files and env prefixes.** `lens.toml`, `prism.toml`, `tlsight.toml`, `spectra.toml`, `beacon.toml` and `ifconfig.toml` and the prefixes `LENS_`, `NETRAY_DNS_`, `NETRAY_TLS_`, `NETRAY_HTTP_`, `NETRAY_EMAIL_`, `NETRAY_IP_`, `PRISM_`, `TLSIGHT_`, `SPECTRA_`, `BEACON_` and `IFCONFIG_`: `serve` refuses to start while any such variable is set, naming `NETRAY_*` and `NETRAY_CONFIG`.
- **The HTTP enrichment client.** The DNS, TLS, HTTP and email modules enrich addresses by calling the IP module in-process; without `[modules.ip]` enrichment is off.
- **V1 endpoints, with their successors:**

| Request | Answer |
|---|---|
| any path on `lens.netray.info` but `/health` and `/ready` | 301 to the same path and query on `netray.info` |
| `email.netray.info/?q=<domain> …` or `/?domain=<domain>` | 301 `https://netray.info/<domain>/email` (`q` wins; selectors are dropped) |
| any other path on `email.netray.info` | 301 `https://netray.info/` |
| `email.netray.info/inspect`, `/inspect/<domain>` | 410, successor `/api/v1/email/inspect` |
| `/api/*` on `dns.`, `tls.`, `http.`, `email.`, `ip.` | 410, successor `/api/v1` |
| `/docs`, `/docs/` on those hosts and on `netray.info` | 301 `https://netray.info/api/v1/docs` |
| `/api-docs/openapi.json` on those hosts and on `netray.info` | 410, successor `/api/v1/openapi.json` |
| `netray.info/api/check`, `/api/check/<domain>` | 410, successor `/api/v1/domains/{domain}/runs` |
| any other `netray.info/api/<x>` but `/api/v1` | 410, successor `/api/v1` |
| `netray.info/api/`, `/api/<name>.html` (V1 API docs) | 301 `https://netray.info/api/v1/docs` |
| `netray.info/guide/lens` | 301 `https://netray.info/guide/scoring` |
| `netray.info/r/<id>` for an ID that is no stored entry (every V1 snapshot link) | 410 page; `/r/<id>/og.png` an empty 410 |
| `ip.netray.info/range` | 410, successor `/api/v1/ip/range/{cidr}` |
| `ip.netray.info/ip/<x>` (neither a format nor `cidr`) | 410, successor `/api/v1/ip` |
| `ip.netray.info/asn/<n>` | 410, successor `/api/v1/asn/{n}` |
| `ip.netray.info/ipv4`, `/ipv6` (and `/<fmt>`) | 410, successor `/api/v1/ip/{address}` or `curl -4`/`curl -6` against the root |
| `ip.netray.info/batch`, `/diff`, `/meta`, `/api/meta` | 410, successors `/api/v1/ip/batch`, `/api/v1/ip/diff`, `/api/v1/meta` |
| any other GET or HEAD on `dns.`, `tls.`, `http.`, `ip.` but `/` and `/static/*` | 301 to the same path on `netray.info` |
| any unknown `Host`, or none | 404 on every path but `/health` and `/ready` |

  `ip.netray.info` keeps its command-line family (`/`, `/json`, `/yaml`, `/toml`, `/csv`, each field and `/<field>/<fmt>`, `/ip/cidr`, bare `/asn`, `?ip=`, `Accept` negotiation); a browser on `/` gets the IP inspector. `dns./?q=`, `tls./?h=`, `http./?url=` and `ip./?ip=` open their inspector prefilled; `dns./?r=<key>` ignores the V1 key. Old V1 API clients get these answers and nothing else.

### Added

- **TLS protocol and cipher probes.** Each address that completes a handshake is probed with hand-built ClientHello messages (SSLv3, TLS 1.0, TLS 1.1, TLS 1.2 when TLS 1.3 was negotiated, and the insecure, CBC and static-RSA suite groups; at most seven short connections, each bounded by the handshake timeout). Evidence: `tls.probes@<address>:<port>` per address and `tls.protocols` per port (TLS 1.0 to 1.3 enabled or disabled, suite groups accepted or refused, forward secrecy across suites).
- **V2 design system (`netray-web`, no route yet).**
  - `crates/web/tokens.json` is the token source. `build.rs` generates `tokens.css` (light, dark by preference or `[data-theme="dark"]`, phone values up to `layout.phoneMax`, `color-scheme`) and Rust constants. A test checks every status colour's contrast in both themes.
  - The ten R44 components, each an askama macro with its own stylesheet and view struct. The spectrum and trace keep the SDD's drawing rules.
  - The W5b logo, the favicon, the badge mark and the tagline partial.
  - A gate test refuses colours, radii, fonts and line widths outside the tokens, a component's classes outside its own files, and inline scripts or event handlers in templates.

- **TLS inspector (V2 pages), board G.** The inspector renders a port as board G draws it: an Addresses table (band, protocol, cipher, the leaf's fingerprint with same or differs), a Certificate section per distinct served chain (validity bar with the expiry window's amber zone, the facts with serial, SHA-1, policy, OCSP stapling and the live OCSP answer, crt.sh links per certificate, the chain with the inferred trust-store root and the chain-trusted verdict, copy per field), the Protocols and features grid from the protocol probes, a notices banner for warnings and skipped addresses, and the related-records summaries. Several ports get one tab and one panel each with that port's band and checks. The form offers a port chooser with the presets HTTPS, E-Mail and All common. A banner names what changed when a host's certificate differs from the viewer's last run.
- **Site navigation (V2 pages).** The header's Inspectors entry is a menu of the five inspectors (DNS, TLS, HTTP, Email, IP) with "All tools →"; it opens without JavaScript and, on phones, inside the menu sheet. `/tools` is the inspector index again (five cards with a quick form, featured guides); `/tools/` answers 301 to `/tools`. Every page carries the full footer: tagline, the version line, and the link groups Inspectors, Site and About. `/privacy` renders in the site frame. The home page explains the grades ("What the grades mean") and offers custom DKIM selectors; the email tab and the email evidence views offer "Check custom DKIM selectors". An internationalised domain shows its Unicode form beside the A-label in headings and titles.

### Changed

- **One config, `netray.toml`.** Tables `[server]` (`bind`, `metrics_bind`, `trusted_proxies`, `apex`), `[engine]`, `[store]`, `[admission]`, `[api]` with `[api.<inspector>]`, `[inspect.<tool>]`, `[badges]`, `[og_cards]`, `[telemetry]` and `[modules.<name>]` with each module's `deadline_ms`; every table refuses unknown keys. Variables use the prefix `NETRAY_` (`NETRAY_SERVER__BIND`), and `NETRAY_CONFIG` names the file. `crates/netray/netray.example.toml` lists every key at its default. A key of an old file is refused at load and by `--check-config`, naming where its value lives now:

| Old key path | Old file | In 0.30.0 |
|---|---|---|
| `backends.resolve_timeout_ms` | lens | `[engine] resolve_timeout_ms` |
| `backends.<m>.timeout_ms` | lens | `[modules.<m>] deadline_ms` |
| `backends.<m>.url` | lens | removed |
| `backends.ip.url` | prism, tlsight | removed: enrichment calls the in-process ip module |
| `backends.ip.timeout_ms` | prism, tlsight | removed: enrichment calls the in-process ip module |
| `backends.ip.max_concurrent` | prism, tlsight | removed: enrichment calls the in-process ip module |
| `backends.ip.cache_ttl_secs` | prism, tlsight | removed: enrichment calls the in-process ip module |
| `backends.ip.cache_capacity` | prism, tlsight | removed: enrichment calls the in-process ip module |
| `backends.ip_url` | beacon | removed: enrichment calls the in-process ip module |
| `backends.timeout_ms` | beacon | removed: enrichment calls the in-process ip module |
| `modules.dns.backends.ip` | lens | removed: enrichment calls the in-process ip module |
| `modules.tls.backends.ip` | lens | removed: enrichment calls the in-process ip module |
| `modules.http.enrichment` | lens | removed: enrichment calls the in-process ip module |
| `modules.email.backends.ip_url` | lens | removed: enrichment calls the in-process ip module |
| `cache.enabled` | lens | `[admission] cache_enabled` |
| `cache.enabled` | ifconfig | removed |
| `cache.ttl_seconds` | lens | `[admission] cache_window_seconds` |
| `cache.ttl_secs` | ifconfig | removed |
| `cache.max_entries` | ifconfig | removed |
| `rate_limit.per_ip_per_minute` | lens | `[admission] per_ip_per_minute` |
| `rate_limit.per_ip_burst` | lens | `[admission] per_ip_burst` |
| `rate_limit.global_per_minute` | lens | `[admission] global_per_minute` |
| `rate_limit.global_burst` | lens | `[admission] global_burst` |
| `rate_limit.per_ip_per_minute` | ifconfig | `[api.ip] per_ip_per_minute` |
| `rate_limit.per_ip_burst` | ifconfig | `[api.ip] per_ip_burst` |
| `rate_limit.per_target_per_minute` | ifconfig | `[api.ip] per_target_per_minute` |
| `rate_limit.per_target_burst` | ifconfig | `[api.ip] per_target_burst` |
| `rate_limit.exempt_cidrs` | ifconfig | `[api.ip] exempt_cidrs` |
| `rate_limit.per_ip` | beacon | removed |
| `scoring.profile_path` | lens | `[engine] profile_path` |
| `history.enabled` | lens | `[store] enabled` |
| `history.db_path` | lens | `[store] db_path` |
| `snapshots` | lens | removed |
| `site` | lens | removed |
| `ecosystem` | lens, prism, tlsight, beacon | removed |
| `site_name` | prism, tlsight, ifconfig | removed |
| `meta` | spectra | removed |
| `base_url` | ifconfig | removed |
| `project_name` | ifconfig | removed |
| `project_version` | ifconfig | removed |
| `inspect.request_timeout_secs` | spectra | `[modules.http.inspect].request_timeout_secs` |
| `inspect.total_timeout_secs` | spectra | `[modules.http.inspect].total_timeout_secs` |
| `inspect.max_redirects` | spectra | `[modules.http.inspect].max_redirects` |
| `inspect.user_agent` | spectra | `[modules.http.inspect].user_agent` |
| `server.admin_bind` | ifconfig | `[server] metrics_bind` |
| `server.admin_token` | ifconfig | removed |
| `circuit_breaker` | prism | `[modules.dns].circuit_breaker` |
| `validation` | tlsight | `[modules.tls].validation` |
| `quality` | tlsight | `[modules.tls].quality` |
| `enrichment` | spectra | `[modules.http].enrichment` |
| `dnsbl` | beacon | `[modules.email].dnsbl` |
| `http` | beacon | `[modules.email].http` |
| `dkim` | beacon | `[modules.email].dkim` |
| `limits` | prism, tlsight, spectra | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `dns` | prism, tlsight, beacon | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `trace` | prism | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `batch` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `filtered_headers` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `internal_mode` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `watch_data_files` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `dns_base_url` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `tls_base_url` | ifconfig | no single destination: the key moved into the module table of the service that read it (`[modules.dns]`, `[modules.tls]`, `[modules.http]`, `[modules.email]` or `[api.ip]`), or went with its routes |
| `user_agent_regexes` | ifconfig | `[modules.ip].user_agent_regexes` |
| `geoip_city_db` | ifconfig | `[modules.ip].geoip_city_db` |
| `geoip_asn_db` | ifconfig | `[modules.ip].geoip_asn_db` |
| `tor_exit_nodes` | ifconfig | `[modules.ip].tor_exit_nodes` |
| `cloud_provider_ranges` | ifconfig | `[modules.ip].cloud_provider_ranges` |
| `feodo_botnet_ips` | ifconfig | `[modules.ip].feodo_botnet_ips` |
| `cins_army_ips` | ifconfig | `[modules.ip].cins_army_ips` |
| `vpn_ranges` | ifconfig | `[modules.ip].vpn_ranges` |
| `datacenter_ranges` | ifconfig | `[modules.ip].datacenter_ranges` |
| `bot_ranges` | ifconfig | `[modules.ip].bot_ranges` |
| `spamhaus_drop` | ifconfig | `[modules.ip].spamhaus_drop` |
| `asn_patterns` | ifconfig | `[modules.ip].asn_patterns` |
| `asn_info` | ifconfig | `[modules.ip].asn_info` |

- **One metrics listener.** `[server] metrics_bind` serves `/metrics` without a token and replaces ifconfig's `admin_bind`; `admin_token` is removed. `/health` and `/ready` answer on any `Host`.
- **Metrics are named `netray_*`.** The SDD §9.2 set (`netray_runs_total{outcome,trigger}`, `netray_section_outcomes_total`, `netray_runs_with_predecessor_total`, `netray_run_interval_seconds`, `netray_fixes_observed_total`, `netray_admission_rejections_total{reason}`, `netray_history_entries`, `netray_history_domains`, `netray_api_requests_total{endpoint}`, `netray_entry_views_total`, `netray_badge_requests_total`, the run and time-to-result histograms) is registered at startup. Renamed: `lens_admission_rejections_total` → `netray_admission_rejections_total`, `lens_runs_in_flight` → `netray_runs_in_flight`, `lens_history_record_errors_total` → `netray_history_record_errors_total`, `lens_rate_limit_hits_total` → `netray_rate_limit_hits_total{scope}` (DNS's own limiter: `netray_dns_rate_limit_hits_total`), `lens_badge_*` → `netray_badge_*`, the modules' series `prism_*` → `netray_dns_*` (label `endpoint` → `operation`), `beacon_*` → `netray_email_*` (label `target` → `fetch`), `tlsight_*` → `netray_tls_*`, `spectra_*` → `netray_http_*`, `ifconfig_*` → `netray_ip_*`, and the HTTP request series under the prefix `netray`. Ended without successor: `lens_check_requests_total` (counted by `netray_runs_total` and `netray_api_requests_total`) and the per-service `*_http_requests_total` of the deleted routers. Dashboards and alerts move without continuity.

- **Section labels and domain input.** A section whose every check is unmeasured (all HTTP probes timed out) is labelled `incomplete` with the reason "every check unmeasured: …" instead of `measured`; its checks stay listed. A single-label name is refused as an invalid domain, "a domain needs at least two labels", by `POST /api/v1/domains/{domain}/runs`, the domain reads of API v1, `/og/<name>.png` and `/badge/<name>.svg`, and starts no run; the DNS inspector still takes single-label names.
- **DNS inspector.** An address query (`192.0.2.1`, `2001:db8::1`) looks up PTR at its reverse name (`1.2.0.192.in-addr.arpa`) instead of the literal address; `@system` queries no longer end empty when the system has several nameservers.
- **Links are apex-relative.** Pages link the inspectors at `/inspect/<tool>`, not at `<tool>.netray.info`; canonical, Open Graph, JSON-LD, the sitemap and robots (now with its `Sitemap:` line under lens too) build absolute URLs from one origin. The sitemap lists `/tools`, `/api/v1/docs` and the five inspector pages instead of the subdomains. The domain-security-audit guide's API samples read the V2 API (`/api/v1/dns/check`, `/api/v1/tls/inspect`, `/api/v1/email/inspect`, `/api/v1/ip/<address>`) and the run answer's nested `run`.
- **A DKIM selector list that breaks the rule** answers its own 400 page, "Not a list of DKIM selectors", naming the rule and the refused part; a selector run's run line names the selectors and that the run is not recorded. `/inspect/ip?ip=` (blank) looks up the visitor's own address.
- **TLS probes are charged and bounded.** Probe connections (seven per probed address and port) are charged to the TLS rate limiters beside the handshakes. When they do not fit the burst, fewer addresses are probed, one kept before further handshakes, with the warning `rate limit: probing K of N addresses to stay within budget`; probes never cause a refusal. At the default burst of 10 on one port a host with more than three addresses is inspected on three. The handshake permit is released after the handshake, and probes wait for their own permits within the handshake timeout, so a hanging server no longer stalls queued inspections.

## [0.26.0] - 2026-10-14

**Grades change.** A check that could not be measured no longer reads as passed, failed or not applicable. Timeouts, SERVFAIL or REFUSED answers, and redirects the fetch policy refuses all count as `unmeasured`. A weighted unmeasured check makes its section incomplete, and the run gets no letter unless a hard-fail check failed, which still grades F. A domain whose A and AAAA lookups both get a definite NXDOMAIN or NODATA now gets a letter from DNS and email, with IP, HTTP and TLS not applicable ("no A or AAAA record"); before, such a run was incomplete.

### Changed

- **lens wire.**
  - The check verdict `"unmeasured"` is new; the frontend shows it as "could not check".
  - A section that answered but holds a weighted unmeasured check reports status `"incomplete"`, in its event, in `summary.sections` and in `summary.overall`. `"error"` stays reserved for a backend that failed.
- **When a check fails or is unmeasured.** A check fails only when the target answered wrongly: a refused connection, a reset, a TLS alert, an HTTP error status, NXDOMAIN or NODATA.
  - TLS: `tls_reachable` stays pass when any address completed a handshake. It is unmeasured when every tested address timed out, and the checks that need a handshake follow it.
  - HTTP: a refused, reset or TLS-failed HTTPS probe makes the ten response checks fail with "no HTTPS response". That includes a redirect target that refuses. CORS follows its own probe.
  - DNS: `ns_lame` and `ns_delegation` judge the nameservers that answered and name the silent ones. They are unmeasured only when no nameserver answered.
  - Email: `dnsbl` passes when at least one zone answered without a listing, names the zones that did not answer, and is unmeasured only when none answered. An MTA-STS policy fetch or a BIMI logo fetch is unmeasured only on a timeout.
- **Backend responses (additive; each field is absent when empty).**
  - tlsight gives a timed-out address the code `HANDSHAKE_TIMEOUT` and adds `dns.lookup_errors`.
  - spectra adds `error: {kind, message}` to the HTTPS, port-80 and CORS probes.
  - beacon gives a category whose answer was not obtained the sub-check `unmeasured`, and a failed side lookup the sub-check `lookup_failed`. Any unmeasured category makes beacon's own grade `skipped`.
- **Evidence on the lens wire (additive).** Every section event, sync JSON section and MCP section carries `evidence`: typed blocks of what the module measured (DNS records, certificate chains and handshakes per address, redirect chains, the header table, CAA/TLSA records, MX, SPF, DMARC, DKIM, DNSBL and FCrDNS tables, IP addresses with reputation flags). Values are tagged `{"t", "v"}`. Every check item carries `evidence`, the ids of the blocks it relies on, and the summary carries `evidence_schema: 1`. Existing fields keep their values; `ip.addresses`, the `http` server fields and `email.grade` are now derived from the evidence, and `headline` from the check verdicts and the evidence. On error inputs a headline symbol follows the check's verdict, and `http.status_code`/`http_version` are absent when the HTTPS probe got no response.
- **beacon** sends one additional SSE message `{"type":"evidence","blocks":[...]}` before `summary`; the V1 email page ignores it.
- **lens configuration.** `[backends] resolve_timeout_ms` must also stay below `[backends.http] timeout_ms`, because the HTTP section now waits for the shared lookup inside its own window.
- mhost 0.12.1.

### Fixed

The three known issues of 0.25.0:
- `email.mx` passes for a working MX whose sub-checks are only notes (one MX host with several addresses, no IPv6).
- An `http.https_redirect` port-80 timeout is unmeasured instead of failing.
- A DNSBL zone that refuses or does not answer no longer turns the check not applicable.

## [0.25.0] - 2026-10-11

**Grades change.** lens now scores with the check catalogue v3 and scoring profile v3: 54 checks instead of 42, with a check's weight split out of the old roll-ups. Measured against 0.24.0 at the same moment, netray.info moved from A 94.1 to A+ 97.7. Three other domains moved from B 86.6 to A 91.0, C 72.6 to C 75.5 and C 73.0 to C 73.8. Most of the shift comes from the HTTP header checks: each header is scored on its own, so one missing header no longer costs the whole `security_headers` weight. A warn now earns exactly half a check's weight.

### Changed

- **New check names in lens's output.** The JSON, SSE and snapshot shapes stay; each section's check list holds the v3 names.
  - HTTP: `csp`, `x_frame_options`, `x_content_type_options`, `referrer_policy` and `permissions_policy` replace `security_headers`; `deprecated_headers`, `info_leakage` and `caching` replace `hygiene`, which 0.24.0 counted only against a site (a healthy site's `hygiene` read `skip`).
  - Email: `mx`, `spf`, `dkim`, `dmarc`, `mta_sts`, `tls_rpt`, `bimi`, `dane`, `dnsbl` and `fcrdns` replace `email_authentication`, `email_infrastructure`, `email_transport` and `email_brand_policy`.
  - DNS no longer reports `infrastructure`.
  - TLS reports all 21 checks in every run. The certificate, protocol, cipher, CT and ECH checks are measured on every address that completed a handshake; the worst result counts and names the address. `tls_reachable`, `consistency`, `alpn_consistency`, `caa_compliant`, `dane_valid` and `ocsp_stapled` are judged once per run. A check that does not apply is `not_applicable` with its reason, such as "no TLS handshake" in TLS, "no MX" in email or "not reported" where a module had no result.
  - Titles, fix hints, owners and guide links come from one catalogue (`crates/engine/catalogue/checks.toml`).
- **Scoring.** A snapshot's `earned` is a decimal (a warn earns exactly half); snapshots taken by 0.24.0 still render with their old check names. `tls.hostname_match` is a fourth hard fail beside `chain_trusted`, `not_expired` and `tls_reachable`: a certificate that does not cover the checked name grades F.
- **lens configuration.** `[backends] resolve_timeout_ms` must stay below `[backends.tls] timeout_ms` as well as `[backends.ip] timeout_ms`, because the TLS section now waits for the shared lookup inside its own window; startup refuses it otherwise. `[scoring] profile_path` refuses a check that is not in the catalogue, naming it. `[modules.dns.backends.ip]` is accepted and has no effect: the DNS section no longer enriches addresses.

### Fixed

- **`http.cors`** counts an `Access-Control-Allow-Origin` as reflected only when it echoes the probe's `Origin` exactly or is `null`; together with `Access-Control-Allow-Credentials: true` that fails. A site that names its own trusted origin passes, with or without credentials; `*` still warns.
- **`tls.caa_compliant`** fails on a critical flag only for an unknown CAA tag. For a wildcard certificate, `issuewild` records decide over `issue` (RFC 8659 §4.3).
- **`tls.strong_signature`** ignores a root's self-signature. MD5, MD2, ECDSA-SHA1, DSA-SHA1 and the OIW SHA-1 identifier fail; an algorithm netray does not recognise warns instead of passing.
- **`tls.chain_trusted`** checks the chain against the trust store without the hostname, so a name mismatch is reported only by `tls.hostname_match`, and an IP address target gets a trust verdict.
- **`tls.chain_complete`** fails an ordered chain that reaches neither a trust anchor nor a self-signed certificate, such as a leaf sent without its intermediate; `tls.aia_reachability` follows from it.
- **tls API.** `validation.chain_trusted` no longer depends on the hostname, `validation.weakest_signature` excludes root self-signatures, and the new `validation.ends_at_anchor` says whether the served chain reaches an anchor or a self-signed certificate. A trust store reloaded on SIGHUP now also applies to inspections.

### Known issues

- **`email.mx` is not applicable for some working MX setups.** When every MX sub-check is informational (for example, no MX host has an AAAA record), the check drops out of the grade instead of passing.
- **`http.https_redirect` fails when the port-80 probe times out**, even if the site redirects.
- **`email.dnsbl` is not applicable when a blocklist zone refuses the query or times out** and no zone lists the address, instead of being reported as not measured.

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
