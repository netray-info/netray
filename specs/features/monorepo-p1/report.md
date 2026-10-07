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
