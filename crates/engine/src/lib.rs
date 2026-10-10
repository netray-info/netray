//! The V2 engine: the `Module` trait and `FactsProvider`, the two seams between the engine and
//! the protocol modules, and the `Registry` that hands them out by protocol. The engine never
//! names a module crate (P40).

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

/// Per-run options a caller may set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunOptions {
    /// DKIM selectors to probe; `None` leaves the choice to the module.
    pub dkim_selectors: Option<Vec<String>>,
}

/// Per-run context handed to modules and the facts provider.
#[derive(Debug, Clone)]
pub struct RunContext {
    pub deadline: Instant,
    pub domain: Domain,
    pub options: RunOptions,
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
    Measured {
        checks: Vec<CheckResult>,
        /// Transitional: the V1 headline and extras, carried as JSON until the evidence shapes
        /// of phase 1b replace it.
        presentation: serde_json::Value,
    },
    NotApplicable {
        reason: String,
    },
    Incomplete {
        reason: String,
    },
    /// The module's own deadline fired.
    TimedOut,
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

/// The modules and the facts provider of one engine, looked up by protocol.
#[derive(Default)]
pub struct Registry {
    modules: Vec<Box<dyn Module>>,
    facts: Option<Box<dyn FactsProvider>>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a module; a second module for the same protocol replaces the first.
    pub fn with(mut self, module: Box<dyn Module>) -> Self {
        self.modules.retain(|m| m.protocol() != module.protocol());
        self.modules.push(module);
        self
    }

    pub fn with_facts(mut self, facts: Box<dyn FactsProvider>) -> Self {
        self.facts = Some(facts);
        self
    }

    pub fn module(&self, protocol: Protocol) -> Option<&dyn Module> {
        self.modules
            .iter()
            .find(|m| m.protocol() == protocol)
            .map(|m| m.as_ref())
    }

    pub fn facts(&self) -> Option<&dyn FactsProvider> {
        self.facts.as_deref()
    }
}
