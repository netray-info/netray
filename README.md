# netray

netray.info is a suite of network inspectors, live at [netray.info](https://netray.info): the apex grades a domain's overall health and keeps its history, and five inspectors show the raw data behind it — IP enrichment (`ip.`), DNS (`dns.`), TLS certificates (`tls.`), HTTP headers (`http.`) and email security (`email.`), each also at `/inspect/<tool>` on the apex. The guides, the comparison page and API v1 (`/api/v1`, docs at `/api/v1/docs`) are served by the same process. This repository holds all of it: one Cargo and npm workspace, one binary `netray`, one subcommand `netray serve`.

## Layout

| Path | What |
|---|---|
| `crates/web` | `netray-web`: host routing, pages, templates, islands (`crates/web/islands`, the npm workspace), API v1, admission, badge, OG card |
| `crates/engine`, `crates/model`, `crates/store` | the run orchestrator and scoring, the check model, the history store |
| `crates/{dns,tls,http,email,ip}` | the check modules and their inspector functions |
| `crates/common` | shared Rust: config loader, middleware, outbound fetch policy, rate limiting |
| `crates/netray` | the binary: `netray serve`, `netray.example.toml`, `netray.dev.toml` |
| `tests/acceptance` | Playwright acceptance suite against a deployed environment |
| `specs/rules` | engineering rules for the suite |

## Verbs

```sh
just adlc-setup          # once per checkout: npm workspace and the islands bundle
just check               # the gate, cargo-deny and the full Rust suite
just build               # the islands bundle, then the release binary target/release/netray
just image               # the container image, local architecture
just acceptance          # Playwright suite against TEST_ENV (default: production)
just release X.Y.Z       # set the workspace version, changelog section, commit, tag; never pushes
```

`just --list` shows the rest (`acceptance-local`, `design-check`, `ifconfig-data`, `test-ifconfig-data`, `tlsight-data`, …).

The image contains no data files. In production `[modules.ip]` points its GeoIP and list paths at `/netray/data`, which the deployment fills with `crates/ip/data/fetch.sh` (MaxMind licence required) plus the repository's `asn_patterns.toml` and mounts read-only; the bundled `netray.example.toml` leaves `[modules.ip]` out, so the IP section and IP enrichment are off.

## Releases

`just release X.Y.Z` commits the version and changelog and tags `vX.Y.Z`; pushing the tag runs `.github/workflows/release.yml`, which builds the one arm64 image, smoke-tests `netray serve` on the apex and the inspector hosts and pushes `ghcr.io/netray-info/netray:X.Y.Z`. It never overwrites a tag, never pushes `latest` and deploys nothing; deployment pins that tag in the infrastructure repository. `.github/workflows/ci.yml` runs `just check` on every push to `main` and every pull request.

## Running locally

`netray serve [<config>]` starts the product (`just build` leaves the binary at `target/release/netray`); the config file is the argument, else `NETRAY_CONFIG`, else defaults. Variables override keys with the prefix `NETRAY_` (`NETRAY_SERVER__BIND=127.0.0.1:8082`). Hosts are told apart by `Host`; set `[server] apex = "localhost"` to reach the apex at `http://localhost:<port>` and an inspector at `http://dns.localhost:<port>`:

```sh
NETRAY_SERVER__APEX=localhost netray serve crates/netray/netray.dev.toml
```

`netray serve --check-config <path>` validates a config file without starting: exit 0 and `config ok: <path>`, or exit 1 with the error (unknown key, a key of an old per-service file with its new place, a value startup would reject). It opens no file the config names: the history store and the GeoIP data are checked at startup.

The IP module needs its runtime data first: `just ifconfig-data`.

## License and contributing

MIT, see `LICENSE`. See [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening an issue or a pull request.
