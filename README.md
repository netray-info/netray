# netray

netray.info is a suite of network inspectors, live at [netray.info](https://netray.info): lens grades a domain's overall health (the apex), and five inspectors show the raw data behind it — IP enrichment (`ip.`), DNS (`dns.`), TLS certificates (`tls.`), HTTP headers (`http.`) and email security (`email.`). A static site carries the guide, the API docs, the tools page and the comparison page. This repository holds all of it: one Cargo and npm workspace, one binary `netray`.

## Layout

| Path | What |
|---|---|
| `crates/` | the six services (`lens`, `mhost-prism`, `tlsight`, `spectra`, `beacon`, `ifconfig-rs`), each a library with its SolidJS frontend; `common` (shared Rust); `netray` (the binary) |
| `packages/common-frontend` | shared SolidJS package, an npm workspace member |
| `site/` | the static site, served by `netray site` |
| `tests/acceptance` | Playwright acceptance suite against a deployed environment |
| `specs/rules` | engineering rules for the suite |

## Verbs

```sh
just adlc-setup          # once per checkout: npm workspaces and frontend builds
just check               # the gate plus the full Rust suite
just build               # frontends, then the release binary target/release/netray
just image               # the container image, local architecture
just acceptance          # Playwright suite against TEST_ENV (default: production)
just release X.Y.Z       # set the workspace version, changelog section, commit, tag; never pushes
```

`just --list` shows the rest (`ifconfig-data`, `test-ifconfig-data`, `tlsight-data`, `e2e`, …).

## Running locally

`netray <service> <config>` starts one service (`just build` leaves the binary at `target/release/netray`); the subcommands are `lens`, `dns`, `tls`, `http`, `email` and `ip`. Each crate ships a dev config named `<name>.dev.toml`:

```sh
netray dns crates/mhost-prism/prism.dev.toml
netray site --root site          # the static site on 127.0.0.1:8080
```

ifconfig-rs needs its runtime data first: `just ifconfig-data`.

## License and contributing

MIT, see `LICENSE`. See [`CONTRIBUTING.md`](CONTRIBUTING.md) before opening an issue or a pull request.
