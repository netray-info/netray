# Spec: v2-tls-versions-coop

Status: Ready for Implementation
Created: 2026-10-20

## Goal

The TLS module answers, per port and address, whether a server still accepts SSLv3, TLS 1.0 or
TLS 1.1, whether it supports TLS 1.2 and TLS 1.3, and whether it accepts weak cipher suites
(RC4, 3DES, DES, export, NULL, anonymous; CBC; static RSA key exchange without forward secrecy).
It learns this from a fixed, small set of hand-built ClientHello probes classified on the server's
first answer, with no second TLS library. The results are evidence (per address, plus a per-port
summary that backs board G's "Protocols and features" grid) and feed two new graded TLS checks,
`tls.legacy_versions` and `tls.weak_suites`. Cross-Origin-Opener-Policy becomes the graded HTTP
check `http.coop`. Scoring profile `v4` weights the three new checks. Closes the data half of
canvas row 9.57 (TLS 1.0 to 1.3 and forward secrecy cells), the TLS inventory finding "neither V1
nor V2 enumerates protocol versions or cipher suites" (`sections/03-tls.md`) and the grading part
of inventory row 4.25 (COOP); CORP and COEP stay evidence
(`netray-ops/specs/netray/inventory/`, private).

## Non-goals

- No template, stylesheet or island change for the grid: board G's "Protocols and features" grid
  is lane A's; this feature delivers its data. The only template touch is guide prose for COOP.
- No second TLS library (OpenSSL, BoringSSL) and no full handshake on a probe: the connection
  closes after the server's first answer.
- No enumeration of every cipher suite: suites are probed by group, one probe per group.
- No TLS 1.3 suite probing: every TLS 1.3 suite is AEAD with forward secrecy.
- COEP and CORP stay ungraded (evidence `http.isolation`).
- No new volatile path; no change to the V1 wire (`InspectResponse` JSON) or the API v1 schema.

## Context and constraints

- The handshake is rustls (`crates/tls/src/tls/connect.rs`), which offers TLS 1.3 and 1.2 only.
  The negotiated version therefore tells TLS 1.3 support directly (a server that supports it picks
  it), and TLS 1.2 support when TLS 1.2 was negotiated.
- `routes::inspect_port` (`crates/tls/src/routes.rs`) runs, per address and under the handshake
  semaphore, `backend.handshake` and then the live OCSP check. The addresses it gets have passed
  `routes::filter_allowed` (the target policy) and the rate-limit selection; a probe connects only
  to such an address and port, which is the same address policy as `netray_common::fetch`'s pinned
  resolver for user-controlled hosts.
- `TlsBackend` (`crates/tls/src/backend.rs`) is the network seam; `StubTlsBackend`
  (`crates/tls/src/testing.rs`) serves module tests without network.
- Rule M as the operator set it for this feature (2026-10-20): a timeout or a reset is
  `unmeasured`, never "disabled". A weighted `unmeasured` check makes the TLS section incomplete
  (S13).
- Evidence rules (SDD §3.7, CLAUDE.md): no placeholders, a judged check references a block that
  holds its deciding fact, new internal data on V1 structs is `#[serde(skip)]`, block ids are
  `<module>.<name>[@<qualifier>]`, released keys are never reused.
- Profiles (SDD §3.8): a released profile is never edited; a change is a new file with a lock
  entry; every catalogue id has a weight in the current profile and vice versa; every stored run
  keeps its profile version and is re-scored with it; the diff splits a grade change across
  profile versions (`netray_engine::history::diff`). The fingerprint includes the profile version
  (SDD §4.2), so the first `v4` run of every domain is a new entry; new check ids between engine
  versions show as "added by release" (SDD §4.3).
- New check ids need catalogue texts (`crates/engine/catalogue/checks.toml`) and `CHECK_LABELS`/
  `CHECK_DESCRIPTIONS` in `crates/lens/frontend/src/lib/checkMeta.ts`
  (`tests/repo/test_checkmeta_catalogue.sh`); `crates/web/compare.toml` rows name check ids
  (`specs/rules/comparison-page-rules.md`). Severity follows the weight (>= 5 high, 3 to 4 medium,
  <= 2 low).
- Goldens: lens goldens (`tests/fixtures/contracts/lens-*.json`) regenerate with
  `UPDATE_GOLDEN=1` and need `ADLC-Test-Change`; protected tests counting 54 checks or 21 TLS
  checks need the trailer too.

## Requirements

1. `netray_tls::tls::probe::client_hello(kind, sni)` builds one TLS record holding one ClientHello
   for a probe kind: SSLv3 (client version 0x0300, record version 0x0300, no extensions), TLS 1.0
   (0x0301), TLS 1.1 (0x0302), TLS 1.2 (0x0303) and the three suite groups `insecure`, `cbc` and
   `static_rsa` (0x0303). Record version 0x0301 for every kind but SSLv3. It offers the kind's
   suites plus `TLS_EMPTY_RENEGOTIATION_INFO_SCSV`, never `TLS_FALLBACK_SCSV`, never the
   `supported_versions` extension; `server_name` only for a host name (not for an address or
   without SNI); `signature_algorithms` only at client version 0x0303.
2. `netray_tls::tls::probe::classify(kind, bytes, end)` reads the server's first answer: a
   ServerHello at the probed version is `accepted` (with version and suite); for a suite group, a
   ServerHello of any version up to TLS 1.2 choosing an offered suite is `accepted`. A ServerHello
   at another version is `refused` naming that version; a fatal handshake_failure, protocol_version
   or insufficient_security alert is `refused` naming the alert. A warning alert before the
   ServerHello is skipped; close_notify and any other alert are `unanswered`.
   A timeout, a reset, a close before any answer, a failed connect, a ServerHello choosing a suite
   that was not offered and bytes that are not a TLS record are `unanswered` naming why. A
   ServerHello split across records is read whole.
3. `netray_tls::tls::probe::probe_address(ip, port, sni, negotiated, timeout)` runs the SSLv3,
   TLS 1.0 and TLS 1.1 probes, the TLS 1.2 probe only when the handshake negotiated TLS 1.3, and
   the three suite-group probes, concurrently, each over its own TCP connection to `ip:port`
   (after the SMTP STARTTLS upgrade on 25 and 587, as the handshake), each bounded by `timeout`:
   at most seven connections per address and port. TLS 1.2 is `enabled` from the handshake when
   it negotiated TLS 1.2; TLS 1.3 is `enabled` when the handshake negotiated it and `disabled`
   otherwise.
4. `inspect_port` probes an address only after its handshake succeeded, through a new
   `TlsBackend::probe` (the live backend calls `probe_address`), concurrently with the live OCSP
   check and under the same handshake permit. The result is `IpInspectionResult.probes`, a
   `#[serde(skip)]` field: the V1 and API JSON stay byte-identical. `StubTlsBackend` answers no
   probes unless a test sets canned ones.
5. Each address with probes gets a fields block `tls.probes@<address>:<port>`: `ssl3`, `tls1_0`,
   `tls1_1`, `tls1_2`, `tls1_3` (`enabled` or `disabled`), `suites_insecure`, `suites_cbc`,
   `suites_static_rsa` (`accepted` or `refused`), and `<key>_answer` (text: the alert, the
   ServerHello's version and suite, or the handshake's version). An unanswered probe's keys are
   omitted.
6. A port with probes on at least one address gets a fields block `tls.protocols` (qualified
   `tls.protocols@<port>` when several ports are inspected): each version and suite key `enabled`/
   `disabled` or `accepted`/`refused` when every probed address agrees, `mixed` when they differ,
   omitted when any probed address left it unanswered; `forward_secrecy_all_suites` (bool) false
   when any address accepted a static-RSA suite, true when every probed address refused them,
   omitted otherwise. Both blocks are in the TLS register; neither has a volatile path.
7. `tls.legacy_versions`, per probed address: `fail` when SSLv3, TLS 1.0 or TLS 1.1 is enabled,
   naming the versions; `pass` when all three are disabled; else `unmeasured` naming the
   unanswered probes. Over addresses: any fail is fail, else any unmeasured is unmeasured, else
   pass. Without probes on any handshake it is `not_applicable` "not reported"; without a
   handshake it follows `tls_reachable` (S28).
8. `tls.weak_suites`, per probed address: `fail` when an insecure suite is accepted; `unmeasured`
   when the insecure probe is unanswered; `warn` when a CBC or static-RSA suite is accepted;
   `pass` when all three groups are refused; else `unmeasured`. Over addresses: fail, then
   unmeasured, then warn, then pass. Findings name the address, group and chosen suite. Without
   probes as requirement 7.
9. Both TLS checks reference the `tls.probes` blocks of the addresses at the reported status and
   the port's `tls.protocols`; an unmeasured check references nothing. Both are emitted in every
   run (23 TLS checks).
10. `http.coop`: `pass` for a Cross-Origin-Opener-Policy of `same-origin`,
    `same-origin-allow-popups` or `noopener-allow-popups` (the token before any `;`); `warn` for
    `unsafe-none`, any other value, an unreadable value or no header; the refused or unanswered
    HTTPS probe overrides it like the other response checks (S36: eleven response checks). It
    references `http.headers` and `http.isolation`. 13 HTTP checks.
11. The catalogue holds the three ids with title, finding, impact, fix hint, owner (`web_server`),
    severity (from the weight), source and guide (`/guide/tls-protocol`,
    `/guide/security-headers`); `checkMeta.ts` holds their labels and descriptions; the
    security-headers guide gains a COOP paragraph and fix line; `compare.toml` lists the TLS ids in
    its TLS-checks and cipher-suite rows and `http.coop` in its header row.
12. Profile `v4` (`crates/engine/profiles/v4.toml`, new lock entry) is v3 plus
    `tls.legacy_versions` 5, `tls.weak_suites` 3 and `http.coop` 1, with v3's section weights, hard
    fails and thresholds; `Profiles::current()` is v4; v3 stays loadable and its lock unchanged.
    The catalogue holds 57 checks (10 DNS, 23 TLS, 13 HTTP, 10 email, 1 IP).
13. SDD `specs/sdd/v2.md` states the probes, the blocks and keys, the three checks, profile v4
    with its weights and severities, the grade change (a decision row) and the counts, in the same
    commit as the code; `CHANGELOG.md` `[Unreleased]` names the grade change.

## Phase 1 — ClientHello probes

**Depends on:** none
**Requirements:** 1, 2, 3

New `crates/tls/src/tls/probe.rs`; tests in `crates/tls/tests/it/` against an in-process
responder on 127.0.0.1 that reads a ClientHello and answers canned bytes.

### Test Scenarios

GIVEN kind TLS 1.0 and SNI `example.com` WHEN the ClientHello is built THEN the record type is 22, record version 0x0301, handshake type 1, client version 0x0301, the server_name extension names example.com, and neither supported_versions nor signature_algorithms nor TLS_FALLBACK_SCSV is present.
GIVEN kind SSLv3 WHEN the ClientHello is built THEN record and client version are 0x0300 and the message carries no extensions.
GIVEN kind TLS 1.2 WHEN the ClientHello is built THEN client version is 0x0303 and signature_algorithms is present.
GIVEN SNI `192.0.2.1` or no SNI WHEN the ClientHello is built THEN no server_name extension is present.
GIVEN each suite group WHEN its ClientHello is built THEN it offers exactly the group's suites plus the renegotiation SCSV; the insecure group holds RC4, 3DES, DES, export, NULL and anonymous suites; the cbc group holds only ECDHE or DHE CBC suites; the static_rsa group holds only RSA key exchange suites.
GIVEN a ServerHello at TLS 1.0 with suite 0xC013 WHEN the TLS 1.0 probe classifies it THEN it is accepted with version TLSv1.0 and suite TLS_ECDHE_RSA_WITH_AES_128_CBC_SHA.
GIVEN a ServerHello at TLS 1.2 WHEN the TLS 1.1 probe classifies it THEN it is refused naming TLSv1.2.
GIVEN an alert record with description 70 WHEN classified THEN it is refused naming protocol_version.
GIVEN a ServerHello at TLS 1.0 choosing an insecure-group suite WHEN the insecure probe classifies it THEN it is accepted.
GIVEN a ServerHello choosing a suite the probe did not offer WHEN classified THEN it is unanswered.
GIVEN a ServerHello split over two records WHEN classified THEN it is read whole and accepted.
GIVEN an end of stream before any byte, a reset or the text `HTTP/1.1 400` WHEN classified THEN each is unanswered, never refused.
GIVEN a responder that accepts TLS 1.0 and 1.1 and alerts SSLv3 and every suite group, and a handshake that negotiated TLS 1.3 WHEN `probe_address` runs THEN SSLv3 is disabled, TLS 1.0 and 1.1 enabled, TLS 1.2 is probed, TLS 1.3 enabled from the handshake, and the responder saw seven connections.
GIVEN a handshake that negotiated TLS 1.2 WHEN `probe_address` runs THEN TLS 1.2 is enabled from the handshake, TLS 1.3 disabled, and the responder saw six connections.
GIVEN a responder that accepts the connection and never answers WHEN `probe_address` runs with a 200 ms timeout THEN every probed kind is unanswered (timed out) and the call returns within about one timeout.
GIVEN a responder that resets the connection after reading the ClientHello WHEN probed THEN the kind is unanswered (reset).

## Phase 2 — Probes in the inspection and the evidence

**Depends on:** Phase 1
**Requirements:** 4, 5, 6

`crates/tls/src/backend.rs` (`probe`), `crates/tls/src/routes.rs` (`inspect_port`),
`crates/tls/src/tls/mod.rs` (`probes` field), `crates/tls/src/testing.rs` (canned probes),
`crates/tls/src/module.rs` (blocks, register), SDD §3.7.

### Test Scenarios

GIVEN a stub backend with canned probes and one address WHEN the module runs THEN the outcome holds `tls.probes@<address>:443` with each answered key and its `_answer`, and `tls.protocols`, and the outcome validates against the register.
GIVEN canned probes where the TLS 1.0 probe is unanswered WHEN translated THEN `tls.probes` has no `tls1_0` and no `tls1_0_answer`, and `tls.protocols` has no `tls1_0`.
GIVEN two addresses, one with TLS 1.0 enabled and one disabled WHEN translated THEN `tls.protocols` says `tls1_0` mixed; GIVEN both disabled THEN disabled.
GIVEN one address accepting a static-RSA suite WHEN translated THEN `forward_secrecy_all_suites` is false; GIVEN all refusing THEN true; GIVEN one unanswered and none accepting THEN the key is absent.
GIVEN a stub backend without canned probes WHEN the module runs THEN no `tls.probes` and no `tls.protocols` block exists.
GIVEN a two-port inspection with probes WHEN `translate_ports` runs THEN each port's outcome holds `tls.protocols@<port>`.
GIVEN an address whose handshake failed WHEN the port is inspected THEN the backend's probe is not called for it; GIVEN a handshake that succeeded THEN it is called once with the negotiated version.
GIVEN an `IpInspectionResult` with probes WHEN serialised THEN its JSON has no probes key.
GIVEN the TLS module WHEN its volatile list is read THEN no path names `tls.probes` or `tls.protocols`.

## Phase 3 — Checks, catalogue and profile v4

**Depends on:** Phase 2
**Requirements:** 7, 8, 9, 10, 11, 12, 13

`crates/tls/src/module.rs` (two checks), `crates/http/src/module.rs` and
`crates/http/src/inspect/security.rs` (COOP), `crates/engine/catalogue/checks.toml`,
`crates/engine/profiles/v4.toml` and `lock.toml`, `crates/engine/src/profile.rs`,
`crates/lens/frontend/src/lib/checkMeta.ts`, `crates/web/compare.toml`, the security-headers
guide, `crates/web/tokens.json` (the spectrum pitch note), lens goldens, SDD §2, §3.7 to §3.9,
`CHANGELOG.md`.

### Test Scenarios

GIVEN one address with TLS 1.0 enabled WHEN translated THEN `tls.legacy_versions` is fail with a finding naming TLS 1.0 and references that address's `tls.probes` block and `tls.protocols`.
GIVEN SSLv3, TLS 1.0 and TLS 1.1 disabled on every address WHEN translated THEN `tls.legacy_versions` is pass.
GIVEN TLS 1.1 unanswered and the other two disabled WHEN translated THEN `tls.legacy_versions` is unmeasured naming the TLS 1.1 probe, references no block, and the TLS section is not scored.
GIVEN one address with TLS 1.0 enabled and another with TLS 1.1 unanswered WHEN translated THEN `tls.legacy_versions` is fail.
GIVEN an insecure suite accepted WHEN translated THEN `tls.weak_suites` is fail naming the group and suite.
GIVEN the insecure group refused and a CBC suite accepted WHEN translated THEN `tls.weak_suites` is warn; GIVEN a static-RSA suite accepted instead THEN warn.
GIVEN all three groups refused WHEN translated THEN `tls.weak_suites` is pass.
GIVEN the insecure probe unanswered and a CBC suite accepted WHEN translated THEN `tls.weak_suites` is unmeasured.
GIVEN handshakes without probes WHEN translated THEN both checks are not_applicable "not reported"; GIVEN no handshake with a refused connection THEN both are not_applicable "no TLS handshake".
GIVEN the TLS module WHEN its checks are listed THEN they are the 23 catalogue TLS ids.
GIVEN a response with `Cross-Origin-Opener-Policy: same-origin` WHEN translated THEN `http.coop` is pass referencing `http.headers` and `http.isolation`; GIVEN `same-origin-allow-popups; report-to="r"` THEN pass; GIVEN `unsafe-none` THEN warn; GIVEN no header THEN warn with a finding naming the missing header.
GIVEN a refused HTTPS probe WHEN translated THEN `http.coop` is fail "no HTTPS response"; GIVEN a timed-out HTTPS probe THEN unmeasured.
GIVEN the embedded catalogue WHEN read THEN it holds 57 ids, 23 TLS and 13 HTTP, and each new id has every text, owner web_server, and severity high, medium, low for weights 5, 3, 1.
GIVEN `Profiles::current()` WHEN read THEN it is v4, weights exactly the catalogue's ids with v3's weights plus the three new ones, and v3's section weights, hard fails and thresholds; GIVEN `Profiles::get(3)` THEN it loads and lacks the three ids; GIVEN the lock THEN v3 and v4 hashes match their files.
GIVEN a stored v3 run and a current v4 run of one domain WHEN diffed THEN the grade change carries the v3 run re-scored under v4.
GIVEN checkMeta.ts and the catalogue WHEN `test_checkmeta_catalogue.sh` runs THEN it passes; GIVEN compare.toml THEN its listed ids resolve.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0-7
  (operator, 2026-10-19).
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-19).
- Per-version verdicts (operator 2026-10-20 asked to decide warn or fail per version): SSLv3,
  TLS 1.0 and TLS 1.1 accepted are each `fail`. RFC 8996 (BCP 195, 2021) forbids TLS 1.0 and 1.1
  alike; Mozilla's intermediate configuration (v5.7) and BSI TR-02102-2 (2025) allow only TLS 1.2
  and 1.3; RFC 7568 forbids SSLv3. No version is a hard fail: a legacy version beside TLS 1.2 is
  a downgrade risk, not a broken site (implementer under the operator's autonomy mandate,
  2026-10-19).
- Weak suites: RC4 (RFC 7465), 3DES (Sweet32, NIST SP 800-131A rev. 2), DES, export, NULL and
  anonymous suites `fail`; CBC suites (non-AEAD; Mozilla intermediate excludes them, BSI still
  lists some ECDHE CBC) and static RSA key exchange (no forward secrecy; excluded by Mozilla
  intermediate and BSI) `warn` (implementer under the operator's autonomy mandate, 2026-10-19).
- Weights in v4: `tls.legacy_versions` 5 (as `tls.tls_version`), `tls.weak_suites` 3, `http.coop`
  1 (as the other single headers); section weights unchanged. They are re-weighted with v3's after
  V2 on measured data (SDD §3.9) (implementer under the operator's autonomy mandate, 2026-10-19).
- An end of stream before any answer is `unanswered`, like the timeout and the reset the operator
  named: a close carries no verdict on the version. Consequence: a server that closes or resets
  instead of sending an alert (often Windows SChannel) leaves `tls.legacy_versions` unmeasured
  unless another legacy version was accepted, and the TLS section incomplete (implementer under
  the operator's autonomy mandate, 2026-10-19).
- All probes of an address run concurrently, at most seven connections, each bounded by the
  handshake timeout, and alongside the live OCSP check: the inspection gets at most one probe
  timeout longer, which keeps the domain run inside the TLS window. Browsers open six parallel
  connections, so seven short ones are within what servers expect (implementer under the
  operator's autonomy mandate, 2026-10-19).
- Suite groups overlap where a suite belongs to two questions (RSA RC4 is insecure and static
  RSA): each probe answers "does the server accept any suite of this group", so
  `suites_static_rsa` alone decides forward secrecy across suites (implementer under the
  operator's autonomy mandate, 2026-10-19).
- Evidence per address always (`tls.probes@<address>:<port>`, like `tls.handshake`) plus one
  per-port summary (`tls.protocols`) whose `mixed` value shows where addresses differ; the grid
  reads the summary. Alternative "per-address blocks only when they differ" rejected: block ids
  would appear and vanish with agreement (implementer under the operator's autonomy mandate,
  2026-10-19).
- Probes travel as a `#[serde(skip)]` field: the V1 wire, the API v1 schema and their goldens
  stay unchanged; the data reaches users as evidence (implementer under the operator's autonomy
  mandate, 2026-10-19).
- The probe cost is not charged to the rate limiters: it is a fixed multiple (at most seven) of
  each charged handshake (implementer under the operator's autonomy mandate, 2026-10-19).
- COOP values: `same-origin`, `same-origin-allow-popups` and `noopener-allow-popups` (HTML
  standard) pass; `unsafe-none` warns as no isolation; COEP and CORP stay evidence as the
  operator decided (implementer under the operator's autonomy mandate, 2026-10-19).
- Grade change: every domain's grade may change once with v4. The first v4 run of each domain
  is a new history entry (the fingerprint includes the profile version); the diff shows the three
  checks as "added by release" (SDD §4.3). Amended after the final review: the grade split cannot
  attribute them to the profile, because the predecessor never measured them and re-scores under
  v4 to its v3 grade; their letter change shows on the domain step beside the added-by-release
  rows. Stated in SDD §2 and
  `CHANGELOG.md` (implementer under the operator's autonomy mandate, 2026-10-19).

- Amendment in Phase 1 (review): only a fatal handshake_failure, protocol_version or
  insufficient_security alert refuses a probe. A warning alert before the ServerHello (an
  unrecognized_name warning from an SNI callback) is skipped, and internal_error, decode_error,
  unrecognized_name or close_notify say nothing about the offered version or suite, so they leave
  the probe unanswered (Rule M) (implementer under the operator's autonomy mandate, 2026-10-19).

- Phase 3 review: COOP is judged in the HTTP module from the received value
  (`coop_verdict`), not by changing the V1 security report, so spectra's V1 wire keeps its
  presence-only COOP verdict (spec non-goal). The token before `;` is matched without trimming
  around it (a structured-field token), so `same-origin ;` warns as browsers ignore it; a second
  header line is not seen (deferred: the V1 report keeps the first line only) (implementer under
  the operator's autonomy mandate, 2026-10-19).
- Phase 3 fallout: the island test fixtures under `crates/web/islands/test/fixtures` and the
  catalogue positions in `crates/web/islands/test/issues.test.ts` are regenerated test data the
  new catalogue ids force; no island source changed. The acceptance history fixtures gain the
  three ids as `not_applicable` "not reported" (they stay v3 runs) because
  `history_fixtures` holds every catalogue id in each. The compare page's "Protocol versions"
  row now claims partial coverage through `tls.legacy_versions` and `tls.tls_version` (no SSLv2
  probe) (implementer under the operator's autonomy mandate, 2026-10-19).
- Lens goldens: `http.coop` warns in every fixture (the spectra fixture carries no COOP header),
  each score drops about 0.6 points and no letter changes; `tls.legacy_versions` and
  `tls.weak_suites` are "not reported" there (fixtures carry no probes) (implementer under the
  operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- Board G's grid, the per-address probe table and any other rendering: lane A.
- A configuration switch for probing, per-suite enumeration, TLS 1.3 suite or group probing,
  SSLv2 probing, renegotiation, compression or Heartbleed tests.
- Measuring the live grade shift: DNS is blocked locally; the shift is argued from the weights in
  SDD §3.9 and measured after deployment.
