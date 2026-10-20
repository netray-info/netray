# Plan: v2-tls-versions-coop

## Phase 1 — ClientHello probes

## Groups

- G1: C1-C19 (all touch the new `crates/tls/src/tls/probe.rs`).

## Plan

G1:
- `crates/tls/src/tls/mod.rs`: `pub mod probe;`.
- `crates/tls/src/tls/probe.rs`:
  - `ProbeKind` (7 kinds) with `ALL`, `client_version`, `suites` (static lists: insecure RC4/3DES/DES/export/NULL/anon; cbc ECDHE+DHE AES-CBC; static_rsa every RSA key-exchange suite incl. RSA RC4/3DES; version probes a broad list valid for the version; SSLv3 RSA and DHE only), `key`.
  - `client_hello(kind, sni)`: record (0x16, 0x0300 for SSLv3 else 0x0301), ClientHello with random bytes, empty session id, suites + 0x00FF, null compression; extensions for every kind but SSLv3: server_name for a host name, supported_groups, ec_point_formats, signature_algorithms at 0x0303 only.
  - `Verdict`, `ProbeOutcome`, `StreamEnd`, `classify(kind, bytes, end)`: concatenate handshake-record payloads, stop at an alert record (refused, alert name) or a whole ServerHello (version, suite; accepted per kind rules; unoffered suite unanswered); anything else unanswered naming the end.
  - `ProtocolProbes` and `probe_address(ip, port, sni, negotiated, timeout)`: one task per probed kind (TLS 1.2 only when negotiated is TLSv1.3), each: connect, SMTP STARTTLS on 25/587 (reuse `connect::smtp_starttls`, made `pub(crate)`), write the ClientHello, read until classify can decide or the stream ends; all under `tokio::time::timeout`; join concurrently; derive tls1_2/tls1_3 from the handshake.

## Phase 2 — Probes in the inspection and the evidence

## Groups

- G1: C1-C12 (all hang on the new `IpInspectionResult.probes` field; the inspection half and the evidence half share it).

## Plan

G1:
- `crates/tls/src/tls/mod.rs`: `IpInspectionResult.probes: Option<probe::ProtocolProbes>` with `#[serde(skip)] #[schema(ignore)]`; `inspect_ip` sets `None`.
- `crates/tls/src/backend.rs`: `TlsBackend::probe(ip, port, sni, negotiated, timeout) -> BoxFuture<Option<ProtocolProbes>>`; `LiveBackend` calls `probe::probe_address` and returns `Some`.
- `crates/tls/src/routes.rs` `inspect_port`: after a handshake without error, run `backend.probe` with `tls.version` concurrently with the live OCSP check (`tokio::join!`), inside the held permit; store into `result.probes`.
- `crates/tls/src/testing.rs`: `with_probes`, `with_failed_handshake`, `probe_calls`.
- `crates/tls/src/module.rs`: `REGISTER` gains `tls.probes` and `tls.protocols` (fields) with the 8 value keys, 8 `_answer` keys (probes) and `forward_secrecy_all_suites` (protocols, bool); `blocks()` pushes one `tls.probes@<addr>:<port>` per handshake with probes and one `tls.protocols` (qualified by port when `qualify`) when any handshake has probes.
