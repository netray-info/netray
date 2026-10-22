# netray-dns

The DNS module of `netray serve` (V1 codename prism), a library inside the netray workspace. It
provides:

- `DnsModule`, the V2 engine module for protocol `dns`. It is both a `Module` (the DNS section of
  a domain run) and the `FactsProvider` (the shared A, AAAA, MX, CAA, NS and HTTPS facts the other
  modules read).
- `DnsJobs`, the DNS inspector jobs netray-web runs for API v1 (`POST /dns/{query,compare,authcompare,trace,dnssec}`,
  plus `/dns/check`, `/dns/parse`, `/dns/servers`, `/dns/record-types`) and for the
  `/inspect/dns` page.
- The query language parser (`domain [TYPE...] [@server...] [+flag...]`), the record formatter
  and the input policy shared by both.

## Checks

A domain run queries the configured servers for the domain's records, runs the check pipeline
over them and maps its results onto these catalogue ids (`crates/engine/catalogue/checks.toml`):

| Id | Title |
|---|---|
| `dns.dnssec` | DNSSEC signed |
| `dns.dnskey_algorithm` | DNSKEY algorithm |
| `dns.dnssec_rollover` | DNSSEC key rollover |
| `dns.caa` | CAA records |
| `dns.ns` | Nameserver redundancy |
| `dns.ns_lame` | Lame delegation |
| `dns.ns_delegation` | Delegation consistency |
| `dns.cname_apex` | No CNAME at apex |
| `dns.https_svcb` | HTTPS record |
| `dns.ttl` | TTL range |

The pipeline's email categories are dropped; email is judged by the email module. A check whose
input lookup failed is `unmeasured`; without DNSKEY records, `dns.dnskey_algorithm` and
`dns.dnssec_rollover` are not applicable.

## Evidence

One records block `dns.<type>` per answered record type, keys `owner`, `type`, `ttl` and `data`,
for `a`, `aaaa`, `mx`, `caa`, `ns`, `https`, `cname`, `dnskey`, `ds`, `nsec`, `nsec3`, `rrsig`,
`soa`, `svcb` and `txt`. Each check references the blocks of its inputs; `dns.ttl` references all
of them. Every `/ttl` path and `dns.rrsig/data` are volatile.

## Configuration

All keys live in `netray.toml` (reference: `crates/netray/netray.example.toml`). An absent
`[modules.dns]` runs the module on its defaults.

| Table | Key | Default |
|---|---|---|
| `[modules.dns]` | `deadline_ms` (the section window) | `2000` |
| | `servers` (empty: `dns.default_servers`) | `[]` |
| `[modules.dns.dns]` | `default_servers` | `["google"]` |
| | `allow_system_resolvers` | `true` |
| | `allow_arbitrary_servers` | `false` |
| `[modules.dns.limits]` | `max_timeout_secs` (at most 10) | `10` |
| | `max_servers` (at most 4) | `4` |
| | `per_target_per_minute`, `per_target_burst` (accepted, not read: runs are admitted by the caller) | `60`, `20` |
| `[modules.dns.circuit_breaker]` | `window_secs` | `60` |
| | `cooldown_secs` | `30` |
| | `failure_threshold` | `0.5` |
| | `min_requests` | `5` |
| `[api.dns]` | `per_ip_per_minute`, `per_ip_burst` | `120`, `64` |
| | `per_target_per_minute`, `per_target_burst` | `60`, `20` |
| | `global_per_minute`, `global_burst` | `1000`, `50` |
| | `per_ip_max_streams` | `10` |
| `[inspect.dns]` | `deadline_ms` | `30000` |
| | `max_concurrent` | `8` |

`[api.dns]` limits the API v1 DNS jobs; its `per_ip_burst` and `global_burst` must be at least 18,
the cost of the smallest authcompare job. Unknown keys are refused.

## Testing

From the repository root:

```sh
just check                                   # the gate plus the full Rust suite
cargo test -p netray-dns --test it <module>  # one integration test module
cargo test -p netray-dns --features testing --test <name>  # a test target that requires `testing`
```

## License

MIT, see [LICENSE](LICENSE).
