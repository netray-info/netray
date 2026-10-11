# Plan: v2-check-fixes

## Phase 1 — Independent check fixes

## Groups

- G1 (C1–C6): `crates/http/src/inspect/cors.rs`, `crates/http/src/inspect/mod.rs`.
- G2 (C7–C10): `crates/tls/src/validate/caa_compliance.rs`.
- G3 (C11–C19): `crates/tls/src/tls/chain.rs`, `crates/tls/src/validate/chain_trust.rs`, `crates/tls/src/quality/checks.rs`, `crates/engine/catalogue/checks.toml`.

The groups share no production file; they run in parallel.

## Plan

- G1: `pub const PROBE_ORIGIN: &str = "https://evil.example.com"` in `cors.rs`; `mod.rs:97` passes `Some(cors::PROBE_ORIGIN)`; `analyze_cors` sets `reflects_origin = acao == Some(PROBE_ORIGIN) || acao == Some("null")`.
- G2: `KNOWN_TAGS` in `caa_compliance.rs`; line 21 fails only when `r.issuer_critical && !KNOWN_TAGS.iter().any(|t| t.eq_ignore_ascii_case(&r.tag))`. The unit test `issuer_critical_blocks_all_issuance` moves to an unknown tag (ADLC-Test-Change).
- G3: `format_sig_algo` becomes pub and gains the six OIDs. `pub fn weakest_signature(chain)` in `chain_trust.rs` skips index > 0 self-signed certificates, ranks weak (sha1/md5/md2) below unreadable/unknown below the strong ones; `validate_chain` calls it. `check_strong_signature` gets the two Warn branches. `checks.toml` `tls.strong_signature` finding gets the R4 text. The unit tests touched (`unknown_algo_is_weakest`, `strong_signature_*`) carry ADLC-Test-Change if they change.
