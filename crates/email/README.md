# netray-email

The email module of `netray serve` (V1 codename beacon): a library crate with no binary of its own.
Given a domain, it inspects the domain's mail posture over DNS, plus the HTTPS fetches of the
MTA-STS policy and the BIMI logo.

## What it provides

- `EmailModule`: the V2 engine module (`netray_engine::Module`, protocol `Email`). It runs the
  category checks in-process and `module::translate` maps their results onto the `email.*` checks
  and evidence blocks (`module::REGISTER`).
- `EmailInspector` and the `job` types: the inspection netray-web runs for `POST /api/v1/email/inspect`
  and the `/inspect/email` page.

## Check ids

`EmailModule::checks()` emits the ten ids in `module::BUCKETED`, each with its texts in
`crates/engine/catalogue/checks.toml`:

`email.mx`, `email.spf`, `email.dkim`, `email.dmarc`, `email.mta_sts`, `email.tls_rpt`,
`email.bimi`, `email.dane`, `email.dnsbl`, `email.fcrdns`.

The inspection also measures DNSSEC (DNSKEY presence) and cross-validation; neither is an email
check (`module::EXCLUDED`): DNSSEC is scored in the DNS section, and each cross-validation sub-check
is routed to the email check it concerns (`module::route_cross_validation`).

## Configuration

All keys live in `netray.toml` (`crates/netray/netray.example.toml`); every table is optional and
falls back to the defaults below.

`[modules.email]` (`ModuleConfig`, `src/config.rs`; `deadline_ms` is the section window, read by
`crates/netray/src/config.rs`):

| Key | Default |
|---|---|
| `deadline_ms` | `2000` |
| `dns.resolvers` | `["cloudflare"]` |
| `dns.timeout_ms` | `5000` |
| `dnsbl.zones` | `["zen.spamhaus.org", "b.barracudacentral.org", "bl.spamcop.net", "dbl.spamhaus.org"]` |
| `dnsbl.timeout_ms` | `2000` |
| `dnsbl.resolvers` | `["system"]` |
| `http.timeout_ms` | `10000` |
| `dkim.max_user_selectors` | `5` |
| `inspections.max_concurrent` | `16` (1 up to tokio's semaphore maximum) |

`[api.email]` (`ApiEmailConfig`, `crates/web/src/api/config.rs`), the per-client limits of
`POST /api/v1/email/inspect`; each must be non-zero:

| Key | Default |
|---|---|
| `per_ip_per_minute` | `10` |
| `per_ip_burst` | `10` |
| `max_concurrent_inspections` | `16` |

`[inspect.email]` (`InspectLimits`, `crates/web/src/pages/inspect.rs`), the `/inspect/email` page's
budget; each must be at least 1:

| Key | Default |
|---|---|
| `deadline_ms` | `35000` (past the inspection's own 30 s cap) |
| `max_concurrent` | `8` |

## Testing

From the repository root: `just adlc-verify` (the gate) or `just check` (adds the full Rust suite).
For this crate alone:

- `cargo test -p netray-email --test it <module>` runs one module of the merged integration binary
  (`tests/it/`).
- The tests declared with `required-features = ["testing"]` in `Cargo.toml` need
  `--features testing`, e.g. `cargo test -p netray-email --features testing --test evidence`.
