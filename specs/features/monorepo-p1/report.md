# Report: monorepo-p1

## Phase 1 — Workspace and verbs

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 1: root Cargo workspace of every crate under `crates/`, `crates/common` by path, shared versions in `[workspace.dependencies]`, one root `Cargo.lock` and `rust-toolchain.toml`, edition 2024 | green | tests/repo/test_cargo_workspace.sh |
| C2 | Req 2: one resolved version of `reqwest` and of `axum-extra` | green | tests/repo/test_dependency_alignment.sh |
| C3 | Req 3: npm workspaces for six frontends + `packages/common-frontend`, common package via workspace, no GitHub Packages / `_authToken` in any `.npmrc`, one root `package-lock.json` (`tests/acceptance` keeps its own), one major of `vite` and `typescript` | green | tests/repo/test_npm_workspaces.sh |
| C4 | Req 8: root `justfile` verbs `check`, `adlc-verify`, `build`, `image`, `acceptance`, `release X.Y.Z`; no crate/package keeps a `justfile` or `Makefile` | green | tests/repo/test_verbs.sh |
| C5 | Req 9: `adlc.toml` contract `adlc-verify`, no `no-ci` | already_implemented | tests/repo/test_adlc_contract.sh |
| C6 | GIVEN clean clone WHEN `cargo metadata` THEN members exactly the crates under `crates/`, `netray-common` resolves to `crates/common` | green | tests/repo/test_cargo_workspace.sh |
| C7 | GIVEN the tree WHEN searched for `Cargo.lock`, `rust-toolchain.toml`, `justfile`, `Makefile` outside the root THEN none | green | tests/repo/test_cargo_workspace.sh |
| C8 | GIVEN the workspace WHEN `cargo tree --workspace -d -e normal` THEN neither `reqwest` nor `axum-extra` twice | green | tests/repo/test_dependency_alignment.sh |
| C9 | GIVEN the tree WHEN every `.npmrc`/`package.json` read THEN no `npm.pkg.github.com`/`_authToken`; every frontend's `@netray-info/common-frontend` linked to `packages/common-frontend` | green | tests/repo/test_npm_workspaces.sh |
| C10 | GIVEN workspace packages WHEN `vite`/`typescript` ranges read THEN one major each | green | tests/repo/test_frontend_toolchain_majors.sh |
| C11 | GIVEN the root WHEN `just --summary` THEN lists `check`, `adlc-verify`, `build`, `image`, `acceptance`, `release` | green | tests/repo/test_verbs.sh |
| C12 | GIVEN `adlc.toml` WHEN read THEN `[contract] recipe = "adlc-verify"`, no `no-ci` | already_implemented | tests/repo/test_adlc_contract.sh |
| C13 | GIVEN the workspace WHEN `cargo test --workspace` THEN ≥1,650 Rust test functions, all pass; frontend ≥171 cases, all pass | green | tests/repo/test_suite_size.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet (C7 part closed by G3) | 65,178 | 188 |
| G2 | 1 | sonnet | 34,866 | 60 |
| G3 | 3 | stuck at `check-sitemap`; closed by the orchestrator (sitemap landing `lastmod` from the deleted `site/index.html` → lens frontend) | 81,318 | 219 |

### Review

First pass (reader over 980d1d2..working tree): 6 BLOCKER, 3 AMENDMENT, 2 DEFERRED.

| finding | class | resolution |
|---|---|---|
| `build-sitemap.mjs` landing lastmod from a directory this phase changes; gate red after commit | BLOCKER | lastmod from `crates/lens/frontend/index.html`; sitemap regenerated |
| hand-built OTLP blocking client dropped the export timeout (30 s instead of 10 s, env ignored) | BLOCKER | `otlp_export_timeout` mirrors otlp 0.31.1 `resolve_timeout`; tests added test-first |
| workspace feature unification changed spectra/beacon/tlsight enrichment (transport, User-Agent, cache, metrics) | BLOCKER | operator: runtime option. `EnrichmentMode::{Plain, Backend { cache_ttl_secs }}`; prism Backend 300 s, tlsight/beacon Backend 0, spectra Plain; test-first |
| lens moved to reqwest 0.13 `rustls` (OS trust store); `webpki-roots` feature dead | BLOCKER | dead feature removed; OS trust store accepted — the other five services already run so, the image carries `ca-certificates` |
| `release` committed with `-a`, no clean-tree check | BLOCKER | refuses a dirty tree, commits only `Cargo.toml Cargo.lock CHANGELOG.md`; test-first |
| `validate-site` cannot fail (`HTMLParser().feed` never raises), copied unchanged from the meta repo | BLOCKER → DEFERRED | operator: known defect, a real HTML5 validator is a separate decision |
| every service now reports the suite version 0.22.0 | AMENDMENT | spec requirement 1 amended (repaired in phase) |
| feature unification vs. requirement 5 | AMENDMENT | resolved by the runtime option above |
| ifconfig-rs data-dependent `#[ignore]` tests lost their runner | AMENDMENT | `just test-ifconfig-data`; broken `tests/Dockerfile.tests` removed |
| per-crate Dockerfiles no longer build | DEFERRED | later CI/image spec |
| root image lacks the `*_SERVER__BIND` env and the ifconfig-rs data stage | DEFERRED | later CI/image spec (Phase 2 replaces the binaries anyway) |

Second pass (reader over the fixes): 0 BLOCKER. Enrichment per service equals its pre-workspace path; OTLP timeout matches; `release` guard holds; sitemap stable. AMENDMENT `release` did not refuse an existing tag or a lower version, NIT version interpolated unquoted — both repaired in phase (`quote(version)`, tag and version checks).

### Behavioural verification

```
$ just --summary
acceptance adlc-setup adlc-verify build check check-frontend-dist check-sitemap clippy default e2e fmt-check ifconfig-data ifconfig-data-image image release test-frontend test-ifconfig-data test-repo test-rust test-rust-offline tlsight-data validate-site
$ just build        # exit 0, Finished `release` profile in 38.21s
target/release: beacon ifconfig-rs lens prism spectra tlsight
$ just -n release 9.9.9 | grep v=
v='9.9.9'
$ bash tests/repo/test_release_guard.sh
PASS: release refused on dirty tree
```
`just image` not run: no Docker daemon on this machine.

## Phase 2 — One binary

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | Req 4: each service crate is a library with an async entry point taking the config path; no `[[bin]]` or `main.rs` in a service crate | green | tests/repo/test_service_libs.sh |
| C2 | Req 5: `crates/netray` builds `netray`; `netray <service> [config-path]` starts that service with unchanged config resolution, env prefix, telemetry defaults, metrics names, log target; `--help` lists seven subcommands | green | tests/repo/test_cli.sh, test_service_config_and_metrics.sh |
| C3 | Req 6: every service subcommand answers `/` and `/health` with 200, unknown path with the SPA `index.html` and 200 | green | tests/repo/test_smoke_services.sh |
| C4 | Req 7: `netray site` routing, MTA-STS host, headers, cache-control | green | tests/repo/test_site.sh |
| C5 | GIVEN service crates WHEN read THEN no `[[bin]]`/`src/main.rs` | green | tests/repo/test_service_libs.sh |
| C6 | GIVEN built `netray` WHEN `--help` THEN lists `lens dns tls http email ip site` | green | tests/repo/test_cli.sh |
| C7 | GIVEN each dev config on a free port WHEN `netray <service>` THEN `/`, `/health` 200, `/does-not-exist` 200 with `index.html` | green | tests/repo/test_smoke_services.sh |
| C8 | GIVEN config path argument and a different `*_CONFIG` WHEN started THEN the argument wins | green | tests/repo/test_service_config_and_metrics.sh |
| C9 | GIVEN metrics address WHEN scraped after one request THEN metric names carry the old prefix | green | tests/repo/test_service_config_and_metrics.sh |
| C10 | GIVEN `netray site` WHEN `/guide/`, `/guide/dnssec` THEN 200; `/`, `/does-not-exist` 404 with `404.html` body; `/.git/config` 404 | green | tests/repo/test_site.sh |
| C11 | GIVEN `netray site` WHEN `/.well-known/mta-sts.txt` with `Host: mta-sts.example.com` THEN 200 `text/plain` `no-cache` body equal to the file; `/guide/` on that host 404 | green | tests/repo/test_site.sh |
| C12 | GIVEN `netray site` WHEN any response THEN every Req-7 header with its value | green | tests/repo/test_site.sh |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1a beacon, spectra, lens | 3 | sonnet (its own group command added `--all-targets`, which trips pre-existing lens test lints; green under the gate's clippy) | 39,871 | 100 |
| G1b prism, tlsight, ifconfig-rs | 1 | sonnet | 41,270 | 72 |
| G2 netray binary + site | 2 | sonnet; two test defects fixed by the orchestrator (`env … start_bg` cannot call a shell function; ANSI codes in the text log broke the `config_source` grep) | 64,007 | 231 |

Note: `cargo test --workspace` now reports fewer passed tests (≈1,560 vs. 2,419 before) because each service's `src/main.rs` re-declared its modules, so their unit tests ran twice (bin and lib). The test functions are unchanged: 1,656 `#[test]` attributes, every `src/*.rs` still declared as a module.

### Review

First pass (reader over 786b68e..working tree): 1 BLOCKER, 3 AMENDMENT, 1 DEFERRED, 2 NIT. All six `run` bodies match the old `main.rs` apart from where the config path comes from; startup logs, log targets, `ip --check`/`--print-config` identical to the old binaries; path traversal safe; every requirement-7 header byte-exact.

| finding | class | resolution |
|---|---|---|
| `resolve` trimmed the trailing slash, so `/guide/dnssec/` served the file (relative CSS then broke) | BLOCKER | trailing-slash paths only try `<path>/index.html` (nginx `try_files $uri $uri.html $uri/`); test-first in `test_site.sh` |
| MTA-STS policy `no-cache` on every host | AMENDMENT | kept; requirement 7 now says so (repaired) |
| 404 responses carry no `Cache-Control` | AMENDMENT | kept (nginx parity); requirement 7 now says so (repaired) |
| `/404.html`, `/50x.html` directly reachable (nginx: `internal`) | AMENDMENT | now 404, requirement 7 amended, test-first (repaired) |
| per-crate Dockerfiles build `--bins` that no longer exist | DEFERRED | later CI/image spec |
| non-GET methods get 405 without the 404 body | NIT | listed |
| index alone did not build at review time | NIT | moot after the commit |

Second pass (reader over the `resolve` fix): 0 BLOCKER. Matches nginx on every listed case, incl. percent-encoded and dot-segment variants; `/404`, `/50x` serve the page as nginx did (spec names only the `.html` paths).
- NIT: no test pins the policy's `no-cache` off the mta-sts host. Listed.
- NIT: no test pins "404 carries no Cache-Control". Listed.
- DEFERRED: `site/api/index.html` names `/api` as canonical but links relatively, so links break at `/api` (pre-existing site content, routing correct). Listed.

### Behavioural verification

```
$ target/debug/netray --help
Commands:
  lens   Domain health checker (netray.info)
  dns    DNS inspector (dns.netray.info)
  tls    TLS certificate inspector (tls.netray.info)
  http   HTTP header inspector (http.netray.info)
  email  Email security inspector (email.netray.info)
  ip     IP enrichment API (ip.netray.info)
  site   Static site (guide, API docs, tools, compare)
$ netray site --bind 127.0.0.1:$p --root site; curl …
/guide/          200 text/html; charset=utf-8
/guide/dnssec    200 text/html; charset=utf-8
/                404 text/html; charset=utf-8
/.git/config     404 text/html; charset=utf-8
$ curl -H 'Host: mta-sts.example.com' …/.well-known/mta-sts.txt
version: STSv1
mode: enforce
mx: smtp.google.com
max_age: 86400
```
The six service subcommands are exercised by `tests/repo/test_smoke_services.sh` (all green: `/`, `/health`, SPA fallback) and `test_service_config_and_metrics.sh` (argument beats `*_CONFIG`; `spectra_http_requests_total`, `prism_http_requests_total` unchanged).
