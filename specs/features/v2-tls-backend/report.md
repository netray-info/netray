# Report: v2-tls-backend

## Phase 1 — Certificate, handshake and OCSP detail in the evidence

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R1: chain items carry serial, fingerprint_sha1, policy, self_signed, expired, ocsp_url when known; unknowns omitted | green | crates/tls/tests/evidence_cert_detail.rs |
| C2 | R2: handshake carries sni and starttls only when present | green | crates/tls/tests/evidence_handshake_detail.rs |
| C3 | R3: handshake carries ocsp_status, ocsp_this_update, ocsp_next_update from the staple | green | crates/tls/tests/evidence_handshake_detail.rs |
| C4 | R4: handshake carries ocsp_live_status (+revoked_at, reason) only when the responder answered; V1 wire unchanged | green | crates/tls/tests/evidence_ocsp_live.rs |
| C5 | R5: register declares the new keys; the five OCSP paths are volatile | green | crates/tls/tests/evidence_detail_register.rs |
| C6 | fixture leaf: serial, fingerprint_sha1, policy DV, self_signed false, expired false, ocsp_url | green | crates/tls/tests/evidence_cert_detail.rs |
| C7 | policy unknown, empty serial, no OCSP URL: keys omitted, block validates | green | crates/tls/tests/evidence_cert_detail.rs |
| C8 | SNI example.com and STARTTLS smtp appear in tls.handshake | green | crates/tls/tests/evidence_handshake_detail.rs |
| C9 | IP target without SNI or STARTTLS: neither key | already_implemented | crates/tls/tests/evidence_handshake_detail.rs |
| C10 | staple good with this/next update: ocsp_status and both times | green | crates/tls/tests/evidence_handshake_detail.rs |
| C11 | no staple: ocsp_stapled false, no other staple key | already_implemented | crates/tls/tests/evidence_handshake_detail.rs |
| C12 | responder revoked: status, revoked_at, reason | green | crates/tls/tests/evidence_ocsp_live.rs |
| C13 | failed or refused fetch: no ocsp_live_ key | green | crates/tls/tests/evidence_ocsp_live.rs |
| C14 | responder unknown: ocsp_live_status unknown | green | crates/tls/tests/evidence_ocsp_live.rs |
| C15 | answered marker set: JSON has exactly the V1 keys | green | crates/tls/tests/evidence_ocsp_live.rs |
| C16 | volatile list holds the five OCSP paths; tls.chain/serial not volatile | green | crates/tls/tests/evidence_detail_register.rs |
| C17 | every new key registered with its type; fixture evidence validates | green | crates/tls/tests/evidence_detail_register.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 1 | sonnet | 41378 | 45 |

### Review

- BLOCKER fixed: `expired` copied `is_expired`, which truncates to whole days; now also true when `not_after` is past the clock (`crates/tls/src/module.rs` `cert_fields`).
- BLOCKER fixed: `starttls` was the port's label also for IMAP and FTP, which are never upgraded (`connect.rs:201`); only `smtp` is published.
- DEFERRED: the live OCSP request takes `raw_certs[1]` as the issuer unchecked and never compares the response's CertID; a stale intermediate can yield a responder `unknown` for a good leaf (`routes.rs:1102`, predates this feature).
- NIT: no test drives `answered` through a real successful OCSP response; the evidence tests build the marker by hand.
- Lens goldens `tests/fixtures/contracts/lens-full-*.json` regenerated (new chain keys); `crates/tls/tests/evidence.rs` volatile list extended.

### Behavioural verification

skipped: no new entry point; the evidence reaches the inspector page and the domain run through the existing translate path, covered by the lens full-output goldens. A live run from a worktree build has no outbound network (reference_new_binary_no_network) and DNS is blocked locally.

## Phase 2 — Multi-port inspector, notices and API ports

### Criteria

| id | criterion | status | test file |
|---|---|---|---|
| C1 | R6: translate_ports returns one outcome per port in port order, 21 checks each, port-qualified addresses and ct with more than one port, tls.caa once in the first | green | crates/tls/tests/translate_ports.rs |
| C2 | R7: tls.notices (warning, skipped_address) in the first outcome only when warnings or skipped addresses exist; never from translate | green | crates/tls/tests/translate_ports.rs |
| C3 | R8: the inspector job streams one part per port, labelled handshake for one port and port n for more; band unchanged | green | crates/web/tests/inspect_tls_ports.rs |
| C4 | R9: inspector entry points deduplicate ports in first-occurrence order before charging and inspecting | green | crates/web/tests/inspect_tls_ports.rs |
| C5 | R10: explore links of tls.addresses@port name that port; unqualified keeps 443 | green | crates/web/tests/explore_tls_port.rs |
| C6 | R11: POST /api/v1/tls/inspect accepts ports; refuses both, empty, too many, port 0; neither gives 443 | green | crates/web/tests/api_tls_ports.rs |
| C7 | R12: SDD states keys, per-port outcome, tls.notices and ports body | test-unwritable | (prose; checked in review) |
| C8 | two ports 443 and 465: two outcomes with tls.addresses@443 and @465, each validates with the evidence rule | green | crates/tls/tests/translate_ports.rs |
| C9 | 465 handshake fails while 443 passes: 465 tls_reachable fail, 443 pass | green | crates/tls/tests/translate_ports.rs |
| C10 | one port: one outcome equal to translate with unqualified tls.addresses | green | crates/tls/tests/translate_ports.rs |
| C11 | two ports with CAA records: tls.caa in the first outcome only | green | crates/tls/tests/translate_ports.rs |
| C12 | warning and two skipped addresses: first outcome holds tls.notices; neither: none; translate: none | green | crates/tls/tests/translate_ports.rs |
| C13 | job for example.com:443,465 has parts port 443 and port 465; example.com one part handshake | green | crates/web/tests/inspect_tls_ports.rs |
| C14 | example.com:443,443,465 parses to 443 then 465; two handshakes per address | green | crates/web/tests/inspect_tls_ports.rs |
| C15 | tls.addresses@465 links address:465; tls.addresses links :443 | green | crates/web/tests/explore_tls_port.rs |
| C16 | POST ports [443,465]: 200 with two port results | green | crates/web/tests/api_tls_ports.rs |
| C17 | POST port and ports, ports [], too many ports, ports [0]: each 400 | green | crates/web/tests/api_tls_ports.rs |
| C18 | POST without port: one result 443; port 25: one result 25 | already_implemented | crates/web/tests/api_tls_ports.rs |
| C19 | OpenAPI TlsInspectRequest lists ports and the contract golden matches | green | crates/web/tests/api_tls_ports.rs |

### Runs

| group | coder runs | green by | tokens | seconds |
|---|---|---|---|---|
| G1 | 2 | sonnet | 41184 | 62 |
| G2 | 3 | sonnet | 36315 | 104 |
| G4 | 1 | sonnet | 25570 | 18 |
| G3 | 1 | orchestrator (small, after G1 and G2) | 0 | 0 |

### Review

- G1 deviation reverted: the coder forced `tls.ct@<port>` with CT skipped (an SCT table of zero placeholders); the test now enables CT instead (`ADLC-Test-Change`).
- `inspect_tls_ports.rs`: `!contains("handshake")` matched the `tls.handshake@` block ids and part order assumed stream order; the test now reads each part's own fragment (`ADLC-Test-Change`).
- BLOCKER fixed: OCSP, CAA and DANE of each port came from the response summary (first handshake over all ports); `port_wide` judges them on the port, CAA via `CaaInfo.lookup` (`#[serde(skip)]`).
- BLOCKER fixed: a host-wide TLSA error made `dane_valid` unmeasured on a port whose records answered.
- AMENDMENT repaired: band port is 443, else the lowest port (`band_port`), as before; `tls.caa` and `tls.notices` sit in its outcome.
- DEFERRED: a port whose TLSA lookup answered NODATA stays unmeasured when another port's lookup failed; `PortResult` cannot tell NODATA from failure without a per-port marker. Conservative under Rule M, same as the band before this feature.
- DEFERRED: the CAA branch of `port_wide` has no test (the records are `#[serde(skip)]`, JSON fixtures cannot set them).
- NIT: POST deduplicates before the `max_ports` check, the host-string parser counts duplicates.
- Second pass: no blocker.

### Behavioural verification

skipped: the entry points (`/inspect/tls/run`, `POST /api/v1/tls/inspect`) are driven end to end in `crates/web/tests/inspect_tls_ports.rs` and `api_tls_ports.rs` over the stub backend; a live run from a worktree build has no outbound network and DNS is blocked locally.
