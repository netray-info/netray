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

## Phase 2 — DNS and email from evidence

## Groups

- G1 (C1, C3, C4, C5, C6, C7): DNS — `crates/dns/src/testing.rs` (visibility of `pub(crate)` check
  pieces as needed, no behaviour change).
- G2 (C2, C3, C8, C9, C10, C11): email — `crates/email/src/testing.rs`, `crates/email/Cargo.toml`
  (the loopback policy listener's TLS crates as optional dependencies of the `testing` feature).

The groups share no production file; they are coded in parallel.

## Plan

- G1: `from_evidence` rebuilds the measure stage of `api::check::run_check` in `testing.rs`: per
  `dns.<type>` block one mhost `Lookup` built through serde JSON as `ZoneLookup` does (empty block a
  definite NODATA, missing block no lookup), DNSKEY key tag computed (RFC 4034 App. B) with zone/SEP/
  revoked flags; `CheckEvent::Batch` per `CHECK_RECORD_TYPES`; `lint_lookups` over the merged
  lookups; `check_ns_lame_delegation` and `check_ns_delegation_consistency` over a `RawOutbound` stub
  that resolves each NS host to a public documentation-free test address and answers
  authoritatively with the zone's SOA and NS set (template: `ns_check_outcomes`); events in
  `run_check` order, then `unanswered_events`, then `translate`.
- G2: `from_evidence` seeds `TestDnsResolver` from the email blocks (MX, MX addresses, TXT at the
  apex, `_dmarc.`, `_smtp._tls.`, `default._bimi.`, `_mta-sts.`, DMARC report authorisation from
  `rua_authorized`/`ruf_authorized`, DKIM TXT per row with a committed RSA 2048 key, TLSA, PTR and
  forward for FCrDNS, DNSBL listings, DNSKEY presence), starts a loopback rustls listener serving
  the recorded MTA-STS status, content type and body (rcgen self-signed, `accept_invalid_certs`,
  `allow = |_| true`, `with_mta_sts_port`), runs the inspection with the rows' selectors and
  `translate`s the events.

## Phase 3 — Guard and consistent fixtures

## Groups

- G1 (C1–C10): the fixtures under `tests/acceptance/fixtures/history/` — evidence edits for the
  story, then `UPDATE_FIXTURES=1` regeneration until the guard is stable.

## Plan

- Regenerate the unedited fixtures, diff each status against the stored one, change the evidence
  where the stored status is the story's (headers, port-80 chain, chain roots, OCSP URL, older
  certificate, DKIM key size, DMARC and MTA-STS records, DANE/FCrDNS/DNSBL blocks, NSEC next name),
  regenerate until a second pass changes nothing, then `just design-check`.
