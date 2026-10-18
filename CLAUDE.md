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
| `crates/{lens,dns,tls,http,email,ip}` | the services; each a library with its frontend in `frontend/` |
| `crates/common` | `netray-common`, shared Rust (workspace member, not published) |
| `crates/{model,engine}` | V2 core (`specs/sdd/v2.md` §3): `netray-model` the check vocabulary, no workspace dependency; `netray-engine` the `Module`/`FactsProvider` traits, depends on `netray-model` only (`tests/repo/test_engine_names_no_module.sh`). Each V1 crate maps its status word onto `netray_model::Status` once |
| `crates/netray` | the binary: subcommands `lens dns tls http email ip site`; all but `site` take `--check-config <path>` (exit 0 `config ok: <path>`, exit 1 with the error) |
| `packages/common-frontend` | `@netray-info/common-frontend` (workspace member, not published) |
| `site/` | static site, served by `netray site` |
| `tests/repo` | repository structure checks (`just test-repo`), one script per check |
| `tests/acceptance` | Playwright suite against a deployed environment (own `package-lock.json`) |
| `specs/rules`, `specs/features` | engineering rules; feature specs |
| `specs/sdd/v2.md` | the V2 SDD, this repository's own: amend it in the same commit as the code that changes what it says. Its token source is `crates/web/tokens.json`. PRD, reviews and baseline stay private; cite them as "(private)" |

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
- A service moved in-process keeps every check its route applied to lens's calls (input validation, per-target limits, policy); only per-client limits go.
- hickory 0.26 `Name::eq` compares `is_fqdn`: a live query name without the trailing dot never equals a wire record name; never compare owners against the query name.
- `tests/repo/test_workflows.sh` checks the workflows' shape, not their behaviour; a workflow change is verified by its first CI run or release tag.
- CI's check job compiles the workspace tests twice (offline suite, then `test-rust` with GeoIP data); a cold-cache run took 25m51s of its 30-minute timeout.
- The `ifconfig-rs-data` image also carries tracked files (`asn_patterns.toml`): never copy it over `crates/ip/data/`.
- `adlc feature start` branches from `origin/main`: with unpushed commits on local `main`, fast-forward the new feature branch to `main` before writing the spec.
- lens records a run after its response: a test reading `history.db` or the record-error counter after a response polls with a bound or awaits `state.runs.drain()`.
- lens starts one run per domain per 60 s on tokio's clock: a second run of one domain in a test needs `common::pass_throttle()` (current_thread) or another domain.
- A paused tokio clock times out sqlx pool acquires: never combine `start_paused` or `pause()` with a `HistoryStore`.
- governor's `check` spends a token whatever follows: take a keyed limiter after every refusal that must cost nothing.
- Adding a field to a protected test's struct literal still needs `ADLC-Test-Change`: the gate's tamper check flags any change.
- Never put `|` in a `report.md` criteria cell, even in backticks: `adlc next` splits cells on it and keeps the phase open.

The adlc working rules (receipt, baseline trailer, test changes, review) are in `AGENTS.md`.

## Conventions

- **One workspace version.** Every crate inherits `[workspace.package] version`; only `just release` changes it.
- **Services are libraries.** Each service crate exposes an async `run(config)` and has no `main.rs` or `[[bin]]`; `crates/netray` only parses arguments and dispatches.
- **Config stays per service.** Config keys, the `*_CONFIG` variable and the env prefix (`LENS_`), metrics names and log targets are unchanged by the merge; do not unify them. A module converted to the V2 engine switches hard to `NETRAY_<P>_` (`NETRAY_<P>_CONFIG` for the file, `NETRAY_<P>_SECTION__KEY` for keys) with no fallback, and refuses its old prefix at load (`netray_common::config::refuse_legacy_prefix`), naming the new one. Converted so far: http (`NETRAY_HTTP_`, was `SPECTRA__`), email (`NETRAY_EMAIL_`, was `BEACON__`; `BEACON_CONFIG` too), ip (`NETRAY_IP_`, was `IFCONFIG_`; `IFCONFIG_CONFIG` too), tls (`NETRAY_TLS_`, was `TLSIGHT_`; `TLSIGHT_CONFIG` too), dns (`NETRAY_DNS_`, was `PRISM_`; `PRISM_CONFIG` too).
- **One config loader.** Config loads only through `netray_common::config::load`; every config struct carries `deny_unknown_fields` (`tests/repo/test_config_strict.sh`).
- **Contract goldens.** `tests/fixtures/contracts/` holds each backend's response as written by its own tests (`contract_golden`); lens's tests parse them. A backend shape change fails its golden test: regenerate with `UPDATE_GOLDEN=1 cargo test -p <crate> --test contract_golden` (ifconfig-rs: `--lib contract_golden`), commit, and keep lens green.
- **lens goldens.** `tests/fixtures/contracts/lens-*.json` pin lens's whole result; `UPDATE_GOLDEN=1 cargo test -p lens --test lens_golden` rewrites them, and a moved row needs `ADLC-Test-Change` naming its requirement.
- **prism's package is `netray-dns`.** Run `cargo test -p netray-dns`, not `-p prism` or `-p mhost-prism`.
- **beacon orders verdicts** Skip < Info < Pass < Warn < Fail; a category with only Info sub-checks is not applicable to lens, except `email.mx` (pass, Info as findings) and `email.dnsbl` (pass when one zone answered). A category carrying the `unmeasured` sub-check is `unmeasured`.
- **A failed lookup is never an answer** (Rule M, `specs/features/v2-unmeasured/spec.md`): SERVFAIL, REFUSED, timeout or a policy-refused redirect is `unmeasured`; only NXDOMAIN/NODATA or a wrong answer from the target is a measured fail.
- **An address lookup has two halves.** Keep the answered half and report the failed one (`AddressLookup { ips, error }`); never fail the whole lookup on one family.
- **Never derive a verdict from a finding's detail text.** Carry it in a structured marker (e.g. SPF `spf_incomplete` with `ipv4`/`ipv6`/`both`/`any`).
- **An evidence block holds the fact that decided its check.** Omit unknowns; never publish a placeholder (`0`, `""`, a guessed TTL). `check_evidence_rule` and each module's `validate` test enforce it.
- **Carry new internal data on V1 response structs as `#[serde(skip)]` fields** so the V1 wire and its goldens stay byte-identical; test struct literals then need the field.
- **Module crates' `required-features = ["testing"]` tests need `--features testing`**: `cargo test -p netray-email --features testing --test evidence`; without it they are skipped or refused.
- **A new volatile path changes every stored fingerprint**: the next run of each domain stores an entry with an empty diff (`crates/store/src/lib.rs` compares the stored fingerprint).
- **beacon goldens are literal scenarios**: a new one must carry every sub-check the checks emit for its records (FCrDNS one per IP, `single_mx`, `no_ipv6`, DMARC `no_ruf`).
- **A new check id needs its texts in the catalogue**: `crates/engine/catalogue/checks.toml` (lens reads titles, fix hints, owners and guide links from it) and `CHECK_LABELS`/`CHECK_DESCRIPTIONS` in the frontend `checkMeta.ts`, which `tests/repo/test_checkmeta_catalogue.sh` holds to the catalogue's names and titles. A catalogue text (`finding`, `fix_hint`, `fix_owner`) describes every warn and fail branch of the check's code: read the check before writing it.
- **Narrowing a check moves its hard fail.** When a check stops failing on a condition, confirm the condition still reaches a hard fail or weight in the profile (`tls.hostname_match` left `chain_trusted`).
- **Release notes trace to code.** Check each `CHANGELOG.md` claim against the code at the release commit, not against specs or the SDD (changelog-0-25-0: six wrong claims).
- **report.md criteria statuses** are `green`, `already_implemented` or `test-unwritable`; any other keeps `adlc next` on implement. Drop a criterion whose scenario left the spec.
- **Startup rejects are checked.** A new startup `.expect`/`panic!` on a config value needs a `validate()` rule and a `startup_rejects` row in `tests/repo/test_check_config.sh`.
- **Data files are checked at startup, not by `--check-config`.** Data files named in config (lens's `[modules.ip]` GeoIP databases, as `netray ip`) are read at startup; the config check parses the table and its keys only.
- **Layer order**, outermost first: concurrency limit, `request_id`, security headers, CORS, body limit, trace, compression — so preflights and 413s carry the request id and headers.
- **`start_bg` never in a subshell**: the EXIT trap of `tests/repo/lib/netray.sh` kills only PIDs recorded in the parent shell.
- **`just acceptance-local`** starts the stack on free ports (`LOCAL_<NAME>_URL`); never assume 8000 or 8080 are free.
- **Test invalid input through the router**, not the handler: extractor rejections (invalid UTF-8) and unmatched paths answer before the handler runs; in axum 0.8 `/x/{*rest}` conflicts with `/x/{id}`, while a static segment beside a catch-all (`/r/{id}/og.png` with `/r/{id}/{*rest}`) does not.
- **Review the paths a contract fix wakes up.** When real data first reaches a code path, it may never have run before (lens `detect_no_mx` read every MX `fail` as "no MX").
- **Timer-run scripts fail closed**: write every file atomically (temp in the same dir, then `mv`), clean only their own files (`*.tmp.$$`, a per-run dir), and are tested with stubs that behave like the real tool (`curl` without `-f` saves the error page, exit 0).
- **Shared dependencies** go in `[workspace.dependencies]` once two crates use them.
- **An inspector under `/api/v1` runs on its module's `[modules.<x>]` state**; `[api.<x>]` holds only per-client limits (email adds its slots). Never build a second inspector state.
- **Compose each new inspector onto the finished v1 document** (`api/compose.rs`, one step per part): composing it with the earlier parts renames a committed component and fails `openapi_contract`.
- **A golden over prism's routes zeroes `latency_ms` and sorts hash-map-ordered lists** (authcompare's servers); capture it twice before committing.
- **Outbound fetches of user- or domain-controlled URLs** go only through `netray_common::fetch`; never build a `reqwest::Client` for them. Checking and pinning live in its DNS resolver; reqwest keeps redirect handling.
- **`FetchOptions::new` follows no redirect**: a test that follows one sets `max_redirects`.
- **rcgen is 0.14**: sign with `Issuer::from_params(&ca_params, &ca_key)`, not 0.13's three-argument `signed_by`.
- **Features unify.** A dependency feature one crate enables reaches every service in the `netray` binary; select behaviour at runtime, never with `cfg!(feature)`.
- **Crate docs** point at the root verbs and `netray <subcommand>`, never at per-crate build commands.
- **Deployment policy wording** lives only in `CONTRIBUTING.md`; `tests/repo/test_no_self_host.sh` rejects it anywhere else outside history.
- **askama 0.16 closes every `{% call %}` with `{% endcall %}`**; without it the component macro does not compile.
- **A `base.html` block default compiles against every child**: put frame-struct fields only inside blocks a struct-less child overrides (v2-entries' pages override `meta`).
- **Stub a third-party API with a body captured from the live endpoint**, never the shape the code reads (status island: the event feed answers `results`, not `data`).
- **A purity guard is an allowlist.** `tests/repo/test_spectrum_module.sh` admits named imports, std paths and macros; a blocklist of I/O names let print macros and `std::process` through.
- **The spectrum follows the grade.** `spectrum::layout` mutes a section with a weighted unmeasured check and sums M over the profile, as `scoring.rs` does; change both together.
- **No `Router` word in `crates/web/src` outside `entries/`, `pages/`, `api/`**, comments included: `tests/repo/test_web_crate.sh` greps the text.
- **Page tests assert a component by its markup** (`class="diff-block"`), never the bare class name: netray-web's pages inline every component stylesheet.
- **Identifier-valued CSS properties** (`grid-area`, `transition`) go into `IDENTIFIER_PROPERTIES` in `crates/web/src/gate.rs`, or the gate refuses colour words in them.
- **`site/` is frozen until v2-cutover; content changes go to netray-web:** guide prose to `crates/web/templates/pages/guides/`, guide titles and descriptions to `crates/web/src/pages/guides.rs`, compare rows to `crates/web/compare.toml`.

## Rules

| File | Apply when |
|---|---|
| [`specs/rules/architecture-rules.md`](specs/rules/architecture-rules.md) | adding or modifying an HTTP service, the shared library, or a cross-service protocol (config, errors, rate limits, headers, probes, OpenAPI) |
| [`specs/rules/frontend-rules.md`](specs/rules/frontend-rules.md) | changing anything under `crates/web/templates`, `crates/web/static` or `crates/web/islands` |
| [`specs/rules/logging-rules.md`](specs/rules/logging-rules.md) | changing tracing init, log filters or `[telemetry]` config |
| [`specs/rules/workflow-rules.md`](specs/rules/workflow-rules.md) | creating or modifying a `.github/workflows/*.yml` file |
| [`specs/rules/comparison-page-rules.md`](specs/rules/comparison-page-rules.md) | adding or removing a tool, adding checks to one, or the quarterly review of `crates/web/compare.toml` and `templates/pages/compare.html` |
