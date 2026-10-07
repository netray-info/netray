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
| `crates/{lens,mhost-prism,tlsight,spectra,beacon,ifconfig-rs}` | the services; each a library with its frontend in `frontend/` |
| `crates/common` | `netray-common`, shared Rust (workspace member, not published) |
| `crates/netray` | the binary: subcommands `lens dns tls http email ip site` |
| `packages/common-frontend` | `@netray-info/common-frontend` (workspace member, not published) |
| `site/` | static site, served by `netray site` |
| `tests/repo` | repository structure checks (`just test-repo`), one script per check |
| `tests/acceptance` | Playwright suite against a deployed environment (own `package-lock.json`) |
| `specs/rules`, `specs/features` | engineering rules; feature specs |

## Verbs and the gate

All verbs live in the root `justfile`; no crate or package has its own `justfile` or `Makefile`.

- `just adlc-setup` first in a fresh checkout: the crates embed `frontend/dist` and do not compile without it.
- `just adlc-verify` is the gate: offline, no browser. `just check` adds the full Rust suite.
- `just build`, `just image`, `just acceptance` (network + browser), `just release X.Y.Z` (never pushes).
- Data: `just ifconfig-data` / `just test-ifconfig-data` (GeoIP), `just tlsight-data` (CAA table, committed).

The adlc working rules (receipt, baseline trailer, test changes, review) are in `AGENTS.md`.

## Conventions

- **One workspace version.** Every crate inherits `[workspace.package] version`; only `just release` changes it.
- **Services are libraries.** Each service crate exposes an async `run(config)` and has no `main.rs` or `[[bin]]`; `crates/netray` only parses arguments and dispatches.
- **Config stays per service.** Config keys, the `*_CONFIG` variable and the env prefix (`LENS_`, `PRISM_`, `TLSIGHT_`, `IFCONFIG_`; `SPECTRA__` and `BEACON__` with a double underscore), metrics names and log targets are unchanged by the merge; do not unify them.
- **Shared dependencies** go in `[workspace.dependencies]` once two crates use them.
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
