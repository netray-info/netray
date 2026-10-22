# netray-ip

The IP module of `netray serve` (package `netray-ip`, formerly the standalone service ifconfig-rs).
A library with three consumers:

- the engine, through `IpModule` as the V2 module for `Protocol::Ip`;
- the dns, tls, http and email modules, through the in-process enrichment client;
- netray-web, through `IpLookup`, the renderers and the content negotiation behind `/api/v1`,
  the V1-compatible paths of the `ip.` host and the `/inspect/ip` page.

## The engine module

`IpModule` (`src/module.rs`) needs the domain's addresses. Of the public ones it looks up at most
four IPv4 and four IPv6 addresses, in ascending order, and emits one check:

| Check | Fails when | Warns when |
|---|---|---|
| `ip.reputation` | an address is on Spamhaus DROP/EDROP, a Tor exit node, or a Feodo C2 node | an address is a VPN endpoint |

Its texts live in `crates/engine/catalogue/checks.toml`. The evidence is the `ip.addresses` table,
one row per looked-up address (ASN, organisation, prefix, network type, country, city and the four
flags); when fewer addresses were looked up than resolved, the findings say how many. A domain with
no public address is not applicable; a failed lookup makes the section incomplete.

For the IP inspector the crate also builds the `ip.lookup` block (one lookup as fields,
`translate_lookup`) and the `ip.headers` block (the caller's `X-Forwarded-For` hops and request
headers, `headers_block`). The register of all three blocks is `module::REGISTER`.

## The lookup and enrichment API

- `IpLookup` (`src/lookup.rs`): `lookup`, `lookup_caller` (reverse DNS on, User-Agent parsed),
  their `_with` variants taking `LookupOptions { reverse_dns, lang }`, `range` (a CIDR prefix),
  `asn` (an AS number) and `geoip_build_epoch`. Refusals are `IpRefusal::InvalidInput` or
  `IpRefusal::Unavailable`.
- Free functions: `parse_address` and `check_address` (private and loopback addresses refused
  unless `internal_mode`), `parse_range`, `parse_diff_pair`, `diff`, `lookup_batch`.
- `enrichment_client(Option<&Arc<IpModule>>)`: the `netray_common::enrichment::EnrichmentClient`
  the other modules enrich addresses through; `None` without an IP module.
- `handlers`, `format::OutputFormat`, `negotiate`, `extractors`: the per-field JSON and plain
  renderers, the JSON/YAML/TOML/CSV serialisation and the V1 content negotiation netray-web reuses.
- `GEOIP_ATTRIBUTION`: the attribution text the GeoLite2 licence requires next to derived data.

How an address is classified (cloud, VPN, Tor, bot, C2, Spamhaus, datacenter, ASN category and
network role, geolocation) is described in [docs/enrichment.md](docs/enrichment.md).

## Configuration

All keys live in the one `netray.toml`; `crates/netray/netray.example.toml` lists them.

`[modules.ip]` (`src/config.rs` `ModuleConfig`, plus `deadline_ms`, default 2000, read by
`crates/netray/src/config.rs`). Without the table the IP section is off and the other modules get
no enrichment client. With it, `geoip_city_db` and `geoip_asn_db` are required, and a missing file
refuses startup. Every other key is optional; an unset one leaves its data out.

| Key | Data |
|---|---|
| `geoip_city_db`, `geoip_asn_db` | MaxMind GeoLite2 City and ASN (required) |
| `user_agent_regexes` | uap-core regexes for User-Agent parsing |
| `tor_exit_nodes`, `feodo_botnet_ips`, `cins_army_ips`, `spamhaus_drop` | threat lists |
| `cloud_provider_ranges`, `vpn_ranges`, `datacenter_ranges`, `bot_ranges` | CIDR lists |
| `asn_patterns`, `asn_info` | ASN name patterns, ASN category and network role |

The published image carries no data files (GeoLite2 licence): the deployment mounts them at
`/netray/data`. SIGHUP reloads every data file; a failed reload keeps the previous data.

`[api.ip]` (`crates/web/src/api/config.rs` `ApiIpConfig`): `internal_mode`, `filtered_headers`,
`per_ip_per_minute`, `per_ip_burst`, `per_target_per_minute`, `per_target_burst`,
`batch_max_size`, `batch_addresses_per_minute`, `batch_addresses_burst`. The hard caps
(600 per minute, burst 100, batch 100) are `HARD_CAP_*` in `src/config.rs`.

`[inspect.ip]` (`crates/web/src/pages/inspect.rs` `InspectLimits`): `deadline_ms` (default 5000)
and `max_concurrent`.

## What `ip.` still serves

`netray_web::api::ip_host` answers GET and HEAD on `/`, the fields `/ip`, `/tcp`, `/location`,
`/user_agent`, `/network`, `/host`, `/isp`, `/country`, `/city`, `/asn`, `/timezone`, `/latitude`,
`/longitude`, `/region`, `/headers`, `/all`, and `/ip/cidr`, each with an optional `/json`,
`/yaml`, `/toml` or `/csv` suffix (or `?format=`), and the query parameters `ip`, `dns`, `lang`
and `fields`. A `/` that negotiates to HTML goes on to the next handler. The JSON API lives under
`/api/v1` (`/ip`, `/ip/{address}`, `/ip/batch`, `/ip/range/{cidr}`, `/ip/diff`, `/asn/{n}`).

```sh
curl -H 'Accept: application/json' 'https://ip.netray.info/?ip=192.0.2.1'
```

## Testing

From the repository root:

```sh
just check                  # the full suite; without GeoIP data the ip tests that need it are skipped
just ifconfig-data          # fetch the data files (data/README.md; GeoLite2 needs a MaxMind licence key)
just test-ifconfig-data     # this crate including the tests that need the data
cargo test -p netray-ip --lib
cargo test -p netray-ip --features testing --test module --test module_v3 --test evidence
UPDATE_GOLDEN=1 cargo test -p netray-ip --lib contract_golden   # rewrite tests/fixtures/contracts/ifconfig-json.json
```

Run it with `netray serve` and a `[modules.ip]` table.

## License

MIT, see [LICENSE](LICENSE).
