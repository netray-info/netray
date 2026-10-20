# Plan: v2-fixture-consistency

## Phase 1 — HTTP, TLS and IP from evidence

## Groups

- G1 (C1, C4, C5, C6, C7, C8): HTTP — `crates/http/src/testing.rs`.
- G2 (C3, C4, C14, C15): IP — `crates/ip/src/testing.rs`.
- G3 (C2, C4, C9, C10, C11, C12, C13): TLS — `crates/tls/src/testing.rs`, `crates/tls/src/module.rs`
  (clock seam), `crates/tls/src/routes.rs` (`compute_consistency` to `pub(crate)`).

The groups share no production file; they are coded in parallel.

## Plan

- G1: `pub async fn from_evidence(domain, blocks) -> SectionOutcome` in `testing.rs`. Rebuild
  `inspect::InspectResult { https, http_upgrade, cors }`: `https.headers` from `http.headers` rows
  (a `HeaderMap`, repeats kept), status, final_url, http_version from `http.server`, redirects from
  `http.redirects` (hops url/status/location/http_version), error from `http.server/error_kind`;
  `http_upgrade` from `http.port80` hops and `http.port80_probe` (status_code, location as the
  `location` header, final_url = the last hop's location or the probe url, error_kind); `cors.headers`
  from `http.cors` (`access-control-allow-origin` from `allow_origin`, `access-control-allow-credentials`
  from `allow_credentials_header`). Resolved address from `http.server/address`; enrichment org and
  ip_type from `org`/`network_type`; duration from `inspection_ms`. Return
  `translate(&assemble_response(url, addr, result, enrichment, None, duration))`.
- G2: `from_evidence` in `testing.rs`: per `ip.addresses` row an `Ifconfig` (network asn from
  "AS<n>", org, prefix, type, flags; location country/city), total = records in `dns.a` + `dns.aaaa`
  (rows count when absent); return `module::translate(&lookups, total)`.
- G3: `from_evidence` in `testing.rs`: rebuild `routes::InspectResponse` for port 443: per
  `tls.addresses` row an `IpInspectionResult` (error from outcome/message when not "ok"; chain
  `CertInfo` from `tls.chain@<addr>:443` items, key split into type and size, lifetime from the
  times, days_remaining and is_expired from the block; `TlsParams` from `tls.handshake@…`; `CtInfo`
  from `tls.ct`; probes from `tls.probes`; `ValidationResult` built from the chain: order, hostname
  coverage, weakest signature, expiry; trusted and at an anchor when in issuer order and ending
  self-signed), port consistency via `compute_consistency`, summary via `validate::summarize`
  (OCSP from the first handshake and the leaf's ocsp_url, CAA via `check_caa_compliance` over
  `tls.caa` or `dns.caa`, CT status from `tls.ct`), ECH from the handshake or `dns.https`. The
  clock: `module.rs` threads a `now: DateTime<Utc>` from `translate_with_facts` (wall clock) through
  an internal `translate_with_facts_at` to `cert_fields`; `from_evidence` passes the run's time
  (leaf not_after minus days_remaining).
