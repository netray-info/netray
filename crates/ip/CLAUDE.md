# CLAUDE.md — netray-ip

The IP module of `netray serve` and the in-process IP enrichment of the other modules; formerly the
standalone service ifconfig-rs. What it is, its API and its config keys: [README.md](README.md).
The repository-wide rules, including the ip rules for `ip_host`, `[api.ip]` charging, the IP
inspector's blocks and the data image, are in the root [CLAUDE.md](../../CLAUDE.md); they are not
repeated here.

## Layout

| Path | What |
|---|---|
| `src/module.rs` | `IpModule` (engine module and `IpLookup`), `translate`, `translate_lookup`, `lookup_block`, `headers_block`, `REGISTER`, `enrichment_client` |
| `src/lookup.rs` | the `IpLookup` trait, `IpRefusal`, address and range parsing, `range_lookup`, `asn_lookup`, `diff`, `lookup_batch` |
| `src/enrichment.rs` | `EnrichmentContext`: loads every data source of `ModuleConfig` |
| `src/backend/` | `Ifconfig` (the lookup document) and the classifiers: cloud, VPN, datacenter, bot, Tor, Feodo, CINS, Spamhaus, ASN heuristic, ASN info, IANA, User-Agent |
| `src/handlers.rs`, `src/format.rs`, `src/negotiate.rs`, `src/extractors.rs` | the V1 renderers, serialisation, negotiation and header extraction netray-web reuses |
| `src/testing.rs` | feature `testing`: `StubIpLookup`, `stub_ip_module`, `golden_module`, `from_evidence` |
| `data/` | `fetch.sh` behind `just ifconfig-data`, the tracked `asn_patterns.toml`, the data image's `Dockerfile`; sources in `data/README.md` |
| [`docs/ip-enrichment.md`](../../docs/ip-enrichment.md) (repository root) | the classification reference |

## Rules

- **The binary enforces the required data, not the crate.** `IpModule::new` loads with no GeoIP
  database and with a missing optional list (`tests/module_data.rs`); `crates/netray/src/registry.rs`
  refuses a `[modules.ip]` without `geoip_city_db` and `geoip_asn_db` or with a missing file.
- **A configured GeoIP database or User-Agent regex file that fails to load is a `LoadError`**; an
  unconfigured or failing optional list only warns. The User-Agent parser loads only when
  `user_agent_regexes` is set.
- **A reload keeps the previous data on failure** (`IpModule::reload`, called on SIGHUP).
- **A new data source** needs a `ModuleConfig` field, its load in `EnrichmentContext`, a
  `netray_ip_enrichment_sources_loaded{source}` gauge beside the others, and its key in
  `crates/netray/netray.example.toml`.
- **`network.type` is one priority order**: internal > c2 > bot > cloud > vpn > tor > spamhaus >
  datacenter > residential (`classify_network_type`). `infra_type` is orthogonal: internal > cloud >
  datacenter > government > education > business > residential. Keep both when one changes.
- **`vpn.provider` is set only when the ASN heuristic named a VPN**; a community CIDR match alone
  leaves it `null`.
- **The ASN heuristic matches case-insensitive regexes against the ASN organisation name, or an
  exact ASN**, from `asn_patterns` when configured, otherwise the compiled-in defaults.
- **`ip.reputation`**: Spamhaus, C2 or Tor fail, VPN warns; at most four addresses per family are
  looked up. A change to these branches changes its catalogue text in
  `crates/engine/catalogue/checks.toml` too.
- **A new evidence key** goes into `REGISTER`. `lookup_block` publishes a text key only when known
  and always the nine flags.
- **`extract_headers` keeps at most 64 headers and 1024 bytes per value.**

## Tests

- Offline, as the gate runs them: `cargo test -p netray-ip --lib` and
  `cargo test -p netray-ip --features testing --test module --test module_data --test module_ua --test module_v3 --test contract_status --test evidence`.
- `just test-ifconfig-data` runs the whole crate and needs `crates/ip/data/GeoLite2-City.mmdb`
  (`just ifconfig-data`).
- `tests/snapshots_test.rs` pins the serialised `Ifconfig` with insta (`tests/snapshots/`);
  accept a change with `INSTA_UPDATE=always cargo test -p netray-ip --test snapshots_test`.
- `tests/fixtures/contracts/ifconfig-json.json` is netray-web's stub lookup in its tests:
  `UPDATE_GOLDEN=1 cargo test -p netray-ip --lib contract_golden` rewrites it.
- Benchmarks in `benches/` (negotiation, ASN heuristic, serialisation, cloud lookup):
  `cargo bench -p netray-ip`.
