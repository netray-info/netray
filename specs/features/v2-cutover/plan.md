# Plan: v2-cutover

## Phase 1 — `netray serve` and `netray.toml`

### Groups

G1: C4, C12–C14, C16–C22 (`legacy.rs`) · G2: C2, C3, C9–C11 (`config.rs`) · G3: C1, C8, C23, C24 (`main.rs`, `lens::run_config`) · G4: C5–C7 (`netray.example.toml`, `netray.dev.toml`). Every test runs through `netray serve`, so G1, G2 and G4 go green only with G3's `mod` lines and subcommand; the four groups are coded by one coder in order G1 → G4.

### Plan

- **G1** `crates/netray/Cargo.toml`: `toml`, `serde`. `crates/netray/src/legacy.rs` (new): `pub fn refuse(raw: &toml::Table) -> Result<(), String>` over one static table of legacy key paths per origin (requirement 11), run on the raw file before deserialization; every message names the key path, its destination or "removed in 0.30.0", and the CHANGELOG 0.30.0 configuration table. Top-level `inspect` is legacy only for spectra's four keys. A bare key owned by exactly one module config names `[modules.<m>].<key>`; shared ones point to the CHANGELOG.
- **G2** `crates/netray/src/config.rs` (new): `Config` with `[server]` (+ `apex`), `[engine]`, `[store]`, `[admission]` (8 keys), `[badges]`/`[og_cards]` (lens's types), `[telemetry]`, `[modules]` (tables with `deadline_ms`); all `deny_unknown_fields`. `load(path, env)`: refuse the eleven legacy prefixes, legacy table on the raw file, filter `NETRAY_LIVE_TESTS`, `load_with_env(path, "NETRAY_", env)`, `validate()` (lens's rules under the new key names). `to_lens()` builds `lens::config::Config` (deadlines → `backends`, `deadline_ms` stripped from module tables, snapshots off). `specs/sdd/v2.md` S10 lists the tables.
- **G3** `crates/lens/src/lib.rs`: `run_config(config, registry)` split out of `run_with`. `crates/netray/src/main.rs`: `mod config; mod legacy;`, `Command::Serve { config, --check-config }`; check = load + `to_lens` + `lens_registry(.., false)`; serve = path from argument or `NETRAY_CONFIG`, load, `to_lens`, telemetry, `lens_registry(.., true)`, SIGHUP reload, `lens::run_config`.
- **G4** `crates/netray/netray.example.toml`: table headers uncommented, every key commented at its default, `[modules.*]` commented out wholly with `deadline_ms`. `crates/netray/netray.dev.toml`: local binds, dev store, no `[modules.ip]`.

CLAUDE.md stays out of the phase commit (shared prose, Phase 8).

## Phase 2 — Host routing

### Groups

G1: C16 (`legacy.rs`) · G2: C3, C15, C1, C2, C4–C7, C9, C10, C12–C14 (enrichment and host routing share `crates/netray/src/main.rs`; enrichment steps first). C8 deferred, C11 already in place. Test call sites of the module constructors were updated beforehand (c3f0201e).

### Plan

- **G1** `crates/netray/src/legacy.rs`: a fate "removed in 0.30.0, enrichment calls the in-process ip module" and four rows `modules.dns.backends.ip`, `modules.tls.backends.ip`, `modules.http.enrichment`, `modules.email.backends.ip_url`, matched before descent; prism/tlsight `backends.ip.*` and beacon's `backends.ip_url`/`timeout_ms` take the same fate.
- **G2 enrichment** `crates/common/src/enrichment.rs`: `EnrichmentSource` trait, `Transport::InProcess`, `EnrichmentClient::in_process` (base_url "", private-IP guard kept, `is_reachable` true). `crates/ip`: `enrichment` feature of netray-common; `IpModule` gains a stub variant (`testing::stub_ip_module(json)`), `impl EnrichmentSource for IpModule` (ifconfig `network` → `IpInfo`), `enrichment_client(Option<&Arc<IpModule>>)` re-exported. `crates/{http,dns,tls,email}`: `ModuleConfig` drops `enrichment`/`backends`; constructors `new(config, enrichment)`; V1 `Config`s and their own clients unchanged. `crates/netray/src/main.rs` `lens_registry`: ip module first, client to the four modules; `strip_lens_enrichment` keeps `netray lens` accepting its old tables (and its `ip_url` warning) until Phase 7.
- **G2 host routing** `crates/web/static/{bimi-logo.svg,mta-sts.txt}` (copies; `site/` keeps its own until Phase 7). `crates/web/Cargo.toml`: `tower` (util). `crates/web/src/host/mod.rs`: `HostRouters { apex, probes, apex_router, dns/tls/http/ip: Option<Router> }`, `router()` — probes first on any Host; host from `Host` or URI authority, port stripped, ASCII-lowercased; apex: `/favicon.svg`, `/bimi-logo.svg`, 404 for `/favicon.ico`, `/.well-known/*`, bare `/r`, `/api`, `/inspect` (± `/`) and `/metrics`, then `apex_router`; inspector hosts → their router or 404; `mta-sts.` → only the policy; `email.`, `lens.`, unknown, none → 404. `crates/lens/src/lib.rs`: `prepare` (state, stores, `app(state)`), `serve` (listeners, metrics, drain), `probe_router()`; `run_config` = both. `crates/netray`: depends on netray-web; the Serve arm composes `host::router` over `lens::prepare` and calls `lens::serve`. `specs/sdd/v2.md` §5.1 (`/health`, `/ready` before host routing; `bimi-logo.svg`) and §7.3 (unknown host 404, `mta-sts.`).

## Phase 3 — Redirects and 410

### Groups

G1: C1–C9, C12–C16 (`crates/web/src/host/`, `pages/errors.rs`) · G2: C10 (`entries/routes.rs`) · G3: C19, C20 (guide templates). C11, C17 deferred; C18 in place.

### Plan

- **G1** `pages/errors.rs`: `domain_path(raw)` shared; `query_redirect_absolute(apex, &Uri) -> Option<Response>` (absolute 301, 400 page for a bad value); `query_redirect` keeps its relative form (pinned by `pages_errors.rs`). `host/legacy.rs` (new): `moved` (301 absolute, GET/HEAD; other methods 405 `Allow: GET, HEAD`), `gone` (410, `ErrorResponse` code `gone`), `lens_host` (13a), `subdomain_common` (13f, 13h), `email_host` (13g, then f/h, then c/d on `/`, else e), `ip_successor` (14, `/ip/cidr` before `/ip/{fmt}`), `apex_legacy` (m, i, o, l, n in that order). `host/mod.rs` `dispatch`: apex `?d=` and `apex_legacy` before the reserved list (`/api/` leaves it), `lens.`/`email.` to their tables, inspector hosts `subdomain_common` (ip: `ip_successor` first) before the inspector. SDD §7.2 (`ip.` `/docs` 301) and §7.3 rows h, i, j, l–o.
- **G2** `entries/routes.rs`: `/r/{id}/{*rest}` → `entry_rest`: 410 `render::gone()` for a well-formed ID, `rest` one of `dns|tls|http|email|ip` and no stored entry; else `unmatched`.
- **G3** `domain-security-audit.html`: the dns, tls, email (and ip) samples become `/api/v1/…` (`POST /api/v1/dns/query`, `/api/v1/tls/inspect`, `/api/v1/email/inspect`, `GET /api/v1/ip/{address}`). `scoring.html`: only the `?d=…&dkim_selectors=…` sample changes (to `https://netray.info/example.com`); the rest of the `dkim_selectors` section is Phase 8's prose pass (inventory).
