# CLAUDE.md — netray-tls

The TLS module of `netray serve` (V1 codename tlsight): a library holding the engine module
`TlsModule` and the inspector netray-web runs for API v1 and `/inspect/tls`. `README.md` lists
its exports, check ids and config keys. Repo-wide rules, verbs and the gate are in the root
[`CLAUDE.md`](../../CLAUDE.md).

## Layout

| Path | What |
|---|---|
| `src/module.rs` | `TlsModule` (`Module` impl, `checks()`, `REGISTER`, `VOLATILE`) and `translate*`: the inspection response onto `tls.<id>` checks and blocks |
| `src/inspector.rs` | `TlsInspector`: the API v1 / inspector-page path (port rules, per-client limiters, inspection) |
| `src/inspection.rs` | the inspection core: `resolve_target`, `filter_allowed`, `plan_*`, `inspect`, `InspectResponse` |
| `src/backend.rs` | `TlsBackend`: resolve and handshake; tests install a stub |
| `src/state.rs` | `AppState`: trust store, semaphores, DNS resolver, enrichment client |
| `src/config.rs` | `ModuleConfig` (`[modules.tls]`) and the runtime `Config`: clamped to hard caps, zeros refused |
| `src/tls/`, `src/dns/`, `src/validate/`, `src/quality/`, `src/security/` | handshake and probes, CAA/TLSA/HTTPS lookups, cross-validation, quality checks, target policy and limiters |
| `tests/it/` | the integration modules (one binary `it`); `tests/module.rs`, `tests/inspector.rs` need `--features testing` |

## Working rules

- **Check ids are fixed in `module.rs` `CHECKS`** (23). A new one also needs its catalogue texts (root CLAUDE.md) and a mapping in `translate`.
- **One cost plan.** Handshakes and probes go through `inspection::plan_budget`/`plan_counts`/`plan_cost`: cost is `ports * (inspected + probe_connections * probed)`. Over budget the plan inspects fewer addresses (first v4, first v6, then the rest; `select_representative_ips`) and probes fewer, each with a warning; the plan itself never refuses (a limiter still may). Keep the module and the API v1 path on these functions.
- **The module's budget** is `min(check_budget, per_target_burst)`; its per-target limiter is charged before a blocked-target refusal is returned (`TlsModule::measure`).
- **Target policy** comes from `netray_common::target_policy` (`security/target_policy.rs`); every resolved address passes `filter_allowed` before any connection.
- **`AcceptAnyCert`** (`tls/verifier.rs`, a local one in `tls/connect.rs`) is for connections to the inspected target only: the handshake and the HSTS/redirect check. Trust is judged afterwards against the trust store.
- **Trust store**: `webpki-roots` plus the `*.pem`/`*.crt` files of `validation.custom_ca_dir`, built once in `AppState::new`; `check_startup` refuses a `custom_ca_dir` that is not a directory.
- **CAA issuer matching** (`validate/caa_compliance.rs`): the CAA `issue` domain is looked up in the table `build.rs` generates from `data/caa_domains.tsv`; the CA name matches the issuer DN by normalized containment or a shared word of at least 6 characters. An unknown CAA domain does not match. Refresh the table with `just tlsight-data` when CAs are added or renamed.
- **`hsts` and `https_redirect`** are quality checks inside the inspection response (`quality/mod.rs`), not module check ids.

- **Expiry reads `expired`, not the day count**: `days_remaining` and `CertInfo.is_expired` truncate toward zero, so a leaf lapsed within the day stores 0 days and `expired` true.
- **`TlsParams.starttls` is a port label, not an upgrade**: only SMTP is upgraded (`tls/connect.rs`).
- **`summary` judges OCSP, CAA and DANE from the first handshake over all ports**: a per-port verdict reads the port's own data (`module::port_wide`).
- **Probe slots go to the first addresses whose handshake succeeded**, per address (`SlotBoard`), never behind a port-wide barrier: one SYN-dropping address would delay the section past its window.
- **A block id with an IPv6 address needs brackets** (`tls.chain@[2001:db8::1]:443`): `BlockId::parse` refuses a bare one, and `format!`-built fixtures hit it.
- **`tls.hostname_match` was narrowed once and left `chain_trusted`**: when narrowing a check, confirm its condition still reaches a hard fail.
- **rcgen is 0.14**: sign with `Issuer::from_params(&ca_params, &ca_key)`, not 0.13's `signed_by`.
