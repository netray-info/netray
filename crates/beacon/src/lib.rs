//! Beacon — DNS-only email-security inspector (fifth pillar of the netray
//! suite: IP -> DNS -> TLS -> HTTP -> Email).
//!
//! Given a domain (e.g. `example.com`), beacon evaluates twelve email
//! security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC,
//! BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate A–F
//! grade. Results stream to the client over Server-Sent Events as they
//! complete.
//!
//! The crate is split into self-contained modules: [`checks`] implements the
//! per-category probes and the three-phase orchestration; [`config`] loads
//! TOML plus `BEACON_*` environment overrides; [`dns`] wraps `mhost` into a
//! shared round-robin resolver; [`quality`] defines the `Verdict` / `Grade`
//! /`CheckResult` model and grade computation; [`input`] validates domains
//! and DKIM selectors; [`routes`] wires the Axum handlers and the utoipa
//! OpenAPI document; [`security`] holds IP extraction, rate limiting, and
//! security-header middleware; [`state`] builds the shared [`state::AppState`].
//!
//! See `CLAUDE.md` and `specs/done/sdd/beacon.md` for design rationale.

pub mod checks;
pub mod config;
pub mod dns;
pub mod error;
pub mod input;
pub mod quality;
pub mod routes;
pub mod security;
pub mod state;

pub use netray_common::middleware::RequestId;
