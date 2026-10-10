# CLAUDE.md — netray

## What this is

The netray.info monorepo (public, MIT): six inspector services, the shared Rust crate, the shared
frontend package and the static site, built as one Cargo + npm workspace into one binary `netray`.
`README.md` is the human entry point; `specs/features/monorepo-p1/spec.md` records how the separate
repositories became this one. netray.info is the only supported deployment, and the repository
carries no deploy instructions for third parties (policy: `CONTRIBUTING.md`).

## Layout

| Path | What |
|---|---|
| `crates/{lens,mhost-prism,tlsight,http,beacon,ifconfig-rs}` | the services; each a library with its frontend in `frontend/` |
| `crates/common` | `netray-common`, shared Rust (workspace member, not published) |
| `crates/{model,engine}` | V2 core (planning SDD `v2.md` §3): `netray-model` the check vocabulary, no workspace dependency; `netray-engine` the `Module`/`FactsProvider` traits, depends on `netray-model` only (`tests/repo/test_engine_names_no_module.sh`). Each V1 crate maps its status word onto `netray_model::Status` once |
| `crates/netray` | the binary: subcommands `lens dns tls http email ip site`; all but `site` take `--check-config <path>` (exit 0 `config ok: <path>`, exit 1 with the error) |
| `packages/common-frontend` | `@netray-info/common-frontend` (workspace member, not published) |
| `site/` | static site, served by `netray site` |
| `tests/repo` | repository structure checks (`just test-repo`), one script per check |
| `tests/acceptance` | Playwright suite against a deployed environment (own `package-lock.json`) |
| `specs/rules`, `specs/features` | engineering rules; feature specs |

## Verbs and the gate

All verbs live in the root `justfile`; no crate or package has its own `justfile` or `Makefile`.

- `just adlc-setup` first in a fresh checkout: the crates embed `frontend/dist` and do not compile without it.
- `just adlc-verify` does not type-check the frontends; run `npx tsc --noEmit` in a frontend you changed.
- Shared frontend types are generated (`npm run build:types -w @netray-info/common-frontend`, run by `just adlc-setup`); a fresh checkout needs it before `tsc`.
- `just adlc-verify` is the gate: offline, no browser. `just check` adds the full Rust suite.
- `just build`, `just image`, `just acceptance` (network + browser), `just release X.Y.Z` (never pushes).
- The published image never contains data files (GeoLite2 licence); `netray ip` gets `/netray/data` from the deployment's mount.
- `just check` runs `cargo deny check bans licenses sources`; `just adlc-setup` installs cargo-deny when it is missing.
- CI (`ci.yml`) and the release (`release.yml`) follow `specs/rules/workflow-rules.md`: one image, exact version tags, no deploy step.
- Data: `just ifconfig-data` / `just test-ifconfig-data` (GeoIP), `just tlsight-data` (CAA table, committed).
- Run tests that depend on feature unification with `--workspace`; `cargo test -p <crate>` resolves features for that crate alone and passes where the binary is wrong.
- `tests/repo/*` read `git ls-files`: stage new and deleted files before running them.
- A repo check over Cargo dependencies reads `cargo metadata`, never the TOML text: renames, dotted keys and inline tables escape a text scan.
- The gate's clippy runs without `--all-targets`; a narrower command with `--all-targets` hits old test-code lints the gate never sees.
- `metrics_util` 0.20's `Snapshotter::snapshot()` swaps every value to 0 on read: read once, accumulate across reads, or render a `PrometheusRecorder`.
- A `describe_*!` alone renders no HELP line: also register the metric at startup.
- `tests/repo/test_workflows.sh` checks the workflows' shape, not their behaviour; a workflow change is verified by its first CI run or release tag.
- The `ifconfig-rs-data` image also carries tracked files (`asn_patterns.toml`): never copy it over `crates/ifconfig-rs/data/`.
- `adlc feature start` branches from `origin/main`: with unpushed commits on local `main`, fast-forward the new feature branch to `main` before writing the spec.

The adlc working rules (receipt, baseline trailer, test changes, review) are in `AGENTS.md`.

## Conventions

- **One workspace version.** Every crate inherits `[workspace.package] version`; only `just release` changes it.
- **Services are libraries.** Each service crate exposes an async `run(config)` and has no `main.rs` or `[[bin]]`; `crates/netray` only parses arguments and dispatches.
- **Config stays per service.** Config keys, the `*_CONFIG` variable and the env prefix (`LENS_`, `PRISM_`, `TLSIGHT_`, `IFCONFIG_`; `SPECTRA__` and `BEACON__` with a double underscore), metrics names and log targets are unchanged by the merge; do not unify them.
- **One config loader.** Config loads only through `netray_common::config::load`; every config struct carries `deny_unknown_fields` (`tests/repo/test_config_strict.sh`).
- **Contract goldens.** `tests/fixtures/contracts/` holds each backend's response as written by its own tests (`contract_golden`); lens's tests parse them. A backend shape change fails its golden test: regenerate with `UPDATE_GOLDEN=1 cargo test -p <crate> --test contract_golden` (ifconfig-rs: `--lib contract_golden`), commit, and keep lens green.
- **lens goldens.** `tests/fixtures/contracts/lens-*.json` pin lens's whole result; `UPDATE_GOLDEN=1 cargo test -p lens --test lens_golden` rewrites them, and a moved row needs `ADLC-Test-Change` naming its requirement.
- **prism's package is `prism`.** Run `cargo test -p prism`, not `-p mhost-prism`.
- **beacon orders verdicts** Skip < Info < Pass < Warn < Fail; a category with only Info sub-checks is not applicable to lens.
- **beacon goldens are literal scenarios**: a new one must carry every sub-check the checks emit for its records (FCrDNS one per IP, `single_mx`, `no_ipv6`, DMARC `no_ruf`).
- **A new check id needs its texts in lens**: `fix_for` and `guide_url_for` (`routes.rs`), the snapshot labels (`snapshot/render.rs`) and `CHECK_LABELS`/`CHECK_DESCRIPTIONS` (frontend `checkMeta.ts`); the `fix_for` test lists names by hand.
- **report.md criteria statuses** are `green`, `already_implemented` or `test-unwritable`; any other keeps `adlc next` on implement. Drop a criterion whose scenario left the spec.
- **Startup rejects are checked.** A new startup `.expect`/`panic!` on a config value needs a `validate()` rule and a `startup_rejects` row in `tests/repo/test_check_config.sh`.
- **Layer order**, outermost first: concurrency limit, `request_id`, security headers, CORS, body limit, trace, compression — so preflights and 413s carry the request id and headers.
- **`start_bg` never in a subshell**: the EXIT trap of `tests/repo/lib/netray.sh` kills only PIDs recorded in the parent shell.
- **`just acceptance-local`** starts the stack on free ports (`LOCAL_<NAME>_URL`); never assume 8000 or 8080 are free.
- **Test invalid input through the router**, not the handler: extractor rejections (invalid UTF-8) and unmatched paths answer before the handler runs; in axum 0.8 `/x/{*rest}` conflicts with `/x/{id}`.
- **Review the paths a contract fix wakes up.** When real data first reaches a code path, it may never have run before (lens `detect_no_mx` read every MX `fail` as "no MX").
- **Timer-run scripts fail closed**: write every file atomically (temp in the same dir, then `mv`), clean only their own files (`*.tmp.$$`, a per-run dir), and are tested with stubs that behave like the real tool (`curl` without `-f` saves the error page, exit 0).
- **Shared dependencies** go in `[workspace.dependencies]` once two crates use them.
- **Outbound fetches of user- or domain-controlled URLs** go only through `netray_common::fetch`; never build a `reqwest::Client` for them. Checking and pinning live in its DNS resolver; reqwest keeps redirect handling.
- **`FetchOptions::new` follows no redirect**: a test that follows one sets `max_redirects`.
- **rcgen is 0.14**: sign with `Issuer::from_params(&ca_params, &ca_key)`, not 0.13's three-argument `signed_by`.
- **Features unify.** A dependency feature one crate enables reaches every service in the `netray` binary; select behaviour at runtime, never with `cfg!(feature)`.
- **Crate docs** point at the root verbs and `netray <subcommand>`, never at per-crate build commands.
- **Deployment policy wording** lives only in `CONTRIBUTING.md`; `tests/repo/test_no_self_host.sh` rejects it anywhere else outside history.

## Rules

| File | Apply when |
|---|---|
| [`specs/rules/architecture-rules.md`](specs/rules/architecture-rules.md) | adding or modifying an HTTP service, the shared library, or a cross-service protocol (config, errors, rate limits, headers, probes, OpenAPI) |
| [`specs/rules/frontend-rules.md`](specs/rules/frontend-rules.md) | changing anything under `crates/*/frontend` or `packages/common-frontend` |
| [`specs/rules/logging-rules.md`](specs/rules/logging-rules.md) | changing tracing init, log filters or `[telemetry]` config |
| [`specs/rules/workflow-rules.md`](specs/rules/workflow-rules.md) | creating or modifying a `.github/workflows/*.yml` file |
| [`specs/rules/comparison-page-rules.md`](specs/rules/comparison-page-rules.md) | adding or removing a tool, adding checks to one, or the quarterly review of `site/compare/index.html` |
