//! The email inspector as a library: its `Module` implementation (`email.*` checks, from DNS
//! plus the MTA-STS policy and BIMI logo fetches) and the inspection netray-web runs for API v1
//! and the email page.
//!
//! Given a domain (e.g. `example.com`), the checks cover twelve mail security categories (MX,
//! SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC, BIMI, FCrDNS, DNSBL, cross-validation).
//!
//! [`checks`] implements the per-category probes and the three-phase orchestration; [`module`]
//! translates their results onto the `email.*` checks and evidence blocks; [`job`] holds the
//! inspector netray-web runs; [`config`] the `[modules.email]` table; [`dns`] wraps `mhost` into
//! a shared round-robin resolver; [`quality`] defines the `Verdict` / `Grade` / `CheckResult`
//! model; [`input`] validates domains and DKIM selectors; [`security`] holds the rate limiter;
//! [`state`] the outbound fetch settings.

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
