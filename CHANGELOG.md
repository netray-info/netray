# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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
