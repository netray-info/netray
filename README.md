# netray

netray.info is a suite of network inspectors, live at [netray.info](https://netray.info): lens grades a domain's overall health (the apex), and five inspectors show the raw data behind it — IP enrichment (`ip.`), DNS (`dns.`), TLS certificates (`tls.`), HTTP headers (`http.`) and email security (`email.`). A static site carries the guide, the API docs, the tools page and the comparison page. This repository holds all of it: one Cargo and npm workspace, one binary `netray`.

## Layout

| Path | What |
|---|---|
| `crates/` | the six services (`lens`, `mhost-prism`, `tlsight`, `http`, `email`, `ip`), each a library with its SolidJS frontend; `common` (shared Rust); `netray` (the binary) |
| `packages/common-frontend` | shared SolidJS package, an npm workspace member |
| `site/` | the static site, served by `netray site` |
| `tests/acceptance` | Playwright acceptance suite against a deployed environment |
| `specs/rules` | engineering rules for the suite |

## Verbs

```sh
just adlc-setup          # once per checkout: npm workspaces and frontend builds
just check               # the gate, cargo-deny and the full Rust suite
just build               # frontends, then the release binary target/release/netray
just image               # the container image, local architecture
just acceptance          # Playwright suite against TEST_ENV (default: production)
just release X.Y.Z       # set the workspace version, changelog section, commit, tag; never pushes
```

`just --list` shows the rest (`ifconfig-data`, `test-ifconfig-data`, `tlsight-data`, `e2e`, …).

The image contains no data files. In production the ifconfig-rs config points its GeoIP and list paths at `data/…`, and the deployment fills `/netray/data` with `crates/ip/data/fetch.sh` (MaxMind licence required) plus the repository's `asn_patterns.toml`, and mounts it read-only; the bundled `ifconfig.example.toml` leaves those paths unset.

## Releases

`just release X.Y.Z` commits the version and changelog and tags `vX.Y.Z`; pushing the tag runs `.github/workflows/release.yml`, which builds the one arm64 image, smoke-tests every subcommand and pushes `ghcr.io/netray-info/netray:X.Y.Z`. It never overwrites a tag, never pushes `latest` and deploys nothing; deployment pins that tag in the infrastructure repository. `.github/workflows/ci.yml` runs `just check` on every push to `main` and every pull request.

## Running locally

`netray <service> <config>` starts one service (`just build` leaves the binary at `target/release/netray`); the subcommands are `lens`, `dns`, `tls`, `http`, `email` and `ip`. Each crate ships a dev config named `<name>.dev.toml`:

```sh
netray dns crates/mhost-prism/prism.dev.toml
netray site --root site          # the static site on 127.0.0.1:8080
```

`netray <service> --check-config <path>` validates a config file without starting the service: exit 0 and `config ok: <path>`, or exit 1 with the error (unknown key, missing file, a value startup would reject). It does not check host data files; `netray ip --check` covers ifconfig-rs's.

ifconfig-rs needs its runtime data first: `just ifconfig-data`.

## License and contributing

MIT, see `LICENSE`. See [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening an issue or a pull request.
