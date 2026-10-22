# Plan: v2-tls-backend

## Phase 1 — Certificate, handshake and OCSP detail in the evidence

## Groups

- G1: C1-C17 (all touch `crates/tls/src/module.rs`; C4/C12-C15 also `crates/tls/src/tls/ocsp.rs`).

## Plan

G1:
- `crates/tls/src/tls/ocsp.rs`: `OcspRevocationResult` gains `#[serde(skip)] #[schema(ignore)] pub answered: bool`; `true` only on the paths of `parse_live_ocsp_response` where the responder returned a successful basic response with a single response (good, revoked, unknown cert status); `false` for every fetch failure, blocked target, non-200, undecodable or unsuccessful response.
- `crates/tls/src/module.rs` `cert_fields`: after `fingerprint_sha256`, push `fingerprint_sha1` (non-empty), `serial` (non-empty), `policy` (`EV`/`OV`/`DV` only), `self_signed`, `expired` (bools; omitted when the cert failed to parse, i.e. empty `subject` issuer? keep: always present for parsed certs), `ocsp_url` (Some).
- `handshake_fields`: `sni`, `starttls` when Some; `ocsp_status`, `ocsp_this_update`, `ocsp_next_update` (via `rfc3339`) from `tls.ocsp` when Some; `ocsp_live_status`, `ocsp_live_revoked_at` (Time), `ocsp_live_reason` from `tls.ocsp_live` when `answered`.
- `REGISTER`: declare the 14 keys; `VOLATILE`: the five OCSP paths.
- SDD `specs/sdd/v2.md` §3.7 tls rows (blocks and volatile).

## Phase 2 — Multi-port inspector, notices and API ports

## Groups

- G1: C1, C2, C8-C12 — `crates/tls/src/module.rs`.
- G2: C4 (parse dedupe), C6, C16-C19 — `crates/tls/src/inspector.rs`, OpenAPI golden.
- G4: C5, C15 — `crates/web/src/explore.rs`.
- G3 (after G1, G2): C3, C13, C14 — `crates/web/src/api/jobs.rs`.

## Plan

G1: `translate_with_facts` becomes a call of a private `translate_port(resp, port, facts, qualify, host_wide)`; `addresses_block` and `ct_block` take the qualifier; `blocks` emits `tls.caa` only when `host_wide`. `pub fn translate_ports(resp) -> Vec<(u16, SectionOutcome)>`: one port → `translate(resp)`; more → per port `translate_port(.., &[], true, i == 0)`; then a `tls.notices` fields block (`warning` text, `skipped_address` addr) pushed into the first outcome's evidence when warnings or parseable skipped addresses exist. REGISTER gains `("tls.notices", Fields)`, keys `warning` Text, `skipped_address` Addr. SDD §3.7 tls row and §6.2 TLS row.
G2: `TlsInspectRequest { host, port: Option<u16>, ports: Option<Vec<u16>> }`; `inspect` refuses both, empty, too many (`TooManyPorts`), port 0; deduplicates; neither → 443. `TlsInspector::parse` deduplicates in first-occurrence order. Regenerate `tests/fixtures/contracts/api-v1.openapi.json`. SDD §7.1 row.
G4: `block_links` for `("tls","addresses")` uses the block id's qualifier as port when it parses as u16, else 443.
G3: `start_tls` builds one `JobPart` per parsed port (label `handshake` for one port, `port <n>` otherwise) over one shared future of `translate_ports`; the band takes the outcome of 443, else the first.
