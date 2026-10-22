# Security policy

## Reporting a vulnerability

Report a vulnerability privately through GitHub: open the repository's **Security** tab and choose
**Report a vulnerability**. Do not open a public issue, pull request or discussion for it.

Please include what is affected, how to reproduce it, and what an attacker gains.

## Supported versions

Only the latest release is supported. Fixes ship as a new release; older releases are not patched.

## Response

netray is maintained by one person. Reports are answered on a best-effort basis, usually within a
week. You will hear when the report is confirmed and when a fixed release is out.

## Scope

This policy covers the whole repository: the `netray` binary and every crate in it.

In scope:

- `netray serve` on every host it answers (the apex, `dns.`, `tls.`, `http.`, `ip.`, `email.`,
  `lens.`, `mta-sts.`): the result, history, entry and inspector pages, the badges and OG cards,
  and API v1 (`/api/v1`).
- The outbound fetches and probes it makes on a visitor's behalf: the target policy (which
  addresses it refuses to contact), the fetch policy and its redirect handling, and the DNS,
  TLS, HTTP and mail lookups of the modules.
- Rate limits, run admission and budgets, and the client-address extraction behind them
  (trusted proxies).
- The configuration loader and the release image.

Out of scope:

- Vulnerabilities in the upstream [mhost](https://github.com/lukaspustina/mhost) library: report
  those to the mhost repository.
- DNS provider infrastructure and resolver behaviour.
- The third-party data sources the IP module reads (MaxMind GeoLite2, the threat and range
  lists): report their content to their publishers.
