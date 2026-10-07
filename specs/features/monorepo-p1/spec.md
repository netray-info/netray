# Spec: one workspace, one binary

The netray.info suite — six services (ifconfig-rs, mhost-prism, tlsight, spectra,
beacon, lens), the shared crate and the shared frontend package — now lives in this
repository with its full history (`crates/`, `packages/common-frontend/`), together
with the static site (`site/`), the production acceptance suite
(`tests/acceptance/`) and the engineering rules (`specs/rules/`). Each part still
builds on its own, the way it did as a separate repository. This spec makes the
repository build, test and run as one.

Scope: the build (Cargo and npm workspaces, one binary, the verbs) and the removal
of what only made sense for separate repositories or for self-hosting. CI, the
release workflow and the image published to GHCR are a later spec; the per-crate
`.github/` directories stay inert until then.

## Decisions

| # | Decision |
|---|---|
| D1 | One binary `netray` with one subcommand per service (`lens`, `dns`, `tls`, `http`, `email`, `ip`) and `site`. Production keeps one process per service, so each subcommand starts exactly one service. |
| D2 | Directory and crate names keep the codenames. `mhost-prism`'s crate stays `prism`. |
| D3 | `netray-common` and `@netray-info/common-frontend` are workspace members only; nothing is published to crates.io or GitHub Packages any more. |
| D4 | No self-host support. "Open source" stays in the copy; "self-hostable" and deploy instructions for third parties go. |
| D5 | `netray site` replaces the nginx container that served `site/`, with the same routing and the headers listed in requirement 7. |

## Requirements

1. The root `Cargo.toml` is a workspace whose members are every crate under `crates/`. Every crate depends on `crates/common` by path. Versions shared by two or more crates live in `[workspace.dependencies]`. There is exactly one `Cargo.lock` (root) and one `rust-toolchain.toml` (root); no crate keeps its own. Edition 2024. Every crate inherits `[workspace.package] version`, so every service reports the one suite version (in `/api/meta`, OpenAPI `info.version`, User-Agents, ETags); it starts at `0.22.0`, above every per-service version released so far.
2. The workspace resolves one version of `reqwest` and one of `axum-extra`.
3. The root `package.json` declares npm workspaces for the six service frontends (`crates/*/frontend`) and `packages/common-frontend`. Each frontend depends on `@netray-info/common-frontend` through the workspace. No `.npmrc` names `npm.pkg.github.com` or an `_authToken`. There is exactly one `package-lock.json` for the workspaces (root); `tests/acceptance/` keeps its own. All workspace packages use one major version of `vite` and one of `typescript`.
4. Each service crate is a library with an async entry point that takes the config path and runs the service until shutdown. No service crate defines a binary target or a `main.rs`.
5. `crates/netray` builds the binary `netray`. `netray <service> [config-path]` starts exactly that service. The config path resolves as before (argument, then the service's `*_CONFIG` variable, then its default), and the service keeps its config keys and env prefix, telemetry defaults, metrics names and log target. `netray --help` lists the seven subcommands.
6. Every service subcommand answers `GET /` and `GET /health` with 200, and an unknown path exactly as the separate binary did (each serves its SPA's `index.html` with 200).
7. `netray site [--bind ADDR] [--root DIR]` serves `site/` (default root `site`, default bind `127.0.0.1:8080`):
   - routing: `/<path>` serves `<path>`, then `<path>.html`, then `<path>/index.html`; a path ending in `/` only serves `<path>/index.html`; anything else is 404 with the body of `site/404.html` (no SPA fallback, so `/` is 404 because the apex belongs to lens); `/404.html` and `/50x.html` are 404 (error pages are internal); paths with a hidden segment (`/.git`, `/.env`) are 404, except `/.well-known/mta-sts.txt`;
   - when the `Host` header starts with `mta-sts.`, only `/.well-known/mta-sts.txt` is served, every other path is 404; the policy is always `text/plain` with `Cache-Control: no-cache`, on any host;
   - headers on every response: `Content-Security-Policy: default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; font-src 'self'; img-src 'self' data:; connect-src 'self' https://stats.uptimerobot.com; frame-ancestors 'none'; base-uri 'self'; object-src 'none'; form-action 'self' https://*.netray.info`, `Cross-Origin-Resource-Policy: same-origin`, `Cross-Origin-Opener-Policy: same-origin`, `Strict-Transport-Security: max-age=31536000; includeSubDomains; preload`, `X-Frame-Options: DENY`, `X-Content-Type-Options: nosniff`, `Referrer-Policy: strict-origin-when-cross-origin`, `Permissions-Policy: camera=(), microphone=(), geolocation=(), payment=()`;
   - `Cache-Control: public, max-age=604800, immutable` for `.css`, `.svg`, `.ico`, `.xml`, `.txt` (except the MTA-STS policy), `public, max-age=3600` for every other file served; 404 responses carry no `Cache-Control` (as nginx served them).
8. The root `justfile` holds the verbs, and no crate or package keeps its own `justfile` or `Makefile`:
   - `check`: fmt, clippy, every Rust and frontend test, the site validation, and the subcommand smoke test;
   - `adlc-verify`: the gate — offline, a subset of `check`;
   - `build`: every frontend, then the binary in release mode;
   - `image`: builds the one image from the root `Dockerfile` for the local architecture;
   - `acceptance`: the Playwright suite against `TEST_ENV`;
   - `release X.Y.Z`: sets the workspace version, adds the changelog section, commits and tags `vX.Y.Z` (no push).
9. `adlc.toml` declares `adlc-verify` as the contract and has no `no-ci` exception.
10. One `CONTRIBUTING.md` at the root (maintainer-led; issues welcome; pull requests by arrangement; no self-host support). No crate or package keeps a `CONTRIBUTING.md`, an `AGENTS.md`, or a DCO workflow (the root `AGENTS.md` holds the adlc block).
11. No product copy or document promises self-hosting: the lens landing page and config defaults, tlsight's value proposition, `site/tools`, the compare page, and every README. No build input (`Cargo.toml`, `package.json`, `.npmrc`, `justfile`, `Dockerfile`, `build.rs`) references crates.io publishing, GitHub Packages, `make -C`, or a sibling repository path.

## Phase 1 — Workspace and verbs

**Depends on:** none
**Requirements:** 1, 2, 3, 8, 9

### Test Scenarios

- GIVEN a clean clone WHEN `cargo metadata` runs at the root THEN the workspace members are exactly the crates under `crates/`, and `netray-common` resolves to `crates/common`.
- GIVEN the tree WHEN searched for `Cargo.lock`, `rust-toolchain.toml`, `justfile` and `Makefile` outside the root THEN none is found.
- GIVEN the workspace WHEN `cargo tree --workspace -d -e normal` runs THEN neither `reqwest` nor `axum-extra` appears twice.
- GIVEN the tree WHEN every `.npmrc` and `package.json` is read THEN none names `npm.pkg.github.com` or `_authToken`, and every frontend's `@netray-info/common-frontend` dependency is satisfied by `packages/common-frontend` (`npm ls` shows it linked).
- GIVEN the workspace packages WHEN their `vite` and `typescript` ranges are read THEN each resolves to a single major.
- GIVEN the root WHEN `just --summary` runs THEN it lists `check`, `adlc-verify`, `build`, `image`, `acceptance` and `release`.
- GIVEN `adlc.toml` WHEN read THEN `[contract] recipe = "adlc-verify"` and no `no-ci` key.
- GIVEN the workspace WHEN `cargo test --workspace` runs THEN at least 1,650 `#[test]`/`#[tokio::test]` functions exist in the tree and all pass; WHEN the frontend tests run THEN at least 171 `it(`/`test(` cases exist and all pass.

## Phase 2 — One binary

**Depends on:** 1
**Requirements:** 4, 5, 6, 7

### Test Scenarios

- GIVEN the service crates WHEN their `Cargo.toml` and `src/` are read THEN none has a `[[bin]]` or `src/main.rs`.
- GIVEN the built `netray` WHEN `netray --help` runs THEN it lists `lens`, `dns`, `tls`, `http`, `email`, `ip`, `site`.
- GIVEN each service's dev config with a free port WHEN `netray <service>` starts THEN `GET /` and `GET /health` return 200 and `GET /does-not-exist` returns 200 with the SPA's `index.html`.
- GIVEN a service started with a config path argument and a different `*_CONFIG` variable WHEN it starts THEN it uses the argument (same precedence as before).
- GIVEN a service's metrics address WHEN scraped after one request THEN the metric names carry the same prefix as the separate binary's.
- GIVEN `netray site` on the repository's `site/` WHEN `GET /guide/` THEN 200; WHEN `GET /guide/dnssec` THEN 200 (extensionless); WHEN `GET /` or `GET /does-not-exist` THEN 404 with the `404.html` body; WHEN `GET /.git/config` THEN 404.
- GIVEN `netray site` WHEN `GET /.well-known/mta-sts.txt` with `Host: mta-sts.example.com` THEN 200, `text/plain`, `Cache-Control: no-cache`, body equal to `site/.well-known/mta-sts.txt`; WHEN `GET /guide/` with that host THEN 404.
- GIVEN `netray site` WHEN any response is read THEN it carries every header requirement 7 lists, with those values.

## Phase 3 — One repository's documents

**Depends on:** 1
**Requirements:** 10, 11

### Test Scenarios

- GIVEN the tree WHEN searched THEN exactly one `CONTRIBUTING.md` and one `AGENTS.md` exist (both at the root), and no `dco.yml`.
- GIVEN the tree outside historical records (`specs/`, `docs/done/`, `CHANGELOG.md` files) and the root `CONTRIBUTING.md` (which states that self-hosting is unsupported) WHEN searched case-insensitively for `self-host` or "run your own instance" THEN nothing is found.
- GIVEN the build inputs WHEN searched for `cargo publish`, `npm.pkg.github.com`, `make -C`, and `../netray-common` THEN nothing is found.

## Open decisions

None.
