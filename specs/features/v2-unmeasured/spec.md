# Spec: v2 unmeasured

Status: Ready for Implementation
Created: 2026-10-12

## Goal

No module reports `not_applicable`, `fail`, `warn` or `pass` for a check whose answer it never obtained. Errors and timeouts become `unmeasured`. A weighted `unmeasured` check makes its section `incomplete`, so the run gets no letter (S13).

A domain whose A and AAAA lookups both get a definite negative has `not_applicable` IP, HTTP and TLS sections, and gets a letter from DNS and email (S31).

Each of the 54 catalogue checks has one contract case for its worst measured status (`fail`, or `warn` where the check has no fail branch) and one for `unmeasured` (V1.13 as amended by A3; `ip.reputation` has no error path). Lens carries `unmeasured` end to end: wire value `"unmeasured"`, shown as "could not check".

The three R4 defects are fixed:
- `email.mx` is N/A on a working MX;
- an `http.https_redirect` port-80 timeout fails;
- a refused DNSBL zone gives N/A.

## Non-goals

- Changing weights or the profile. `profiles/v3.toml` and its lock hash stay byte-identical; only the engine's rule for `unmeasured` changes.
- tlsight's own HSTS probe (`crates/tls/src/quality/http.rs:128-135`). It runs in every TLS module run (`crates/tls/src/module.rs:125` → `routes.rs:947`), but HSTS is not one of the TLS module's checks, so its `Skip` never reaches the grade. `http.hsts` is the http module's (Phase 4). See AMENDMENT A1.
- The V2 frontend's rendering of unmeasured bands (§6 of the SDD). This feature gives the current lens frontend one label only.
- Evidence references (`v2-evidence`, V1.14).
- Retrying lookups or probes to turn an `unmeasured` into a measurement.

## Context and constraints

Facts are from the tree at 4e37fe0, unless a line says otherwise.

**Engine**
- `Status` has `Pass, Warn, Fail, NotApplicable, NotTested, Unmeasured` (`crates/model/src/lib.rs:117-124`).
- Scoring skips `NotApplicable | NotTested | Unmeasured` (`crates/engine/src/scoring.rs:54`). A section with one weighted `Unmeasured` check and one measured check therefore stays complete.
- A section is incomplete when its possible is 0 (`scoring.rs:57-59`, :108), or when its outcome is `Incomplete` or `TimedOut` (:92).
- `crates/engine/tests/score.rs:112` asserts `(Pass, Unmeasured) → complete` (:129). This test changes with R1 and needs `ADLC-Test-Change`.
- `engine::run` (`crates/engine/src/lib.rs:195-264`):
  - On a failed resolve, a module that needs addresses gets `Incomplete("address resolution failed")` (:238-243), and a module that wants facts gets `Facts::default()` (:233).
  - A module that overruns gets `TimedOut` (:245-247).
- No module emits `Status::Unmeasured` today. `crates/tls/src/tls/mod.rs:67-72` maps error codes to it, but `check_status` (`crates/tls/src/module.rs:339-350`) only passes `NotTested` through.

**Facts and S31**
- `fact_lookups` (`crates/dns/src/module.rs:153-193`) queries A, AAAA, MX, CAA, NS and HTTPS. It returns `Err` only when every resolver fails at transport level or a query is refused by policy or rate limit.
- A per-lookup `Error` or `NxDomain` becomes an empty vector in `facts_from_lookups` (:244-290). A failed lookup and an empty answer are therefore the same in `Facts`.
- The workspace pins `mhost = "=0.12.0"` (`Cargo.toml:26`). In 0.12.0, SERVFAIL, REFUSED and NOTIMP fold into `LookupResult::NxDomain` without an rcode.
- mhost 0.12.1 is on crates.io (max_version 0.12.1, checked 2026-10-12). It adds `NxDomain::response_code()`, `is_referral()` and `is_definite()` (mhost checkout `src/resolver/lookup.rs:592-614`). `is_definite` is true for NXDOMAIN and for a NOERROR that is not a referral (NODATA), and false otherwise.
- Which modules use the facts:
  - IP needs addresses.
  - TLS wants facts, but uses them only for CAA and HTTPS and resolves its own IPs.
  - HTTP uses neither and resolves through `tokio::net::lookup_host` (`crates/http/src/input.rs:39-53`).
- Lens turns an IP section with empty A/AAAA facts into `NoDnsResults` (`crates/lens/src/modules.rs:186-192`), and that into `Incomplete` (`crates/lens/src/score.rs:173-175`).
- The golden `tests/fixtures/contracts/lens-full-no-address-records.json` holds http, ip and tls as `"error"`, `complete:false`, `grade:"incomplete"`. It changes with R5 and needs `ADLC-Test-Change`.

**Lens round trip**
- Lens maps `NotApplicable | NotTested | Unmeasured → CheckVerdict::Skip` (`crates/lens/src/modules.rs:172`), and back `Skip → NotApplicable` (`crates/lens/src/score.rs:35`). The status is lost before lens scores.
- The wire value `"skip"` is written at `crates/lens/src/routes.rs:821` and :850, and in `crates/lens/src/snapshot/types.rs:165`. Snapshots count skips at `snapshot/types.rs:103`.
- The API page documents verdicts at `site/api/lens.html:206-221`.
- The frontend:
  - `Verdict = 'pass'|'warn'|'fail'|'error'|'skip'` (`crates/lens/frontend/src/lib/types.ts:1`).
  - `skip` renders as "–" (`components/CheckList.tsx:11`).
  - The incomplete note is "incomplete — some checks failed to run", shown when a section is `'error'` (`Summary.tsx:87-88`, :172-174).

**Error paths that report a measured status today**

| Module | Path | Status today |
|---|---|---|
| DNS | lookup error → empty `Lookups` → `NotFound` (`crates/dns/src/api/check.rs:719-790`, `module.rs:481-485`) | `Fail` |
| DNS | stream timeout, no lint (`api/check.rs:565-575`, `module.rs:400-409`) | `NotApplicable` "not reported" |
| DNS | NS unresolved, or no answer to SOA (`check.rs:1058`, :1102, :1164) | `Warn` |
| TLS | handshake failed on every address, timeout included (`quality/checks.rs:290-297`) | `Fail` |
| TLS | the other 20 checks after no handshake (`module.rs:259-272`) | `NotApplicable` |
| TLS | CAA lookup error → no records (`dns/caa.rs:121-123`) | `Pass` |
| TLS | HTTPS-record lookup error (`dns/https_record.rs:13-19`) | not advertised |
| TLS | TLSA lookup error (`dns/tlsa.rs:40-56`) | `NotApplicable` |
| HTTP | port-80 timeout (`module.rs:140-144`; `request.rs:117-131` sets `final_url` to the http URL) | `Fail` |
| HTTP | HTTPS probe error → empty headers | `hsts` `Fail` (`inspect/security.rs:77`); `csp` and the four presence headers `Warn` (`csp.rs:22`, `security.rs:137`) |
| HTTP | HTTPS probe error, `caching` | `Warn` |
| HTTP | CORS probe error (a separate probe, `inspect/mod.rs:90`, :205) → no ACAO | `cors` `Pass` (`cors.rs:43`) |
| HTTP | HTTPS redirect to an address the fetch policy blocks (`inspect/request.rs:107`) → empty headers | as for a probe error |
| Email | `DnsLookup` swallows every lookup error (`crates/email/src/dns/mod.rs:25-31`), so an MX error reads "no MX" (`module.rs:266-267`) | 7 checks `NotApplicable` |
| Email | TXT error → `spf`/`dmarc` absent | `Fail` |
| Email | MTA-STS policy fetch timeout (`checks/mta_sts.rs:178-189`) | `Fail` |
| Email | BIMI logo fetch timeout (`checks/bimi.rs:82-91`) | `Warn` |
| Email | DNSBL zone timeout or policy response (`checks/dnsbl.rs:99-107`, :123-142) | `NotApplicable` |
| Email | DNSBL SERVFAIL (`dnsbl.rs:109-111`, :144-150) | `Pass` "clean" |
| Email | `email.mx` with Info sub-checks only (`checks/mx.rs:114-143`, `module.rs:433-437`) | `NotApplicable` |

**Tests**
- No per-check contract test with an error case exists. `crates/*/tests/module_v3.rs` assert that each catalogue ID is emitted exactly once, and assert statuses for their golden scenarios, e.g. `tls_reachable` Pass with one refused address (`crates/tls/tests/module_v3.rs:69`, :133), `http.csp` Warn (`crates/http/tests/module_v3.rs:79`), `email.mx` Pass (`crates/email/tests/module_v3.rs:136`). Those stay green.
- 17 checks have no fail branch: `dns.caa`, `dns.ttl`; `tls.san_quality`, `ct_logged`, `ocsp_stapled`, `consistency`, `alpn_consistency`, `ech_advertised`; `http.cookie_secure`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`, `deprecated_headers`, `info_leakage`, `caching`; `email.fcrdns`.
- `ip.reputation` reads in-memory lists (`get_ifconfig` with `skip_dns: true`, `crates/ip/src/module.rs:80-83`) and has no error path.
- HTTP probes one address, the first from the system resolver (`crates/http/src/input.rs:70`). TLS inspects up to 10 addresses from its own resolver (`crates/tls/src/routes.rs:651`).
- The MTA-STS and BIMI fetches resolve through `netray_common::fetch`, which returns an empty set for any failed resolution (`crates/common/src/fetch.rs:30`): NXDOMAIN and SERVFAIL look the same.
- `CLAUDE.md:61` states "a category with only Info sub-checks is not applicable to lens"; requirement 14 narrows it.
- The catalogue counts 54 checks: dns 10, tls 21, http 12, email 10, ip 1 (`crates/engine/catalogue/checks.toml`).

**Rules**
- The gate is `just adlc-verify`, and it runs offline. Contract cases use fixtures or injected failures, never the network.
- A changed protected test needs `ADLC-Test-Change: <path> -- <reason>`.
- No real domains other than netray.info and example.com.
- A catalogue text describes every warn and fail branch (`CLAUDE.md:63`). `unmeasured` is neither. The only `checks.toml` text that changes is `dns.ns_lame`'s finding (`checks.toml:76`, "or reached"), because requirement 6 removes that branch. `profiles/v3.toml` is untouched, so the lock hash holds.

## Requirements

### Rule M (referenced by 6, 8–15; owned by no single phase)

**Measured or unmeasured.**
   - A check is `unmeasured` when its answer was not obtained: a timeout, a resolver SERVFAIL, REFUSED or timeout, or a redirect the fetch policy refuses to follow.
   - It is `fail` (or `warn`) only when the target answered and the answer is wrong: a refused connection, a reset, a TLS alert, an HTTP error status, NXDOMAIN, or NODATA.
   - It is `not_applicable` only by design, with a reason.
   - **Precedence.** A check whose input is ruled out by a definite answer elsewhere (no A/AAAA, no MX or Null MX, no handshake behind a `fail`) is `not_applicable`, whatever its own lookups returned.
   - Every `unmeasured` result carries at least one finding that names the cause: what timed out or errored, and where.

### Phase 1 — Engine, lens, frontend

1. The engine scores a weighted `unmeasured` check as making its section `incomplete`. A run with such a section gets no letter. `not_applicable` and `not_tested` keep their current treatment.
2. Lens carries `Status::Unmeasured` to scoring and to the wire as verdict `"unmeasured"`, in SSE, snapshots and MCP output. It is no longer folded into `skip`.
   - A stored snapshot with `"skip"` still reads as before.
   - `site/api/lens.html` lists the new value.
3. The lens frontend shows an `unmeasured` check with the label "could not check", distinct from "–". The incomplete note in `Summary.tsx` also appears when a section is incomplete because of an `unmeasured` check.

### Phase 2 — Facts, S31, DNS

4. The workspace uses mhost 0.12.1. `Facts` records, for A and AAAA, whether the answer is definite: every resolver asked returned NXDOMAIN or a non-referral NODATA (`NxDomain::is_definite()`).
5. When A and AAAA are both definite negatives, the IP, HTTP and TLS sections are `not_applicable` with reason "no A or AAAA record", and the run is complete with a letter from DNS and email. When either lookup has a SERVFAIL, timeout, referral or other non-definite answer, those sections stay `incomplete`.
6. DNS checks follow Rule M:
   - A lookup error or stream timeout is `unmeasured`, never `fail` through `NotFound` and never "not reported".
   - `dns.ns_lame` and `dns.ns_delegation` judge the nameservers that answered (worst-of). A nameserver that times out is excluded and named in a finding. When no nameserver answers, or the NS addresses cannot be resolved because of a resolver error, the check is `unmeasured`. An NS host name that is NXDOMAIN stays a measured defect.
   - `dns.ns_lame`'s catalogue finding drops "or reached".
7. Contract cases for the 10 DNS checks (requirement 16's shape).

### Phase 3 — TLS

8. `tls.tls_reachable` keeps its catalogue meaning ("no address completed a handshake", `checks.toml:143`), per address under Rule M:
   - `pass` when any address completed a handshake. Addresses that timed out are named in a finding.
   - `fail` when none did and at least one answered with a refusal, a reset or a TLS alert.
   - `unmeasured` when none did and none answered (every tested address timed out). Addresses not tested from here are excluded; when every address is not tested from here, the check stays `not_tested`.
9. Every other per-address TLS check is the worst over the addresses that completed a handshake. When none did, they follow `tls_reachable`: `not_applicable` "no TLS handshake" behind `fail`, `unmeasured` behind `unmeasured`, `not_tested` behind `not_tested`. This takes precedence over requirement 10.
10. Lookup and comparison errors:
    - A CAA, HTTPS-record or TLSA lookup error makes `tls.caa_compliant`, `tls.ech_advertised` or `tls.dane_valid` `unmeasured`, instead of `pass`, "not advertised" or `not_applicable`.
    - `tls.consistency` and `tls.alpn_consistency`, when at least one address handshook but fewer than two did because the others timed out, are `unmeasured`.
    - Fewer than two addresses, or fewer than two handshakes because the others refused or alerted, stays `not_applicable`.
11. Contract cases for the 21 TLS checks (requirement 16's shape).

### Phase 4 — HTTP

12. Probe failures:
    - `http.https_redirect`: a port-80 probe that times out is `unmeasured`. A refused connection, a reset or a response that does not redirect stays `fail`.
    - When the HTTPS probe times out, or a redirect is cut by the fetch policy, the ten checks that read its response are `unmeasured`: `hsts`, `csp`, `x_frame_options`, `x_content_type_options`, `referrer_policy`, `permissions_policy`, `cookie_secure`, `caching`, `deprecated_headers`, `info_leakage`. The finding names the timeout or the blocked hop.
    - When the HTTPS connection is refused, reset or alerted, those ten are `not_applicable` "no HTTPS response". The defect is graded by `tls.tls_reachable` when TLS probes that address. HTTP probes one address, so there is no per-address aggregation.
    - `http.cors` follows its own probe: a CORS probe timeout is `unmeasured`, whatever the HTTPS probe got.
13. Contract cases for the 12 HTTP checks (requirement 16's shape).

### Phase 5 — Email, IP

14. Email follows Rule M. `DnsLookup` surfaces lookup errors instead of returning an empty answer.
    - **MX:**
      - An MX lookup error makes `email.mx` and its six dependents `unmeasured`.
      - A definite empty answer stays `not_applicable` "no MX", and so does a Null MX. By the precedence in Rule M, this holds for the six dependents even when their own lookups errored.
      - `email.mx` with at least one MX host and no warn or fail sub-check is `pass`, and its Info sub-checks (`single_mx`, `no_ipv6`) become findings.
      - An "absent" Info elsewhere (BIMI, TLS-RPT, MTA-STS, DANE) stays `not_applicable`. This narrows the rule in `CLAUDE.md:61` for `email.mx` and `email.dnsbl` only.
    - **TXT:** a TXT lookup error makes `email.spf`, `email.dmarc`, `email.dkim`, `email.bimi`, `email.tls_rpt` or `email.mta_sts` `unmeasured`. For `email.dkim`, that holds when every selector lookup errored.
    - **Fetches:**
      - An MTA-STS policy fetch or a BIMI logo fetch that times out is `unmeasured`.
      - Every other fetch error (host does not resolve, refused, TLS error, HTTP error status) stays `fail` or `warn` as today. The fetch cannot tell NXDOMAIN from SERVFAIL (`crates/common/src/fetch.rs:30`).
    - **DNSBL:**
      - `email.dnsbl` is `fail` when any zone that answered lists an address.
      - It is `pass` when at least one zone answered and none listed. The zones that refused, timed out or SERVFAILed are named in a finding.
      - It is `unmeasured` when no zone answered.
      - A policy response (127.255.255.x) counts as "not answered", never as "clean".
    - **Beacon pipeline timeout:** the section stays `TimedOut` (incomplete). A fixture asserts it (V1.16).
15. Contract cases for the 10 email checks and for `ip.reputation` (requirement 16's shape). `ip.reputation` has no error path (`crates/ip/src/module.rs:80-83`): it gets the measured row only, and the guard lists it as the one exception with that reason. "No public addresses" because every address is private stays a `not_applicable` section.

### Contract shape (Phases 2–5)

16. Each module has one contract test file with a table of two rows per catalogue check of that module: its worst measured status (`fail`, or `warn` for the 17 checks without a fail branch) and `unmeasured`.
    - Each row drives the module through a fixture or an injected failure, without network. It asserts the status, and for `unmeasured` a non-empty finding.
    - A guard test in the same file fails when the table's check IDs differ from the module's catalogue IDs, apart from the exceptions listed in the file with their reason (only `ip.reputation`'s `unmeasured` row).
    - Any further check that cannot reach a row is recorded in the feature report as an AMENDMENT and asked, never silently dropped.

### Prose (after Phase 5)

17. `CHANGELOG.md` `[Unreleased]`:
    - states the grade change: unmeasured makes a run incomplete, and S31 gives mail-only domains a letter;
    - lists the new wire value `"unmeasured"`;
    - notes the three R4 defects as fixed.

    `CLAUDE.md:61` states the narrowed Info rule.

    `crates/lens/README.md` and the docs that list statuses name `unmeasured`.

## Phase 1 — Engine, lens, frontend

**Depends on:** none
**Requirements:** 1, 2, 3

The work is in `crates/engine/src/scoring.rs`, the lens round trip (`modules.rs`, `score.rs`, `routes.rs`, `snapshot/types.rs`), the lens frontend (`types.ts`, `CheckList.tsx`, `Summary.tsx`) and `site/api/lens.html`. It also changes the test `score.rs:112`.

### Test Scenarios

- GIVEN a DNS section with `spf` pass and `dmarc` unmeasured, both weighted, WHEN scored, THEN the section is incomplete and the run has grade `incomplete`.
- GIVEN the same section with `dmarc` `not_applicable`, WHEN scored, THEN the section is complete, possible is spf's weight, and the run has a letter.
- GIVEN the same section with `dmarc` `not_tested`, WHEN scored, THEN the section is complete.
- GIVEN an engine run whose email module emits `email.dnsbl` `unmeasured`, WHEN lens streams it, THEN the SSE check carries `"verdict": "unmeasured"` and the summary grade is `incomplete`.
- GIVEN the same run with `email.dnsbl` `not_applicable`, WHEN lens streams it, THEN the verdict is `"skip"`.
- GIVEN a stored snapshot with `"verdict": "skip"`, WHEN read, THEN it loads unchanged.
- GIVEN a check with verdict `unmeasured`, WHEN the frontend renders the check list, THEN it shows "could not check", and the summary shows the incomplete note.
- GIVEN a check with verdict `skip`, WHEN rendered, THEN it shows "–", and no incomplete note appears on its account.

## Phase 2 — Facts, S31, DNS

**Depends on:** Phase 1
**Requirements:** 4, 5, 6, 7

The work is in `Cargo.toml` (mhost 0.12.1), `crates/model` or `crates/engine` (`Facts` definiteness), `crates/dns/src/module.rs` (`facts_from_lookups`, lint mapping), `crates/dns/src/api/check.rs` (lookup errors, NS checks), the engine and lens handling of no-address sections, and the golden `lens-full-no-address-records.json`.

### Test Scenarios

- GIVEN A and AAAA both NODATA from every resolver, WHEN the run completes, THEN IP, HTTP and TLS are `not_applicable` "no A or AAAA record", and the run has a letter from DNS and email.
- GIVEN A NODATA and AAAA SERVFAIL, WHEN the run completes, THEN IP, HTTP and TLS are incomplete and the grade is `incomplete`.
- GIVEN A NXDOMAIN and AAAA timeout, WHEN the run completes, THEN the grade is `incomplete`.
- GIVEN A NODATA from one resolver and a referral from another, WHEN the run completes, THEN the grade is `incomplete`.
- GIVEN a DNSKEY lookup that SERVFAILs, WHEN the DNS module runs, THEN `dns.dnskey_algorithm` is `unmeasured` with a finding naming the lookup.
- GIVEN a definite empty DNSKEY answer, WHEN the DNS module runs, THEN `dns.dnskey_algorithm` is `not_applicable`.
- GIVEN two nameservers where one answers SOA with authority and one times out, WHEN `dns.ns_lame` runs, THEN it is `pass` with a finding naming the silent nameserver.
- GIVEN two nameservers that both time out, WHEN `dns.ns_lame` runs, THEN it is `unmeasured`.
- GIVEN the DNS contract table, WHEN its guard runs, THEN its IDs equal the 10 DNS catalogue IDs.

## Phase 3 — TLS

**Depends on:** Phase 1
**Requirements:** 8, 9, 10, 11

The work is in `crates/tls/src/module.rs` (`check_status`, the no-handshake mapping, aggregation), `crates/tls/src/quality/checks.rs` (`tls_reachable`, the consistency checks) and `crates/tls/src/dns/{caa,https_record,tlsa}.rs` (surfacing errors).

### Test Scenarios

- GIVEN two addresses that both time out on 443, WHEN TLS runs, THEN `tls.tls_reachable` is `unmeasured`, and the 20 other checks are `unmeasured`.
- GIVEN two addresses that both refuse 443, WHEN TLS runs, THEN `tls.tls_reachable` is `fail`, and the other 20 are `not_applicable` "no TLS handshake".
- GIVEN one address that times out and one that handshakes with a valid chain, WHEN TLS runs, THEN `tls.tls_reachable` is `pass` with a finding naming the timed-out address.
- GIVEN one address that refuses and one that handshakes with a valid chain, WHEN TLS runs, THEN `tls.tls_reachable` is `pass` (unchanged, `crates/tls/tests/module_v3.rs:133`).
- GIVEN one address that refuses and one that times out, WHEN TLS runs, THEN `tls.tls_reachable` is `fail`, and the other 20 checks, `consistency` included, are `not_applicable` "no TLS handshake".
- GIVEN one address not tested from here and one that times out, WHEN TLS runs, THEN `tls.tls_reachable` is `unmeasured`.
- GIVEN every address not tested from here, WHEN TLS runs, THEN `tls.tls_reachable` stays `not_tested`.
- GIVEN a CAA lookup that SERVFAILs, WHEN TLS runs, THEN `tls.caa_compliant` is `unmeasured`, not `pass`.
- GIVEN a definite empty CAA answer, WHEN TLS runs, THEN `tls.caa_compliant` is `pass`.
- GIVEN one address, WHEN TLS runs, THEN `tls.consistency` is `not_applicable`.
- GIVEN two addresses where one handshook and one timed out, WHEN TLS runs, THEN `tls.consistency` is `unmeasured`.
- GIVEN two addresses where one handshook and one refused, WHEN TLS runs, THEN `tls.consistency` is `not_applicable`.
- GIVEN the TLS contract table, WHEN its guard runs, THEN its IDs equal the 21 TLS catalogue IDs.

## Phase 4 — HTTP

**Depends on:** Phase 1
**Requirements:** 12, 13

The work is in `crates/http/src/module.rs`, `crates/http/src/inspect/{mod,request,security,cors}.rs` and `crates/http/src/quality/checks.rs`: the probe outcome carries "no response" as its own case.

### Test Scenarios

- GIVEN a port-80 probe that times out, WHEN HTTP runs, THEN `http.https_redirect` is `unmeasured` with a finding naming port 80.
- GIVEN a port-80 connection refused, WHEN HTTP runs, THEN `http.https_redirect` is `fail`.
- GIVEN a port-80 connection reset after accept, WHEN HTTP runs, THEN `http.https_redirect` is `fail`.
- GIVEN a port-80 response 200 without a redirect, WHEN HTTP runs, THEN `http.https_redirect` is `fail`.
- GIVEN an HTTPS probe that times out and a CORS probe that answers with `ACAO: *`, WHEN HTTP runs, THEN the ten response checks are `unmeasured` and `http.cors` is `warn`.
- GIVEN an HTTPS probe that answers 200 and a CORS probe that times out, WHEN HTTP runs, THEN `http.cors` is `unmeasured` and the response checks are measured.
- GIVEN an HTTPS probe whose 301 points at an address the fetch policy blocks, WHEN HTTP runs, THEN the ten response checks are `unmeasured` with a finding naming the blocked hop.
- GIVEN an HTTPS response without CSP, WHEN HTTP runs, THEN `http.csp` is `warn` (unchanged, `crates/http/tests/module_v3.rs:79`).
- GIVEN an HTTPS response without HSTS, WHEN HTTP runs, THEN `http.hsts` is `fail`.
- GIVEN an HTTPS connection refused, WHEN HTTP runs, THEN the response checks are `not_applicable` "no HTTPS response".
- GIVEN the HTTP contract table, WHEN its guard runs, THEN its IDs equal the 12 HTTP catalogue IDs.

## Phase 5 — Email, IP

**Depends on:** Phase 1
**Requirements:** 14, 15

The work is in `crates/email/src/dns/mod.rs` (`DnsLookup` errors), `crates/email/src/module.rs` (`fold_category`, `beacon_verdict`, the MX-dependent mapping), `crates/email/src/checks/{mx,dnsbl,mta_sts,bimi,spf,dmarc,dkim,tls_rpt,dane,fcrdns}.rs` and `crates/ip/src/module.rs`.

### Test Scenarios

- GIVEN an MX lookup that SERVFAILs, WHEN email runs, THEN `email.mx` and its six dependents are `unmeasured`.
- GIVEN a definite empty MX answer, WHEN email runs, THEN they are `not_applicable` "no MX".
- GIVEN a definite empty MX answer and a TLS-RPT TXT lookup that SERVFAILs, WHEN email runs, THEN `email.tls_rpt` is `not_applicable` "no MX".
- GIVEN one MX host with two addresses and no IPv6, WHEN email runs, THEN `email.mx` is `pass` with findings for `single_mx` and `no_ipv6`.
- GIVEN no BIMI record, WHEN email runs, THEN `email.bimi` stays `not_applicable`.
- GIVEN three DNSBL zones where one lists an address and two time out, WHEN email runs, THEN `email.dnsbl` is `fail`.
- GIVEN three zones where one answers clean and two refuse, WHEN email runs, THEN `email.dnsbl` is `pass` with a finding naming the two zones.
- GIVEN every zone refusing or SERVFAILing, WHEN email runs, THEN `email.dnsbl` is `unmeasured`.
- GIVEN an MTA-STS policy fetch that times out, WHEN email runs, THEN `email.mta_sts` is `unmeasured`.
- GIVEN an MTA-STS policy fetch with HTTP 404, WHEN email runs, THEN `email.mta_sts` is `fail`.
- GIVEN an `_mta-sts` record whose policy host does not resolve, WHEN email runs, THEN `email.mta_sts` is `fail` (unchanged).
- GIVEN a TXT lookup that times out, WHEN email runs, THEN `email.spf` is `unmeasured`.
- GIVEN no SPF record, WHEN email runs, THEN `email.spf` is `fail`.
- GIVEN the beacon pipeline timeout, WHEN email runs, THEN the section is `TimedOut` and the run is incomplete.
- GIVEN only private addresses, WHEN IP runs, THEN the section is `not_applicable` "no public addresses".
- GIVEN the email and IP contract tables, WHEN their guards run, THEN their IDs equal the 10 email and 1 IP catalogue IDs, with `ip.reputation`'s `unmeasured` row as the listed exception.

## Decision log

- All error paths in five modules plus a contract table per check (V1.13 verbatim) over only the named cases plus R4 and S31: a fail or pass from an error is the same defect wherever it sits (operator, 2026-10-12).
- `email.dnsbl` unmeasured only when no zone answered, over any zone failing: a single zone refusing public resolvers is common and would cost the letter (operator, 2026-10-12).
- Lens frontend shows "could not check" in this feature, over engine/API only: otherwise a run shows incomplete with no visible cause (operator, 2026-10-12).
- A 443 timeout on every address is `unmeasured`, over `fail`: P10 and R4 treat a timeout as no measurement. Refused, reset or alert stay `fail` (operator, 2026-10-12).
- Per-address checks count the measured addresses (worst-of), and the check is unmeasured only when all are, over any unmeasured address making the check unmeasured. This is the same logic as DNSBL and `not_tested` (operator, 2026-10-12).
- A check missing its input follows the input's cause: `unmeasured` behind an error, `not_applicable` behind a fail or by design. The alternative was always `unmeasured`, which would turn a broken handshake's F into no letter, against the hard fail (operator, 2026-10-12).
- Wire value `"unmeasured"` over `"skip"` plus a flag: one field carries one status, the same word as the V2 model (operator, 2026-10-12).
- The refused connection on port 80 stays `fail`, like refused on 443. This follows from the 443 decision: the target answered.
- V1.13's `fail` row is the worst measured status (`warn` for the 17 checks without a fail branch), and `ip.reputation` has no `unmeasured` row, over adding fail branches. Adding them would be a grade change far beyond unmeasured (operator, 2026-10-12; AMENDMENT A3).
- Only a timeout makes an MTA-STS or BIMI fetch `unmeasured`, over extending `netray_common::fetch` to tell NXDOMAIN from SERVFAIL. A missing policy host stays visible as `fail`; the price is that a policy-host SERVFAIL also reads `fail` (operator, 2026-10-12).
- An HTTPS redirect the fetch policy blocks makes the response checks `unmeasured` with a finding, over grading the last received response. The final page was never received (operator, 2026-10-12).
- A nameserver that times out while others answer is excluded with a finding (worst-of over answering nameservers), over counting it as lame. A timeout is no answer; `ns_lame`'s finding drops "or reached" (operator, 2026-10-12).
- `tls.tls_reachable` stays best-of (pass when any address handshakes), as its catalogue text says; Rule M decides only between `fail` and `unmeasured` when none did. Derived from the reader's finding against `crates/tls/tests/module_v3.rs:133`.

## Open decisions

None.

### Amendments

- **A1.** V1.16 names "tlsight's HSTS timeout" (`crates/tls/src/quality/http.rs:133`). Every TLS module run reaches it (`module.rs:125` → `routes.rs:947`), but `hsts` is not among the TLS module's checks (`crates/tls/src/module.rs:24-47`), so its `Skip` never reaches the grade. The V2 check `http.hsts` belongs to the http module and is covered by requirement 12. tlsight's own probe stays as it is.
- **A3.** V1.13 asks for a `fail` case per check. 17 checks have no fail branch, and `ip.reputation` has no error path. The contract rows use the worst measured status instead, and `ip.reputation` has no `unmeasured` row. To be passed to the planning session.
- **A2.** S32 says Phase 1b splits V1 `Skip`. The tree shows more error paths that report `fail`, `warn` or `pass`, not `Skip` (see the Context table). Rule M covers those as well.

## Out of scope

- The V2 frontend's unmeasured band and spectrum line: `specs/sdd/v2.md` §6 (planning repo), V2 frontend features.
- The deferred v2-check-fixes items (AIA incomplete-branch URL, IpCard "hostname mismatch", repeated ACAO, ACAC case, `[modules.dns.backends.ip]`): `specs/features/v2-check-fixes/report.md`.
- Re-weighting after V2: SDD §3.9.
