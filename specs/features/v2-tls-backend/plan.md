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
