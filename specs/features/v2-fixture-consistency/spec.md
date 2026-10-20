# Spec: v2-fixture-consistency

Status: Done
Created: 2026-10-20
Finished: 2026-10-20

## Goal

The seeded history fixtures under `tests/acceptance/fixtures/history/` (read by
`just acceptance-local`, `just design-check` and the live review instance) hold runs whose check
statuses contradict their own evidence: HTTP header checks pass while `http.headers` lacks the
header, DMARC warns "p=none" beside a `p=reject` record, `http.https_redirect` passes beside a
port-80 probe that answered 200 without a redirect, and the three shop runs share identical blocks
while their statuses differ. Found by v2-result-polish (PR #44, report.md "Deferred").

After this feature every fixture run is what the real modules and lens compute from the run's own
evidence, and a test recomputes it for every fixture and fails on any difference, so a
hand-edited status can not come back.

## Non-goals

- No production behaviour change: no check rule, register, block key, API or contract golden
  changes. The one production edit is a seam (TLS: the clock `translate` reads for the chain's
  `expired` key becomes a parameter of an internal function; the public `translate` keeps
  reading the wall clock).
- The canvas and the manifest's boards: unchanged unless a measured value legitimately moves.
- The V1 contract goldens under `tests/fixtures/contracts/`.

## Context and constraints

- Each module turns a measurement into a `SectionOutcome` (checks and blocks) through its own
  analysis and `translate`. The blocks hold most of that measurement; a few facts never reach a
  block (TLS chain trust from DER and the root store, the authoritative answers of each NS for
  `dns.ns_lame` and `dns.ns_delegation`, an MTA-STS policy's TLS certificate). For those the
  rebuild assumes the well-behaved world: a chain in issuer order that ends at a self-signed
  certificate is trusted and at an anchor; each NS answers authoritatively with the zone's SOA
  and NS set; the policy host serves the recorded status, content type and body over a
  certificate the rebuild accepts. A lone self-signed certificate is never trusted (the public
  root store would refuse it).
- What the rebuild can not reproduce without DER or a production change, and so no fixture holds:
  TLSA records (`tls.tlsa`, DANE needs the served certificates' DER) and ALPN that differs between
  addresses (`tls.alpn_consistency` lists the addresses in hash-map order, so its finding is not
  stable between calls); in DNS an NSEC next name with an escaped `\000` label (mhost's name
  parser refuses it); in email SPF `include:`, `redirect=` and `exists:` targets (their TXT and
  address records are not in the evidence), a BIMI logo fetch, and an MTA-STS fetch that failed
  other than "blocked". A missing email block is a failed lookup of its input, as a missing DNS
  block is (Rule M).
- Two checks read the wall clock: TLS (`expired` in `tls.chain`) and DNS (`dns.dnssec` over
  DNSKEY-covering RRSIGs). A fixture must give the same result on every day: TLS evaluates at
  the run's own measurement time (leaf `not_after` minus `days_remaining`); fixtures carry no
  RRSIG records (an empty `dns.rrsig` block, a definite NODATA).
- Fixtures stay in the `example.com` family and use documentation addresses only (192.0.2.0/24,
  198.51.100.0/24, 203.0.113.0/24, 2001:db8::/32).
- The boards' story stays: `shop.example.com` has three runs (14 days, 7 days, now) whose lines
  are fixed or new between them (DKIM fail to pass, DMARC fail to warn, MTA-STS warn to pass,
  HSTS pass to fail, expiry window warn to pass); `new.example.com` is a first visit graded A;
  `example.com` and `stale.example.com` are graded B; the long-domain fixture keeps four TLS
  addresses with three-item chains. Where consistency would change a status the story needs,
  the evidence changes, not the status.
- `engine_version` is the release that wrote a run; the comparison ignores it so a release bump
  does not fail the test.

## Requirements

### Rebuild from evidence

1. **HTTP.** `netray_http::testing::from_evidence(domain, blocks)` rebuilds the inspection's raw
   input from the `http.*` blocks (the HTTPS response's headers, status, version, final URL and
   redirect chain from `http.headers`, `http.server` and `http.redirects`; the port-80 probe from
   `http.port80` and `http.port80_probe`; the CORS probe's answer from `http.cors`) and returns
   `translate(assemble_response(...))`.
2. **TLS.** `netray_tls::testing::from_evidence(domain, blocks)` rebuilds the inspect response
   (per address the error or the certificate chain, handshake parameters, SCT count; CAA from
   `tls.caa` or the facts block `dns.caa`; ECH from the handshake or `dns.https`) and runs the
   module's own per-address assessment and translation, with the chain trust assumption of the
   constraints and the run's measurement time as the clock.
3. **DNS.** `netray_dns::testing::from_evidence(domain, blocks)` rebuilds one lookup per
   `dns.<type>` block (an empty block a definite NODATA, a missing block no answer), with DNSKEY
   key tags computed from the key data, runs the module's lints, the two NS checks over an
   authoritative stub and `translate`.
4. **Email.** `netray_email::testing::from_evidence(domain, blocks)` seeds an offline resolver from
   the `email.*` blocks (MX, MX addresses, the SPF, DMARC, TLS-RPT, BIMI and MTA-STS records,
   DMARC report authorisation, DKIM keys of the recorded type and size, TLSA, PTR, DNSBL
   listings, DNSKEY presence), serves a recorded MTA-STS policy from a loopback listener, runs the
   module's inspection and `translate`.
5. **IP.** `netray_ip::testing::from_evidence(domain, blocks)` rebuilds one lookup per
   `ip.addresses` row and returns `translate` over them, with the address total from the facts
   blocks `dns.a` and `dns.aaaa`.
6. **A fixpoint.** For each module, `from_evidence` over the blocks it returned returns the same
   outcome, byte for byte, on any day.

### Guard

7. **Every fixture is recomputed.** A lens integration test loads every fixture, gives lens one
   module per section that answers `from_evidence` over the fixture's blocks (or the section's
   recorded not-applicable or incomplete reason), runs lens's check and `build_run`, and fails
   naming the fixture and the first differing check or block when the result differs from the
   fixture's run in anything but `engine_version`.
8. **Regeneration.** With `UPDATE_FIXTURES=1` the same test rewrites each fixture's run with the
   recomputed one, keeping `domain`, `age_seconds` and `engine_version`.

### Fixtures

9. **Consistent fixtures.** Every fixture passes requirement 7. The statuses of the story in the
   constraints hold, the grades stay (new A, shop runs A, example.com and stale B,
   long domain A), and requirement 23 of v2-inspectors (every catalogue check once) and the
   design fixture checks (`tests/repo/test_design_fixtures.sh`) keep passing.
10. **The design gate.** `just design-check` exits 0 against the private manifest.

## Phases

## Phase 1 — HTTP, TLS and IP from evidence

**Depends on:** none
**Requirements:** 1, 2, 5, 6

Tests in `crates/lens/tests/it/` (lens links every module's `testing` feature).

### Test Scenarios

- GIVEN `http.headers` without `x-frame-options` WHEN HTTP `from_evidence` runs THEN
  `http.x_frame_options` is not pass; GIVEN the header `x-frame-options: DENY` THEN it passes.
- GIVEN `http.port80_probe` with status 200 and `redirects_to_https` false WHEN HTTP
  `from_evidence` runs THEN `http.https_redirect` fails; GIVEN a port-80 chain whose hop
  redirects to `https://` THEN it passes.
- GIVEN `http.headers` with `strict-transport-security: max-age=31536000` WHEN HTTP
  `from_evidence` runs THEN `http.hsts` passes and an `http.hsts` block holds `max_age`
  31536000.
- GIVEN the HTTP blocks `from_evidence` returned WHEN it runs over them again THEN the outcome is
  equal.
- GIVEN a TLS leaf with `days_remaining` 10 WHEN TLS `from_evidence` runs THEN
  `tls.expiry_window` warns; GIVEN 68 THEN it passes.
- GIVEN two TLS addresses whose leaf fingerprints differ WHEN TLS `from_evidence` runs THEN
  `tls.consistency` is not pass.
- GIVEN a TLS chain in issuer order ending in a self-signed root WHEN `from_evidence` runs THEN
  `tls.chain_trusted` and `tls.chain_complete` pass; GIVEN a leaf whose SANs do not cover the
  domain THEN `tls.hostname_match` fails.
- GIVEN a chain whose `not_after` lies before today and whose `days_remaining` says the run
  measured it valid WHEN TLS `from_evidence` runs THEN the chain's `expired` key reads false.
- GIVEN the TLS blocks `from_evidence` returned WHEN it runs over them again THEN the outcome is
  equal.
- GIVEN an `ip.addresses` row with `tor` true WHEN IP `from_evidence` runs THEN `ip.reputation`
  fails with the finding "<address>: tor"; GIVEN every flag false THEN it passes.
- GIVEN the IP blocks `from_evidence` returned WHEN it runs over them again THEN the outcome is
  equal.

## Phase 2 — DNS and email from evidence

**Depends on:** none
**Requirements:** 3, 4, 6

### Test Scenarios

- GIVEN `dns.caa` empty WHEN DNS `from_evidence` runs THEN `dns.caa` warns; GIVEN a CAA issue
  record and an issuewild record THEN it passes.
- GIVEN a KSK in `dns.dnskey` and a `dns.ds` record whose key tag is the KSK's WHEN DNS
  `from_evidence` runs THEN `dns.dnssec_rollover` passes; GIVEN a DS key tag no DNSKEY carries
  THEN it fails.
- GIVEN one NS record WHEN DNS `from_evidence` runs THEN `dns.ns` warns; GIVEN two THEN it
  passes.
- GIVEN the DNS blocks `from_evidence` returned WHEN it runs over them again THEN the outcome is
  equal.
- GIVEN `email.dmarc` with record `v=DMARC1; p=reject` WHEN email `from_evidence` runs THEN
  `email.dmarc` is not warn with a p=none finding; GIVEN `p=none` THEN it warns.
- GIVEN no `email.dkim` key WHEN email `from_evidence` runs THEN `email.dkim` is not pass; GIVEN
  one valid RSA 2048 key THEN it passes.
- GIVEN `email.mta_sts` with policy status 200 and an `enforce` policy listing the MX WHEN email
  `from_evidence` runs THEN `email.mta_sts` passes; GIVEN a `testing` policy THEN it warns.
- GIVEN the email blocks `from_evidence` returned WHEN it runs over them again THEN the outcome is
  equal.

## Phase 3 — Guard and consistent fixtures

**Depends on:** Phase 1, Phase 2
**Requirements:** 7, 8, 9, 10

### Test Scenarios

- GIVEN a fixture whose stored `email.dmarc` status differs from the recomputed one WHEN the guard
  runs THEN it fails naming the fixture and `email.dmarc`.
- GIVEN every committed fixture WHEN the guard runs THEN it passes.
- GIVEN `UPDATE_FIXTURES=1` WHEN the guard runs over a fixture THEN the rewritten file keeps its
  `domain`, `age_seconds` and `engine_version` and the guard then passes on it.
- GIVEN the shop fixtures WHEN read THEN run 1 has `email.dkim` fail, `email.dmarc` fail,
  `email.mta_sts` warn, `http.hsts` pass and `tls.expiry_window` warn; run 2 the same but
  `email.dmarc` warn; run 3 has `email.dkim`, `email.mta_sts` and `tls.expiry_window` pass,
  `email.dmarc` warn and `http.hsts` fail.
- GIVEN the fixtures WHEN read THEN `new.example.com`, the shop runs and the long domain are
  graded A, `example.com` and `stale.example.com` B.
- GIVEN the private manifest WHEN `just design-check` runs THEN it exits 0.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0–7 (operator, 2026-10-19).
- Fixtures are recomputed through the modules' own analysis from their own evidence (a fixpoint), not from separate source contracts: the V1 contract goldens carry their own verdicts and so would not prevent a contradiction, and rebuilding raw input from the blocks keeps the fixtures readable as they are (implementer under the operator's autonomy mandate, 2026-10-20).
- Facts no block holds are assumed well-behaved (chain trust, NS authority, the policy host's certificate): a fixture can not show what its evidence does not hold, and every story beat is reachable through evidence (implementer under the operator's autonomy mandate, 2026-10-20).
- The guard lives in lens's `tests/it`: only lens links every module and the scoring that sets the grade, so the comparison covers the whole run, grade included (implementer under the operator's autonomy mandate, 2026-10-20).
- Review of Phase 1: a lone self-signed certificate is not trusted by the rebuild; TLSA records and differing ALPN stay out of the fixtures (DANE needs DER; the ALPN finding's address order is a hash-map order, and fixing it would be a production change this feature excludes) (implementer under the operator's autonomy mandate, 2026-10-20).
- Review of Phase 2: a missing email block is rebuilt as a failed lookup (Rule M), as in DNS; the NSEC `\000` next name, SPF include trees, BIMI logo fetches and non-blocked MTA-STS fetch failures are outside the rebuild and stay out of the fixtures, which use NSEC next names the parser reads and `v=spf1 mx -all` (implementer under the operator's autonomy mandate, 2026-10-20).
- Review of Phase 3: the DNSBL blocks name the module's configured zones (zen.spamhaus.org, b.barracudacentral.org, bl.spamcop.net, dbl.spamhaus.org), an exception to the example.com rule: the rebuild queries the module's own zone list, the zones are public service names, not data about a party, and the contract goldens carry them already (implementer under the operator's autonomy mandate, 2026-10-20).
- Review of the range: a recorded run has no caller DKIM selectors, so the email rebuild passes none and the fixtures' keys sit under `default`, a selector the module probes itself; a lone self-signed certificate is untrusted but at an anchor (`tls.chain_complete` passes, as in the module); fixtures carry an empty `dns.rrsig` block. An incomplete section's reason is passed back wrapped by lens, which the guard does not unwrap: no fixture has one (out of scope) (implementer under the operator's autonomy mandate, 2026-10-20).
- Finish entries (CLAUDE.md bullets, the solutions note, the checks to build) written without an operator question under the mandate (implementer under the operator's autonomy mandate, 2026-10-20).

## Open decisions

None.

## Out of scope

- A consistency check of the V1 contract goldens.
- Fixtures for not-applicable or incomplete sections (none exist today; the guard passes their
  recorded reasons through).
