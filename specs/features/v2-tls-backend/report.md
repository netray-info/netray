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
