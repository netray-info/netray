# Plan: monorepo-p1

## Phase 1 — Workspace and verbs

## Groups

- G1: C1, C2, C6, C8 — Cargo workspace (root `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`; crate manifests on `workspace = true`; one reqwest incl. the otlp exporter path; lens on reqwest 0.13 / axum-extra 0.12).
- G2: C3, C9, C10 — npm workspaces (root `package.json` + lock; no `.npmrc`/per-package locks; common-frontend private; beacon on vite 8 / typescript 6).
- G3: C4, C7, C11, C13 — verbs (root `justfile`, `Dockerfile`, `.dockerignore`, `CHANGELOG.md`; per-crate justfiles and Makefiles become root recipes). Depends on G1 and G2.
- C5, C12: already implemented.

## Plan

### G1
1. Root `Cargo.toml`: `[workspace] members = ["crates/*"]`, `resolver = "3"`; `[workspace.package] version = "0.22.0"` (monorepo SDD M4: first suite version), `edition = "2024"`; `[workspace.dependencies]` for every dependency used by two or more crates, `netray-common = { path = "crates/common" }`, `reqwest = { version = "0.13", default-features = false }`, `axum-extra = "0.12"`. x509-parser stays per crate (0.17 vs 0.18, not covered by Req 2).
2. Every `crates/*/Cargo.toml`: `version.workspace = true`, `edition.workspace = true`, shared deps `{ workspace = true, features = [...] }` (keep extra features and `optional`), `netray-common = { workspace = true, ... }` first key in the table.
3. `crates/common/Cargo.toml` + `src/telemetry.rs` (`init_otel_layer`): otlp `default-features = false, features = ["http-proto","trace"]`; a newtype over reqwest 0.13 `blocking::Client` implementing `opentelemetry_http::HttpClient`, built on a spawned thread, passed via `.with_http_client(..)`, so only reqwest 0.13 resolves.
4. `crates/lens/src/state.rs:66`: `.use_rustls_tls()` → `.tls_backend_rustls()`; lens reqwest features `json, rustls, webpki-roots, stream`.
5. `crates/{beacon,mhost-prism,spectra,tlsight}/frontend/vite.config.ts` `cargoVersion()`: read `../../../Cargo.toml`.
6. Root `rust-toolchain.toml` (stable, rustfmt, clippy); delete per-crate `Cargo.lock` and `rust-toolchain.toml`; generate the root `Cargo.lock`.

### G2
1. Root `package.json`: `{"name":"netray","private":true,"workspaces":["crates/*/frontend","packages/common-frontend"]}`.
2. `crates/beacon/frontend/package.json`: typescript `^6.0.0`, vite `^8.0.0` (fix what TS 6 / vite 8 break in beacon's frontend).
3. `packages/common-frontend/package.json`: drop `publishConfig` and `prepublishOnly`, add `"private": true`.
4. Delete every `crates/*/frontend/.npmrc`, `packages/common-frontend/.npmrc`, every per-package `package-lock.json` incl. `crates/ifconfig-rs/tests/e2e/package-lock.json`; generate the root `package-lock.json` (`common-frontend` linked).

### G3
1. Root `justfile`: `adlc-verify` = test-repo, check-frontend-dist, fmt-check, clippy (`--workspace -D warnings`), Rust tests offline (`--workspace --exclude ifconfig-rs` + `-p ifconfig-rs --lib`), frontend tests (`npm test --workspaces --if-present`), validate-site, check-sitemap; `check` = adlc-verify + `cargo test --workspace`; `build`; `image` (`docker build` from the root `Dockerfile`); `acceptance`; `release version` (validate X.Y.Z, set `[workspace.package] version`, changelog section under `## [Unreleased]`, commit, tag, no push); `adlc-setup` (tools, `npm ci`, `build:types`, frontend builds); `ifconfig-data`, `ifconfig-data-image`, `tlsight-data`, `e2e crate` replacing the per-crate Makefiles.
2. `crates/ifconfig-rs/data/fetch.sh` porting the data Makefile; `crates/tlsight/build.rs` messages point at `just tlsight-data`.
3. Delete every per-crate `justfile` and the six Makefiles.
4. Root `Dockerfile` (node stage builds the workspaces, muslrust stage builds the workspace, alpine runtime; no `NODE_AUTH_TOKEN` mount), `.dockerignore`, root `CHANGELOG.md` with `## [Unreleased]`.

## Phase 2 — One binary

## Groups

- G1: C1, C5 — each service crate becomes a library with `run(...)` at the root of `src/lib.rs` (log target unchanged); `src/main.rs` deleted; ifconfig-rs loses its `[[bin]]`. Split by crate: G1a beacon, spectra, lens; G1b prism, tlsight, ifconfig-rs (no shared files).
- G2: C2–C4, C6–C12 — `crates/netray` (clap dispatch to the six `run`s, `site` module), root `build` recipe and `Dockerfile` ship the one binary. Depends on G1.

## Plan

### G1
- Move each `main()` body unchanged into `pub async fn run(config_arg: Option<String>)` at the root of `src/lib.rs` with its private helpers and the `RustEmbed` `Assets`; only `std::env::args().nth(1)` becomes the parameter; the `*_CONFIG` fallback and defaults stay in the service (beacon keeps logging `argv` vs `BEACON_CONFIG`). tlsight keeps the rustls provider install first; prism passes the resolved path to the reload watcher; lens moves `security_headers_mw` and keeps its embed in `spa.rs`; ifconfig-rs `run(config_path, print_config, check)` keeps `--check`/`--print-config`, no env fallback (as before).
- `git rm` every `crates/<svc>/src/main.rs`; remove ifconfig-rs `[[bin]]`.

### G2
- `crates/netray/Cargo.toml` (workspace version/edition, path deps on the six crates, netray-common `server`, axum/tokio/mime_guess/percent-encoding from the workspace, clap 4 derive, anyhow).
- `crates/netray/src/site.rs`: `run(bind, root)` — one fallback handler + response mapper for the requirement-7 headers; 404.html read at start; percent-decoded path, hidden segments 404 except `/.well-known/mta-sts.txt`; `mta-sts.*` host serves only the policy; try `p`, `p.html`, `p/index.html`; mime by file; cache-control by resolved extension, policy `no-cache`.
- `crates/netray/src/main.rs`: clap `netray {lens,dns,tls,http,email} [config]`, `ip [config] [--check] [--print-config]`, `site [--bind] [--root]`.
- Root `justfile` `build` → `cargo build --release -p netray`; root `Dockerfile` builds and ships `netray` (+ `site/`), `ENTRYPOINT ["netray"]`.
- Per-crate Dockerfiles/`Dockerfile.dev` stay for the CI/image spec (DEFERRED).
