//! The V2 engine: the `Module` trait and `FactsProvider`, the two seams between the engine and
//! the protocol modules, the `Registry` that hands them out by protocol, and [`run`], which
//! drives one check through them. The engine never names a module crate (P40).

pub mod catalogue;
pub mod domain_run;
pub mod history;
pub mod profile;
pub mod scoring;

pub use catalogue::catalogue;
pub use domain_run::{DomainRun, run_domain, section_plan};
pub use scoring::score;

use std::collections::HashSet;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;
use std::time::{Duration, Instant};

use futures::FutureExt;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::sync::mpsc;

use netray_model::evidence::{Block, BlockId};
use netray_model::run::SectionKind;
use netray_model::{CheckId, CheckResult, Protocol, Status};

/// A boxed, sendable future; keeps the traits usable as trait objects.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A path into a module's evidence: `<block name>/<key>`, e.g. `tls.handshake/handshake_ms`, or
/// `<block name>/<column>@<row key>` for one table row, e.g. `http.headers/value@date`, or
/// `<block name>/*`, which names the whole block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvidencePath(pub &'static str);

impl EvidencePath {
    /// Splits `path` into block id, key and optional row key: at the first `/`, then at the
    /// first `@` after it. The key `*` names the whole block and takes no row key.
    pub fn parse(path: &str) -> Result<(BlockId, &str, Option<&str>), EvidencePathError> {
        let err = |why: &str| EvidencePathError(format!("evidence path {path:?} {why}"));
        let (block, rest) = path.split_once('/').ok_or_else(|| err("has no '/'"))?;
        let block = BlockId::parse(block).map_err(|e| err(&e.to_string()))?;
        let (key, row) = match rest.split_once('@') {
            Some((key, row)) => (key, Some(row)),
            None => (rest, None),
        };
        if key.is_empty() || row.is_some_and(str::is_empty) {
            return Err(err("has an empty key or row"));
        }
        if key == "*" && row.is_some() {
            return Err(err("has a '*' key with a row"));
        }
        Ok((block, key, row))
    }
}

/// Why a string is not a valid evidence path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidencePathError(String);

impl fmt::Display for EvidencePathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for EvidencePathError {}

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
    /// HTTPS records in zone-file form, e.g. `1 . alpn=h2`.
    pub https: Vec<String>,
    /// A and AAAA both definite negatives on every resolver asked.
    pub no_address_definite: bool,
    /// The records blocks of the lookups that answered.
    pub blocks: Vec<Block>,
}

/// What a module returns for its section.
#[derive(Debug, Clone, PartialEq)]
pub enum SectionOutcome {
    Measured {
        checks: Vec<CheckResult>,
        evidence: Vec<Block>,
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

/// The kind and reason a section is reported with, given the `(status, findings)` of the checks
/// of a section its module reported measured. A section with at least one check and every check
/// `Unmeasured` is `Incomplete` with the reason "every check unmeasured: <the first check's first
/// finding>" (no suffix when that check has no finding); any other is `Measured`. Every site that
/// maps a section outcome onto a [`SectionKind`] goes through this rule.
pub fn measured_kind_and_reason<'a>(
    checks: impl IntoIterator<Item = (Status, &'a [String])>,
) -> (SectionKind, Option<String>) {
    let mut checks = checks.into_iter().peekable();
    let Some((_, first_findings)) = checks.peek().copied() else {
        return (SectionKind::Measured, None);
    };
    if checks.all(|(status, _)| status == Status::Unmeasured) {
        let reason = match first_findings.first() {
            Some(finding) => format!("every check unmeasured: {finding}"),
            None => "every check unmeasured".to_string(),
        };
        return (SectionKind::Incomplete, Some(reason));
    }
    (SectionKind::Measured, None)
}

impl SectionOutcome {
    /// The kind and reason this outcome is reported with; see [`measured_kind_and_reason`] for
    /// the `Measured` case. `NotApplicable` and `Incomplete` carry their reason, `TimedOut` none.
    pub fn kind_and_reason(&self) -> (SectionKind, Option<String>) {
        match self {
            SectionOutcome::Measured { checks, .. } => {
                measured_kind_and_reason(checks.iter().map(|c| (c.status, c.findings.as_slice())))
            }
            SectionOutcome::NotApplicable { reason } => {
                (SectionKind::NotApplicable, Some(reason.clone()))
            }
            SectionOutcome::Incomplete { reason } => {
                (SectionKind::Incomplete, Some(reason.clone()))
            }
            SectionOutcome::TimedOut => (SectionKind::TimedOut, None),
        }
    }
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
    /// Whether the module needs the resolved addresses; when the resolve stage fails, such a
    /// module is not run and its section is `Incomplete`.
    fn needs_addresses(&self) -> bool {
        false
    }
    /// Whether the module reads the resolved `Facts` without needing addresses; such a module
    /// waits for the resolve inside its own window and, after a failed resolve, runs with empty
    /// `Facts`.
    fn wants_facts(&self) -> bool {
        false
    }
    /// Whether the module has nothing to measure on a name without A and AAAA records; when the
    /// facts are a definite negative such a module is not run and its section is not applicable.
    fn needs_address_records(&self) -> bool {
        self.needs_addresses()
    }
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

/// One finished section of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct SectionEvent {
    pub protocol: Protocol,
    pub outcome: SectionOutcome,
}

/// What a run leaves besides its sections.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    /// The resolved facts; empty when the resolve stage failed or no provider is registered.
    pub facts: Facts,
    pub resolve_error: Option<ResolveError>,
    /// The evidence of every measured section.
    sections_evidence: Vec<Block>,
}

impl RunReport {
    /// The blocks of the run, one per id: the evidence of every measured section, then the
    /// facts blocks whose id no section carries. A section's block wins over the facts block
    /// of the same id; a facts block fills in where no section has it, e.g. after a DNS
    /// timeout.
    pub fn blocks(&self) -> Vec<&Block> {
        let mut seen = HashSet::new();
        self.sections_evidence
            .iter()
            .chain(&self.facts.blocks)
            .filter(|b| seen.insert(&b.id))
            .collect()
    }
}

/// Runs one check. The resolve stage calls the registry's provider once (none registered: empty
/// `Facts`, no error), bounded by the earlier of `resolve_budget` from the run start and
/// `base.deadline`; an overrun is a `ResolveError`. Every module named in `sections` runs
/// concurrently with the resolve, each under the earlier of its section duration from the run
/// start and `base.deadline`. Modules that need addresses wait for the resolve inside their own
/// window and, after a failed resolve, are not run and yield `Incomplete`; modules that want
/// facts also wait inside their own window and, after a failed resolve, run with empty `Facts`;
/// all others start at the run start with empty `Facts`. A module with `needs_address_records`
/// also waits for the resolve; when the facts are `no_address_definite` it is not run and yields
/// `NotApplicable`, and one that only has `needs_address_records` runs with empty `Facts` after
/// a resolve that succeeded without a definite negative or failed. Each section is sent on `tx`
/// the moment it finishes; a
/// module that overruns its deadline yields `TimedOut`. Protocols without a registered module
/// are skipped, and a closed receiver only drops the events.
pub async fn run(
    registry: &Registry,
    base: RunContext,
    sections: &[(Protocol, Duration)],
    resolve_budget: Duration,
    tx: mpsc::Sender<SectionEvent>,
) -> RunReport {
    let run_start = Instant::now();
    let resolve_deadline = (run_start + resolve_budget).min(base.deadline);
    let resolve = async {
        match registry.facts() {
            None => Ok(Facts::default()),
            Some(provider) => tokio::time::timeout_at(
                resolve_deadline.into(),
                provider.resolve(&base, &base.domain),
            )
            .await
            .unwrap_or_else(|_| Err(ResolveError("resolve budget exceeded".to_string()))),
        }
    }
    .shared();

    let no_facts = Facts::default();

    let mut running: FuturesUnordered<_> = sections
        .iter()
        .filter_map(|&(protocol, section)| {
            let module = registry.module(protocol)?;
            let deadline = (run_start + section).min(base.deadline);
            let ctx = RunContext {
                deadline,
                ..base.clone()
            };
            let (resolve, no_facts, tx) = (resolve.clone(), &no_facts, &tx);
            Some(async move {
                let work = async {
                    if !(module.needs_addresses()
                        || module.wants_facts()
                        || module.needs_address_records())
                    {
                        return module.run(&ctx, no_facts).await;
                    }
                    match resolve.await {
                        Ok(f) if f.no_address_definite && module.needs_address_records() => {
                            SectionOutcome::NotApplicable {
                                reason: "no A or AAAA record".into(),
                            }
                        }
                        Ok(f) if module.needs_addresses() || module.wants_facts() => {
                            module.run(&ctx, &f).await
                        }
                        Err(e) if module.needs_addresses() => SectionOutcome::Incomplete {
                            reason: format!("address resolution failed: {e}"),
                        },
                        Err(_) if module.wants_facts() => module.run(&ctx, &Facts::default()).await,
                        _ => module.run(&ctx, no_facts).await,
                    }
                };
                let outcome = tokio::time::timeout_at(deadline.into(), work)
                    .await
                    .unwrap_or(SectionOutcome::TimedOut);
                let evidence = match &outcome {
                    SectionOutcome::Measured { evidence, .. } => evidence.clone(),
                    _ => Vec::new(),
                };
                let _ = tx.send(SectionEvent { protocol, outcome }).await;
                evidence
            })
        })
        .collect();
    let (resolved, sections_evidence) = futures::join!(resolve, async {
        let mut all = Vec::new();
        while let Some(evidence) = running.next().await {
            all.extend(evidence);
        }
        all
    });

    match resolved {
        Ok(facts) => RunReport {
            facts,
            resolve_error: None,
            sections_evidence,
        },
        Err(e) => RunReport {
            facts: Facts::default(),
            resolve_error: Some(e),
            sections_evidence,
        },
    }
}
