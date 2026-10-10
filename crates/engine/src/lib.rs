//! The V2 engine: the `Module` trait and `FactsProvider`, the two seams between the engine and
//! the protocol modules. The engine never names a module crate (P40).

use std::error::Error;
use std::fmt;
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::time::Instant;

use netray_model::{CheckId, CheckResult, Protocol};

/// A boxed, sendable future; keeps the traits usable as trait objects.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A dotted path into a check's evidence, e.g. `tls.handshake_ms`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidencePath(pub &'static str);

/// The domain under test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Domain(String);

impl Domain {
    pub fn new(name: impl Into<String>) -> Self {
        Domain(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Per-run context handed to modules and the facts provider.
#[derive(Debug, Clone)]
pub struct RunContext {
    pub deadline: Instant,
}

/// DNS facts resolved once per run and shared by all modules.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Facts {
    pub a: Vec<Ipv4Addr>,
    pub aaaa: Vec<Ipv6Addr>,
    pub mx: Vec<String>,
    pub caa: Vec<String>,
    pub ns: Vec<String>,
}

/// What a module returns for its section.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionOutcome {
    Measured(Vec<CheckResult>),
    NotApplicable { reason: String },
    Incomplete { reason: String },
}

/// Facts could not be resolved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveError(pub String);

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for ResolveError {}

/// One protocol's checks.
pub trait Module: Send + Sync {
    fn protocol(&self) -> Protocol;
    /// The checks this module can produce.
    fn checks(&self) -> &'static [CheckId];
    /// Evidence paths whose values vary between runs.
    fn volatile(&self) -> &'static [EvidencePath];
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome>;
}

/// Resolves the shared DNS facts for a domain.
pub trait FactsProvider: Send + Sync {
    fn resolve<'a>(
        &'a self,
        ctx: &'a RunContext,
        domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>>;
}
