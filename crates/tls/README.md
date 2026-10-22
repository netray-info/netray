# netray-tls

The TLS module of `netray serve` (V1 codename tlsight). A library, no binary: the
`netray` crate builds it from `[modules.tls]` (`crates/netray/src/registry.rs`), and netray-web
runs its inspector for API v1 (`POST /api/v1/tls/inspect`) and the `/inspect/tls` page.

## What it exports

| Item | Used by |
|---|---|
| `TlsModule` | the engine: `Module` with protocol `Tls`, the TLS section of a domain run |
| `TlsModule::inspector()` / `TlsInspector`, `TlsInspectRequest`, `TlsRefusal` | netray-web: API v1 and the inspector page |
| `ModuleConfig` | the registry: the `[modules.tls]` table |
| `module::translate_ports`, `module::band_port`, `module::REGISTER` | netray-web: checks and blocks per port, the TLS view |
| `security::RateLimitState` | netray-web: the `[api.tls]` limiters |

## What it measures

Per domain it resolves the addresses, applies the target policy, fits handshakes and protocol
probes into a per-check budget (cap-and-warn: fewer addresses with a warning, never a refusal),
handshakes each address and cross-checks the result against DNS (CAA, TLSA, the HTTPS record for
ECH). STARTTLS is negotiated on ports 25 and 587. Port 443, else the first port, is mapped onto
the checks.

## Check ids

`checks()` emits these 23 ids (`src/module.rs`), each with its texts in
`crates/engine/catalogue/checks.toml`:

| Area | Ids |
|---|---|
| Reachability | `tls.tls_reachable` |
| Certificate | `tls.chain_trusted`, `tls.not_expired`, `tls.hostname_match`, `tls.chain_complete`, `tls.strong_signature`, `tls.key_strength`, `tls.expiry_window`, `tls.cert_lifetime`, `tls.san_quality`, `tls.aia_reachability` |
| Protocol | `tls.tls_version`, `tls.forward_secrecy`, `tls.aead_cipher`, `tls.legacy_versions`, `tls.weak_suites`, `tls.ocsp_stapled`, `tls.ct_logged` |
| DNS and consistency | `tls.caa_compliant`, `tls.dane_valid`, `tls.consistency`, `tls.alpn_consistency`, `tls.ech_advertised` |

Evidence blocks: `tls.addresses`, `tls.chain`, `tls.handshake`, `tls.probes`, `tls.protocols`,
`tls.tlsa`, `tls.ct`, `tls.caa`, `tls.notices` (`module::REGISTER`).

## Configuration

All keys live in `netray.toml`; `crates/netray/netray.example.toml` lists them with their
defaults. Without a `[modules.tls]` table the module runs on its defaults (with a startup warning).

`[modules.tls]` (`src/config.rs` `ModuleConfig`; `deadline_ms`, the section window, is read by
the `netray` crate):

| Table | Keys (default) |
|---|---|
| `[modules.tls.limits]` | `handshake_timeout_secs` (5, at most 5), `request_timeout_secs` (15, at most 15), `max_ports` (7, at most 7), `max_concurrent_handshakes` (10), `max_ips_per_hostname` (10, at most 10), `per_target_per_minute` (60), `per_target_burst` (20), `check_budget` (10) |
| `[modules.tls.dns]` | `timeout_secs` (3) |
| `[modules.tls.validation]` | `expiry_warning_days` (30), `expiry_critical_days` (14), `check_dane` (true), `check_caa` (true), `check_ct` (false), `custom_ca_dir` (unset) |
| `[modules.tls.quality]` | `enabled` (true), `http_check_timeout_secs` (5, at most 5), `skip_http_checks` (false) |

Values above a cap are clamped with a warning; zero limits are refused. A `custom_ca_dir` must be
an existing directory; its `*.pem` and `*.crt` files are added to the Mozilla roots.

`[api.tls]` (`crates/web/src/api/config.rs` `ApiTlsConfig`), the limiters of
`POST /api/v1/tls/inspect`: `per_ip_per_minute` (30), `per_ip_burst` (10),
`per_target_per_minute` (60), `per_target_burst` (20).

`[inspect.tls]` (`crates/web/src/pages/inspect.rs`), the inspector page's budget: `deadline_ms`
(15000), `max_concurrent` (8).

## Data

`data/caa_domains.tsv` maps CAA `issue` domains to CA names; `build.rs` compiles it into a sorted
lookup table, and the build fails without it. Refresh it with `just tlsight-data` from the
repository root and commit the result.

## Testing

From the repository root:

```sh
just adlc-verify                              # the offline gate
just check                                    # the gate plus the full Rust suite
cargo test -p netray-tls --test it <module>   # one integration test module (tests/it/)
cargo test -p netray-tls --features testing --test module
cargo test -p netray-tls --features testing --test inspector
```

## License

MIT, see [LICENSE](LICENSE).
