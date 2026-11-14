# CLAUDE.md — netray

## What this is

The netray.info monorepo (public, MIT): one process, `netray serve`, from one Cargo + npm workspace,
serving the apex and every inspector host by `Host`. `README.md` is the human entry point.
netray.info is the only supported deployment; no deploy instructions for third parties (`CONTRIBUTING.md`).
The adlc working rules (receipt, baseline trailer, test changes, review) are in `AGENTS.md`.

Area rules load with the files they cover: `crates/web/CLAUDE.md` (pages, API v1, islands, gate,
design boards, admission, store), `crates/{dns,tls,http,email,ip,common}/CLAUDE.md`,
`tests/acceptance/CLAUDE.md`.

## Layout

| Path | What |
|---|---|
| `crates/{dns,tls,http,email,ip}` | module crates: a `Module`, inspector functions, module config; no router, listener or `main.rs` |
| `crates/web` | `netray-web`: host routing, pages, templates, islands (`crates/web/islands`, the one npm package), API v1, admission, badge, OG card |
| `crates/store` | `netray-store`: the history (`history.db`) |
| `crates/common` | `netray-common`, shared Rust |
| `crates/{model,engine}` | V2 core (`specs/sdd/v2.md` §3): `netray-model` the check vocabulary; `netray-engine` the `Module`/`FactsProvider` traits. Each V1 crate maps its status onto `netray_model::Status` once |
| `crates/netray` | the binary and composition root: `netray serve [<config>]`, `--check-config <path>`; builds registry and store, hands them to netray-web |
| `tests/repo` | repository structure checks (`just test-repo`), one script per check |
| `tests/acceptance` | Playwright suite against a deployed environment |
| `specs/sdd/v2.md` | the V2 SDD: amend it in the same commit as the code that changes what it says. Token source `crates/web/tokens.json`. PRD, reviews and baseline are private; cite them as "(private)" |

Site content lives in netray-web (`site/` was deleted by v2-cutover; parity fixtures in `crates/web/tests/fixtures/v1-site/`): guide prose `crates/web/templates/pages/guides/`, titles `crates/web/src/pages/guides.rs`, compare rows `crates/web/compare.toml`.

## Verbs and the gate

All verbs live in the root `justfile` (`just --list`); no crate has its own `justfile` or `Makefile`.

- `just adlc-setup` first in a fresh checkout: netray-web embeds the islands bundle.
- `just adlc-verify` is the gate (offline, no browser); `just check` adds the full Rust suite.
- Read `adlc verify`'s `verified:`/`refuted:` line; piped into `tail`, a refuted gate looks green.
- Write no file into the worktree while `adlc verify` runs: `cargo fmt --check` reads untracked files.
- `adlc verify` runs no tamper check: after a rename or move of protected tests, run `adlc check` and add one `ADLC-Test-Change` per listed file.
- `adlc feature start` branches from `origin/main`: with unpushed commits on `main`, fast-forward the branch first.
- Never edit another feature's `specs/features/<name>/` on a feature branch; supersede old decisions in the new spec's decision log.
- `report.md`: criteria statuses are `green`, `already_implemented`, `test-unwritable`; never a `|` in a criteria cell, even in backticks. Otherwise `adlc next` stays on implement.
- Integration tests: a module in `crates/<c>/tests/it/` (`test_test_binaries.sh`); run with `cargo test -p <crate> --test it <module>`.
- Tests that depend on feature unification run with `--workspace`; `-p <crate>` resolves features for that crate alone.
- `required-features = ["testing"]` tests need `--features testing`, or they are skipped.
- The gate's clippy runs without `--all-targets`; adding it surfaces old test-code lints the gate never sees.
- `tests/repo/*` read `git ls-files`: stage new and deleted files first. `start_bg` (`tests/repo/lib/netray.sh`) never in a subshell.
- A repo check over Cargo dependencies reads `cargo metadata`, never the TOML text. A purity guard is an allowlist, never a blocklist.
- A frontend dev-dependency major bump also moves its pin in `tests/repo/test_islands_package.sh` (protected: `ADLC-Test-Change`).
- `tests/repo/test_workflows.sh` checks shape only; a workflow change is verified by its first CI run or release tag.
- A freshly built binary (a cargo example) gets no network locally; replay its bytes from `python3` to check a live host.

## Conventions

- One workspace version (`[workspace.package]`); only `just release` changes it. Shared dependencies go in `[workspace.dependencies]` once two crates use them.
- One config, `netray.toml` (`netray.example.toml`), one loader `netray_common::config::load`, prefix `NETRAY_` (`NETRAY_SERVER__BIND`), `NETRAY_CONFIG` names the file.
- A `NETRAY_` variable a script exports reaches `serve` as a config key and stops it: list it in `TOOL_VARS` (`crates/netray/src/config.rs`). `serve` refuses legacy-prefixed variables too.
- Startup rejects are checked: a new startup `.expect`/`panic!` on a config value needs a `validate()` rule and a `startup_rejects` row in `tests/repo/test_check_config.sh`. Data files (GeoIP) are read at `serve` startup, not by `--check-config`.
- Features unify across the `netray` binary: select behaviour at runtime, never with `cfg!(feature)`.
- Outbound fetches of user- or domain-controlled URLs go only through `netray_common::fetch`.
- Never wrap self-timed work in an outer timeout at the same deadline: bound the wait by the deadline, the work by the time left plus a grace.
- Augment a request before its limit and cost checks, never after.
- Domain input is the UTS #46 A-label (`netray_common::domain::validate`); a refused-character test needs a URL-forbidden code point (`\`, space, `%`).
- A failed lookup is never an answer (Rule M, `specs/features/v2-unmeasured/spec.md`): SERVFAIL, REFUSED, timeout or a policy-refused redirect is `unmeasured`; only NXDOMAIN/NODATA or a wrong answer is a measured fail.
- An address lookup has two halves: keep the answered one, report the failed one (`AddressLookup { ips, error }`).
- Never derive a verdict from a finding's detail text; carry a structured marker (SPF `spf_incomplete`).
- An evidence block holds the fact that decided its check; omit unknowns, never a placeholder (`0`, `""`, a guessed TTL).
- A new check id needs its texts in `crates/engine/catalogue/checks.toml`: `finding`/`fix_hint`/`fix_owner` describe every warn and fail branch, `checks` only what the pass branch guarantees. Read the check first.
- A check outside the verdict is evidence, not a catalogue ID (the profile refuses weight 0).
- Narrowing a check moves its hard fail: confirm the condition still reaches a fail or weight in the profile.
- A new volatile path changes every stored fingerprint (next run stores an empty diff). A volatile key never decides presence: make the block unconditional or the sibling volatile. A volatile row path covers `<key>#<n>` repeats; never join rows into one cell.
- A profile version that only adds checks re-scores a stored run to its old grade. A new profile version moves the current-profile pins (`profile.rs`, `schema_derives.rs`, `scoring_regression.rs`, `api_reads.rs`) and every history fixture's `profile_version`: regenerate, never hand-edit.
- Contract goldens (`tests/fixtures/contracts/`): a module shape change fails its golden; regenerate with `UPDATE_GOLDEN=1 cargo test -p <crate> --test it contract_golden` (netray-ip: `--lib`), commit, keep `run_golden` green.
- Run goldens (`tests/fixtures/contracts/lens-*.json`) are never rewritten: a moved row is a hand edit under `ADLC-Test-Change` naming its requirement.
- History fixtures are recomputed: change evidence, never a status, then `UPDATE_FIXTURES=1 cargo test -p netray-web --test it every_history_fixture` until stable, and `just design-check`.
- When real data first reaches a code path after a contract fix, review that path: it may never have run.
- Timer-run scripts fail closed: atomic writes (temp in the same dir, then `mv`), clean only their own files, tested with stubs that behave like the real tool.
- Release notes trace to code: check each `CHANGELOG.md` claim against the code at the release commit.
- Crate docs point at the root verbs and `netray serve`, never at per-crate build commands.

## Rules

| File | Apply when |
|---|---|
| [`specs/rules/architecture-rules.md`](specs/rules/architecture-rules.md) | adding or modifying an HTTP service, the shared library, or a cross-service protocol (config, errors, rate limits, headers, probes, OpenAPI) |
| [`specs/rules/frontend-rules.md`](specs/rules/frontend-rules.md) | changing anything under `crates/web/templates`, `crates/web/static` or `crates/web/islands` |
| [`specs/rules/logging-rules.md`](specs/rules/logging-rules.md) | changing tracing init, log filters or `[telemetry]` config |
| [`specs/rules/workflow-rules.md`](specs/rules/workflow-rules.md) | creating or modifying a `.github/workflows/*.yml` file |
| [`specs/rules/design-rules.md`](specs/rules/design-rules.md) | changing anything under `crates/web/templates`, `crates/web/static` or `crates/web/islands`: run `just design-check` (private canvas) before review |
| [`specs/rules/comparison-page-rules.md`](specs/rules/comparison-page-rules.md) | adding or removing a tool, adding checks to one, or the quarterly review of `crates/web/compare.toml` and `templates/pages/compare.html` |
