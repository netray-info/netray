//! Beacon — DNS-only email-security inspector (fifth pillar of the netray
//! suite: IP -> DNS -> TLS -> HTTP -> Email).
//!
//! Given a domain (e.g. `example.com`), beacon evaluates twelve email
//! security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC,
//! BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate A–F
//! grade.
//!
//! The crate is split into self-contained modules: [`checks`] implements the
//! per-category probes and the three-phase orchestration; [`config`] holds the
//! `[modules.email]` sections; [`dns`] wraps `mhost` into a shared round-robin
//! resolver; [`quality`] defines the `Verdict` / `Grade` / `CheckResult` model
//! and grade computation; [`input`] validates domains and DKIM selectors;
//! [`security`] holds the rate limiter; [`state`] the outbound fetch settings.
//!
//! See `CLAUDE.md` and `specs/done/sdd/beacon.md` for design rationale.

pub mod checks;
pub mod config;
pub mod dns;
pub mod error;
pub mod input;
pub mod job;
pub mod module;
pub mod quality;
pub mod security;
pub mod state;
#[cfg(feature = "testing")]
pub mod testing;

pub use config::ModuleConfig;
pub use job::{
    EmailInspect, EmailInspectRequest, EmailInspectResult, EmailInspector, EmailJob, EmailRefusal,
    EmailSummary, InspectSlots,
};
pub use module::{EmailModule, translate};
pub use netray_common::middleware::RequestId;
