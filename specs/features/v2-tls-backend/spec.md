# Spec: v2-tls-backend

Status: Done
Created: 2026-10-19
Finished: 2026-10-19

## Goal

The backend half of inventory theme E (TLS depth) holds: the TLS evidence carries the certificate
and handshake details V1 showed (serial, SHA-1, policy, self-signed and expired flags, OCSP URL,
SNI, STARTTLS, the stapled OCSP status and freshness, the live OCSP answer), the TLS inspector job
inspects and reports every listed port with a per-port outcome (checks and blocks) and the
run's warnings and skipped addresses, and `POST /api/v1/tls/inspect` accepts `ports`. Closes
inventory rows 3.3, 3.9, 3.16, 3.19 (data), 3.23, 3.25 and 3.44, and the data half of 3.5 and 3.24
(`netray-ops/specs/netray/inventory/sections/03-tls.md`, private).

## Non-goals

- No template, stylesheet or island change: port tabs, per-port bands, the validity bar, copy
  buttons, crt.sh links, banners, presets, history and shortcuts are the UI half (lane A).
- No change to the domain run's port: `TlsModule::run` judges port 443 only (SDD §3.5 "The domain
  check inspects port 443").
- No new check id, no scoring change: the live OCSP answer is evidence, not a verdict.
- No CRL checking (neither V1 nor V2 has it).
- No change to the V1 wire (`InspectResponse` JSON) or its goldens.

## Context and constraints

- The inspection core already inspects every parsed port: `routes::inspect` spawns one
  `inspect_port` per port and returns `InspectResponse.ports` sorted by port
  (`crates/tls/src/routes.rs` around line 702). Only the V2 translation drops them:
  `translate_with_facts` picks port 443, else the first (`crates/tls/src/module.rs:283`).
- The inspector job (`crates/web/src/api/jobs.rs` `start_tls`) runs `inspect_parsed` over every
  parsed port and renders `module::translate`, one part labelled `handshake` plus the band.
  `admit_tls` charges `ports.len()` on the shared `[api.tls]` limiter; `inspect_parsed` charges
  ports x addresses. The parser does not deduplicate ports (`crates/tls/src/input.rs`
  `parse_ports`), so `host:443,443` is charged and inspected twice.
- `limits.max_ports` defaults to 7, hard cap 7 (`crates/tls/src/config.rs`); V1 prod used 5 (row
  3.3). The limit stays config-driven.
- `POST /api/v1/tls/inspect` deserialises `TlsInspectRequest {host, port}` with
  `deny_unknown_fields` (`crates/tls/src/inspector.rs`), so `ports` is refused today; the port in
  `host` is ignored. SDD §7.1 documents `{host, port}`.
- The data already exists on the V1 structs: `CertInfo` has `serial`, `fingerprint_sha1`,
  `cert_policy` (`EV`, `OV`, `DV` or `unknown`), `is_self_signed`, `is_expired`, `ocsp_url`
  (`crates/tls/src/tls/chain.rs`); `TlsParams` has `sni`, `starttls`, `ocsp {stapled, status,
  this_update, next_update}` and `ocsp_live` (`crates/tls/src/tls/params.rs`). The live OCSP check
  runs per address in `inspect_port` (`routes.rs:1095`) and returns status `unknown` both when
  the responder answered "unknown" and when the fetch failed or was refused
  (`crates/tls/src/tls/ocsp.rs` `check_live_ocsp_with`).
- V1's crt.sh link is `https://crt.sh/?q=<fingerprint_sha256 without colons>`
  (`crates/tls/frontend/src/components/CertDetail.tsx:298`): `fingerprint_sha256` is already in
  `tls.chain`.
- `InspectResponse.warnings` and `skipped_ips` exist on the wire; the translation drops them.
- Evidence rules (SDD §3.7, CLAUDE.md): blocks never hold placeholders (unknowns are omitted);
  Rule M: a failed lookup is never an answer; new internal data on V1 structs is a
  `#[serde(skip)]` field; register keys are released once shipped; block ids are
  `<module>.<name>[@<qualifier>]` with a port allowed as qualifier.
- SDD §3.7 already lists the stapled OCSP response's `this_update` and `next_update` as TLS
  volatile fields. A new volatile path changes every stored fingerprint (CLAUDE.md).
- Goldens: `tests/fixtures/contracts/lens-*.json` pin lens's result built from
  `tlsight-inspect.json`; new chain and handshake fields rewrite them (`UPDATE_GOLDEN=1`, with
  `ADLC-Test-Change`). The OpenAPI golden `tests/fixtures/contracts/api-v1.openapi.json` changes
  with the request schema.
- Gate: `just adlc-verify`; Rust module tests run with `--features testing` where required.

## Requirements

1. Each item of a `tls.chain` block carries, when known: `serial` (text), `fingerprint_sha1`
   (text), `policy` (text, `EV`, `OV` or `DV`), `self_signed` (bool), `expired` (bool) and
   `ocsp_url` (text). An empty serial or fingerprint, a policy of `unknown` and an absent OCSP URL
   are omitted, never published as a placeholder.
2. Each `tls.handshake` block carries `sni` (text) when an SNI was sent and `starttls` (text, the
   protocol) when the connection was upgraded with STARTTLS; neither appears otherwise.
3. Each `tls.handshake` block carries the stapled OCSP response's `ocsp_status` (text) and its
   freshness `ocsp_this_update` and `ocsp_next_update` (time) when the staple holds them; without a
   staple only `ocsp_stapled` false remains.
4. Each `tls.handshake` block carries the live OCSP answer as `ocsp_live_status` (text: `good`,
   `revoked` or `unknown`), with `ocsp_live_revoked_at` (time) and `ocsp_live_reason` (text) for a
   revocation, only when the responder answered. A failed, timed-out or policy-refused responder
   fetch publishes no live key (Rule M); the V1 wire stays byte-identical (the marker is a
   `#[serde(skip)]` field on `OcspRevocationResult`).
5. The TLS register declares every new key with its type, and `tls.handshake/ocsp_this_update`,
   `tls.handshake/ocsp_next_update`, `tls.handshake/ocsp_live_status`,
   `tls.handshake/ocsp_live_revoked_at` and `tls.handshake/ocsp_live_reason` are volatile.
6. `netray_tls::module::translate_ports(resp)` returns one outcome per inspected port, in port
   order, each the full set of 21 `tls.*` checks judged on that port alone with that port's
   blocks. With one port its outcome equals `translate(resp)` apart from `tls.notices` (requirement 7). With more than one port the
   port-wide blocks are qualified by the port (`tls.addresses@<port>`, `tls.ct@<port>`), and the
   host-wide `tls.caa` block appears once, in the band port's outcome (443, else the lowest port). OCSP stapling, CAA and DANE are judged on the port itself, not from the response's summary, and a TLSA lookup error leaves a port whose TLSA records answered measured.
7. `translate_ports` adds a `tls.notices` fields block (`warning` text, repeated;
   `skipped_address` addr, repeated) to the band port's outcome when the response carries
   warnings or skipped addresses, and no such block otherwise. `translate` and the domain run never
   emit it.
8. The TLS inspector job streams one part per inspected port. With one port the part keeps its
   label `handshake`; with more it is labelled `port <n>`. Each part holds that port's blocks; the
   band shows the checks of port 443, else the lowest port, as today.
9. The TLS inspector's entry points deduplicate the parsed ports, keeping first occurrence order,
   before charging and inspecting, so every limiter charge counts only the ports inspected.
10. An explore link derived from a port-qualified `tls.addresses@<port>` block names that port
    (`<address>:<port>`); an unqualified block keeps `:443`.
11. `POST /api/v1/tls/inspect` accepts `ports` (array of 1 to `limits.max_ports` ports, each
    1-65535) as an alternative to `port`; the answer's `ports` holds one result per distinct port.
    A body with both `port` and `ports`, an empty `ports`, more than `max_ports` ports or a port 0
    is refused with 400 and the API's error format. A body with neither inspects port 443 as today.
12. SDD `specs/sdd/v2.md` states the new keys, the per-port inspector outcome, the `tls.notices`
    block and the `ports` body in the same commit as the code.

## Phase 1 — Certificate, handshake and OCSP detail in the evidence

**Depends on:** none
**Requirements:** 1, 2, 3, 4, 5

`crates/tls/src/module.rs` (`cert_fields`, `handshake_fields`, `REGISTER`, `VOLATILE`),
`crates/tls/src/tls/ocsp.rs` (answered marker on `OcspRevocationResult`), SDD §3.7 tls row. Lens
goldens regenerate.

### Test Scenarios

GIVEN the fixture leaf with serial `01:02:03`, SHA-1 `CC:DD`, policy `DV`, OCSP URL WHEN translated THEN its `tls.chain` item holds `serial`, `fingerprint_sha1`, `policy` DV, `self_signed` false, `expired` false and `ocsp_url`.
GIVEN a leaf with policy `unknown`, an empty serial and no OCSP URL WHEN translated THEN its chain item holds no `policy`, no `serial` and no `ocsp_url`, and the block validates.
GIVEN a handshake with SNI `example.com` and STARTTLS `smtp` WHEN translated THEN `tls.handshake` holds `sni` example.com and `starttls` smtp.
GIVEN an IP-target handshake without SNI and without STARTTLS WHEN translated THEN `tls.handshake` holds neither `sni` nor `starttls`.
GIVEN a staple with status good, this_update and next_update WHEN translated THEN `tls.handshake` holds `ocsp_status` good and both times as RFC 3339 times.
GIVEN no staple WHEN translated THEN `tls.handshake` holds `ocsp_stapled` false and no other `ocsp_` key from the staple.
GIVEN a responder that answered revoked with a time and reason WHEN translated THEN `tls.handshake` holds `ocsp_live_status` revoked, `ocsp_live_revoked_at` and `ocsp_live_reason`.
GIVEN a live OCSP result from a failed or refused fetch (status unknown, not answered) WHEN translated THEN `tls.handshake` holds no `ocsp_live_` key.
GIVEN a responder that answered unknown WHEN translated THEN `ocsp_live_status` is unknown.
GIVEN an `OcspRevocationResult` with the answered marker set WHEN serialised THEN the JSON has exactly the V1 keys.
GIVEN the TLS module WHEN its volatile list is read THEN it holds the five new OCSP paths beside the existing ones; GIVEN `tls.chain/serial` THEN it is not volatile.
GIVEN every new key WHEN the register is read THEN each is declared with the type of requirements 1-4, and the fixture evidence validates.

## Phase 2 — Multi-port inspector, notices and API ports

**Depends on:** Phase 1
**Requirements:** 6, 7, 8, 9, 10, 11, 12

`crates/tls/src/module.rs` (`translate_ports`, port-qualified ids, `tls.notices` in the register),
`crates/tls/src/inspector.rs` (`ports` field, deduplication), `crates/web/src/api/jobs.rs` (one part
per port), `crates/web/src/explore.rs` (port from the qualifier), the OpenAPI golden, SDD §3.7,
§6.2, §7.1.

### Test Scenarios

GIVEN a response with ports 443 and 465 WHEN `translate_ports` runs THEN it returns two outcomes, for 443 then 465, each with 21 checks and blocks `tls.addresses@443` and `tls.addresses@465` respectively, and each validates with the evidence rule holding.
GIVEN port 465 failing its handshake while 443 passes WHEN `translate_ports` runs THEN 465's `tls.tls_reachable` is fail and 443's is pass.
GIVEN a response with one port and no warnings WHEN `translate_ports` runs THEN it returns one outcome equal to `translate`, with the unqualified `tls.addresses`.
GIVEN a two-port response with CAA records WHEN `translate_ports` runs THEN `tls.caa` is in the band port's outcome only.
GIVEN a response with a warning and two skipped addresses WHEN `translate_ports` runs THEN the band port's outcome holds `tls.notices` with the warning and both addresses; GIVEN neither THEN no outcome holds `tls.notices`; GIVEN the same response WHEN `translate` runs THEN no `tls.notices`.
GIVEN the inspector job for `example.com:443,465` WHEN started THEN it has two parts labelled `port 443` and `port 465`; GIVEN `example.com` THEN one part labelled `handshake`.
GIVEN `example.com:443,443,465` WHEN the inspector parses it THEN the ports are 443 then 465, and the stub backend records handshakes on two ports per address.
GIVEN a block `tls.addresses@465` WHEN explore links are derived THEN each links `<address>:465`; GIVEN `tls.addresses` THEN `:443`.
GIVEN `POST /api/v1/tls/inspect` with `{"host":"example.com","ports":[443,465]}` WHEN answered THEN 200 with two port results 443 and 465.
GIVEN a body with `port` and `ports` WHEN posted THEN 400; GIVEN `ports: []` THEN 400; GIVEN more than `max_ports` ports THEN 400; GIVEN `ports: [0]` THEN 400.
GIVEN `{"host":"example.com"}` WHEN posted THEN one port result, 443; GIVEN `{"host":"example.com","port":25}` THEN one result, 25.
GIVEN the OpenAPI document WHEN generated THEN `TlsInspectRequest` lists `ports` and the contract golden matches.

## Decision log

- Spec drafted and approved by the implementer under the operator's mandate for features 0-7
  (operator, 2026-10-19).
- Independent reading of the Draft skipped: the review step reads the whole range against the spec
  (implementer under the operator's autonomy mandate, 2026-10-19).
- Per-port outcomes as one `SectionOutcome` per port over a combined section: the 21 check ids are
  unique per outcome, and the UI half renders one band per port from them (implementer under the
  operator's autonomy mandate, 2026-10-19).
- The inspector page keeps one band (443, else the lowest port) and gets one part per port; per-port
  bands need template changes and belong to lane A (implementer under the operator's autonomy
  mandate, 2026-10-19).
- Port-qualified `tls.addresses@<port>` and `tls.ct@<port>` only with more than one port: single
  port and domain-run ids, goldens and stored fingerprints stay unchanged (implementer under the
  operator's autonomy mandate, 2026-10-19).
- `tls.notices` in the inspector only, not in the domain run: skipped addresses rotate with the
  resolver and would need a whole-block volatile path, which changes every stored fingerprint for
  little value on a recorded run (implementer under the operator's autonomy mandate, 2026-10-19).
- Live OCSP keys volatile: a flapping responder would otherwise record a changed entry each time
  it times out; a revocation in practice comes with a certificate change, which is compared
  (implementer under the operator's autonomy mandate, 2026-10-19).
- New chain and handshake keys also in the domain run: the evidence page shows them; the next run
  of each domain records the added keys as a change, accepted (implementer under the operator's
  autonomy mandate, 2026-10-19).
- Deduplicate ports over refusing duplicates: V1 inspected a duplicate twice; deduplication charges
  only what is inspected (implementer under the operator's autonomy mandate, 2026-10-19).
- `port` and `ports` exclusive over `ports` silently overriding `port`: a body naming both is
  ambiguous (implementer under the operator's autonomy mandate, 2026-10-19).
- Port limit stays `limits.max_ports` (default 7) over a fixed 5: config-driven like V1, prod sets
  its value (implementer under the operator's autonomy mandate, 2026-10-19).
- crt.sh needs no new field: V1 builds the link from `fingerprint_sha256`, already in `tls.chain`
  (implementer under the operator's autonomy mandate, 2026-10-19).

- Amendment in Phase 2: requirement 6's one-port equality excludes `tls.notices`, which requirement 7 adds to the band port's outcome also with one port (an IP target always carries the no-SNI warning) (implementer under the operator's autonomy mandate, 2026-10-19).
- Review of Phase 2: requirement 6 per port means OCSP, CAA and DANE come from the port itself (the summary judges the first handshake over all ports), a host-wide TLSA error spares a port whose records answered, and the host-wide blocks sit in the band port's outcome so the band keeps referencing `tls.caa` (implementer under the operator's autonomy mandate, 2026-10-19).
- Finish: three CLAUDE.md bullets (per-port summary, STARTTLS label and day truncation, inspector part order in tests); no solutions note, no check to build beyond the report's list (implementer under the operator's autonomy mandate, 2026-10-19).

## Open decisions

None.

## Out of scope

- Port tabs, per-port verdict display, the validity bar, copy buttons, crt.sh links, banners,
  presets: lane A, `crates/web/templates`, `crates/web/static`, `crates/web/islands`.
- IP enrichment badges (3.32), cross-links (3.33), total duration and counts (3.10): other themes
  in `v2-parity.md`.
