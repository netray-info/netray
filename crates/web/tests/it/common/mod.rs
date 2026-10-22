//! Copied from lens's `tests/common/mod.rs` (spec v2-cutover, Phase 6). Shared by the tests that
//! run the DNS, TLS, HTTP, email and IP sections through the engine registry: the DNS module is `netray_dns::testing::golden_module` on a prism `.sse` golden, the
//! TLS module is `netray_tls::testing::golden_module` on a tlsight contract golden,
//! the HTTP module is `netray_http::testing::golden_module` on a spectra contract golden, the
//! email module `netray_email::testing::golden_module` on a beacon `.sse` golden, the IP module
//! `netray_ip::testing::golden_module` on the ifconfig contract golden; for a scenario where a
//! section fails, a module answering `SectionOutcome::Incomplete`. Every registry also carries
//! the resolve stage: `netray_dns::testing::golden_facts` on the same prism golden as the DNS
//! module, so the IP module samples its addresses from `Facts`, not from the DNS section.

// Every test crate compiles this file whole but uses only some helpers.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use netray_engine::domain_run::{BackendResult, HARD_DEADLINE, SectionError, adapt};
use netray_engine::profile::{Profile, Profiles};
use netray_engine::{
    BoxFuture, Domain, EvidencePath, Facts, FactsProvider, Module, Registry, ResolveError,
    RunContext, RunOptions, SectionOutcome,
};
use netray_model::{CheckId, Protocol};
use netray_web::admission::ApexState;
use netray_web::api::AdmissionLimits;
use netray_web::config::ApexConfig;
use tokio::sync::Notify;

/// A module of `protocol` whose run is always incomplete: what lens saw as "the backend
/// answered HTTP 500".
struct Incomplete(Protocol);

impl Module for Incomplete {
    fn protocol(&self) -> Protocol {
        self.0
    }
    fn checks(&self) -> &'static [CheckId] {
        &[]
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        &[]
    }
    fn needs_address_records(&self) -> bool {
        matches!(self.0, Protocol::Ip | Protocol::Http | Protocol::Tls)
    }
    fn run<'a>(&'a self, _ctx: &'a RunContext, _facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async {
            SectionOutcome::Incomplete {
                reason: "module failed".to_string(),
            }
        })
    }
}

/// Wraps a module so that its run starts only after `delay`; `None` never finishes. What lens
/// saw as a backend that stalls or never answers.
struct Slow {
    inner: Box<dyn Module>,
    delay: Option<Duration>,
}

impl Module for Slow {
    fn protocol(&self) -> Protocol {
        self.inner.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.inner.checks()
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        self.inner.volatile()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            match self.delay {
                Some(d) => tokio::time::sleep(d).await,
                None => std::future::pending::<()>().await,
            }
            self.inner.run(ctx, facts).await
        })
    }
}

/// Wraps a module so that its run announces itself on `entered` and then waits until `release`
/// is notified. What lens saw as a backend stub that holds its answer.
struct Gated {
    inner: Box<dyn Module>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl Module for Gated {
    fn protocol(&self) -> Protocol {
        self.inner.protocol()
    }
    fn checks(&self) -> &'static [CheckId] {
        self.inner.checks()
    }
    fn volatile(&self) -> &'static [EvidencePath] {
        self.inner.volatile()
    }
    fn run<'a>(&'a self, ctx: &'a RunContext, facts: &'a Facts) -> BoxFuture<'a, SectionOutcome> {
        Box::pin(async move {
            self.entered.notify_one();
            self.release.notified().await;
            self.inner.run(ctx, facts).await
        })
    }
}

/// `module` held open: its run announces itself on `entered` and answers once `release` is
/// notified.
pub fn gated(
    module: Box<dyn Module>,
    entered: Arc<Notify>,
    release: Arc<Notify>,
) -> Box<dyn Module> {
    Box::new(Gated {
        inner: module,
        entered,
        release,
    })
}

/// `module` answering only after `delay` (`None`: never).
pub fn slow(module: Box<dyn Module>, delay: Option<Duration>) -> Box<dyn Module> {
    Box::new(Slow {
        inner: module,
        delay,
    })
}

/// The contents of the contract golden `file` (from `tests/fixtures/contracts/`).
pub fn golden(file: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/contracts")
        .join(file);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()))
}

/// The documentation address the DNS golden's A record carries, which lens's production address
/// policy does not enrich, and the public stand-in the harness serves in its place so the IP
/// section stays scored.
const DNS_DOCUMENTATION_A: &str = r#"{"A":"192.0.2.10"}"#;
const DNS_PUBLIC_A: &str = r#"{"A":"1.1.1.1"}"#;

/// A DNS module that runs the prism contract golden `file` (a `prism*.sse`) as it is.
pub fn dns_golden_raw(file: &str) -> Box<dyn Module> {
    netray_dns::testing::golden_module(&golden(file))
}

/// A DNS module that runs the prism contract golden `file` with the A record's documentation
/// address replaced by a public one.
pub fn dns_golden(file: &str) -> Box<dyn Module> {
    dns_with_body(&golden(file).replace(DNS_DOCUMENTATION_A, DNS_PUBLIC_A))
}

/// A DNS module that runs `contract_sse` (a prism check stream).
pub fn dns_with_body(contract_sse: &str) -> Box<dyn Module> {
    netray_dns::testing::golden_module(contract_sse)
}

/// The resolve stage for the prism golden `file` as it is (`dns_golden_raw`).
pub fn facts_golden_raw(file: &str) -> Box<dyn FactsProvider> {
    netray_dns::testing::golden_facts(&golden(file))
}

/// The resolve stage for the prism golden `file` with the same public stand-in `dns_golden`
/// serves, so the IP section samples the addresses it sampled before.
pub fn facts_golden(file: &str) -> Box<dyn FactsProvider> {
    facts_with_body(&golden(file).replace(DNS_DOCUMENTATION_A, DNS_PUBLIC_A))
}

/// The resolve stage for `contract_sse` (a prism check stream).
pub fn facts_with_body(contract_sse: &str) -> Box<dyn FactsProvider> {
    netray_dns::testing::golden_facts(contract_sse)
}

/// A resolve stage that answers exactly `facts`.
pub struct StubFacts(pub Facts);

impl FactsProvider for StubFacts {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

/// A resolve stage that fails.
pub struct FailingFacts;

impl FactsProvider for FailingFacts {
    fn resolve<'a>(
        &'a self,
        _ctx: &'a RunContext,
        _domain: &'a Domain,
    ) -> BoxFuture<'a, Result<Facts, ResolveError>> {
        Box::pin(async { Err(ResolveError("resolver unreachable".to_string())) })
    }
}

/// A resolve stage answering exactly the A and AAAA addresses in `ips`.
pub fn facts_with_ips(ips: &[&str]) -> Box<dyn FactsProvider> {
    let mut facts = Facts::default();
    for ip in ips {
        match ip.parse::<std::net::IpAddr>().unwrap() {
            std::net::IpAddr::V4(a) => facts.a.push(a),
            std::net::IpAddr::V6(a) => facts.aaaa.push(a),
        }
    }
    Box::new(StubFacts(facts))
}

/// A DNS module whose run is incomplete: what lens saw as "prism answered HTTP 500".
pub fn dns_incomplete() -> Box<dyn Module> {
    Box::new(Incomplete(Protocol::Dns))
}

/// An email module that runs the beacon contract golden `file` (a `beacon-*.sse`).
pub fn email_golden(file: &str) -> Box<dyn Module> {
    netray_email::testing::golden_module(&golden(file))
}

/// An email module whose run is incomplete: what lens saw as "beacon answered HTTP 500".
pub fn email_incomplete() -> Box<dyn Module> {
    Box::new(Incomplete(Protocol::Email))
}

/// A TLS module that runs the tlsight contract golden `file` (a `tlsight-*.json`); `None` makes
/// it incomplete: what lens saw as "tlsight answered HTTP 500".
pub fn tls_module(file: Option<&str>) -> Box<dyn Module> {
    match file {
        Some(name) => netray_tls::testing::golden_module(&golden(name)),
        None => Box::new(Incomplete(Protocol::Tls)),
    }
}

/// A TLS module that runs the tlsight contract golden `file`.
pub fn tls_golden(file: &str) -> Box<dyn Module> {
    tls_module(Some(file))
}

/// A TLS module whose run is incomplete.
pub fn tls_incomplete() -> Box<dyn Module> {
    Box::new(Incomplete(Protocol::Tls))
}

/// An HTTP module that runs the spectra contract golden `file`; `None` makes it incomplete.
pub fn http_module(file: Option<&str>) -> Box<dyn Module> {
    match file {
        Some(name) => netray_http::testing::golden_module(&golden(name)),
        None => Box::new(Incomplete(Protocol::Http)),
    }
}

/// An IP module that answers the ifconfig contract golden `file` (an `ifconfig-json*.json`) for
/// every sampled address.
pub fn ip_golden(file: &str) -> Box<dyn Module> {
    netray_ip::testing::golden_module(&golden(file))
}

/// An IP module that answers `contract_json` (an ifconfig body) for every sampled address.
pub fn ip_with_body(contract_json: &str) -> Box<dyn Module> {
    netray_ip::testing::golden_module(contract_json)
}

/// An IP module whose run is incomplete: what lens saw as a failed or timed-out enrichment call.
pub fn ip_incomplete() -> Box<dyn Module> {
    Box::new(Incomplete(Protocol::Ip))
}

/// One engine run of `protocol`'s section alone over `registry` for `example.com`, the resolve
/// stage under the hard deadline, adapted onto its section result; what lens's
/// `ModuleSection::run` did.
async fn run_section(
    registry: Registry,
    protocol: Protocol,
    timeout: Duration,
) -> Result<BackendResult, SectionError> {
    if registry.module(protocol).is_none() {
        return Err(SectionError::BackendError(format!(
            "no {protocol:?} module registered"
        )));
    }
    let base = RunContext {
        deadline: std::time::Instant::now() + HARD_DEADLINE,
        domain: Domain::new("example.com"),
        options: RunOptions {
            dkim_selectors: None,
        },
    };
    let plan = [(protocol, timeout)];
    let (tx, mut rx) = tokio::sync::mpsc::channel(1);
    let (report, event) = tokio::join!(
        netray_engine::run(&registry, base, &plan, HARD_DEADLINE, tx),
        rx.recv()
    );
    let outcome = event.map_or(SectionOutcome::TimedOut, |e| e.outcome);
    adapt(protocol, "example.com", outcome, &report)
}

/// Run the IP section over `module` for a domain whose resolve stage answered `ips`. `Err` is
/// what turns into an Errored section.
pub async fn run_ip(
    module: Box<dyn Module>,
    timeout: Duration,
    ips: &[&str],
) -> Result<BackendResult, SectionError> {
    let registry = Registry::new().with(module).with_facts(facts_with_ips(ips));
    run_section(registry, Protocol::Ip, timeout).await
}

/// Run the DNS section over `module`. `Err` is what turns into an Errored section.
pub async fn run_dns(
    module: Box<dyn Module>,
    timeout: Duration,
) -> Result<BackendResult, SectionError> {
    let registry = Registry::new().with(module).with_facts(facts_with_ips(&[]));
    run_section(registry, Protocol::Dns, timeout).await
}

/// Run the TLS section over `module`. `Err` is what turns into an Errored section.
pub async fn run_tls(
    module: Box<dyn Module>,
    timeout: Duration,
) -> Result<BackendResult, SectionError> {
    let registry = Registry::new().with(module).with_facts(facts_with_ips(&[]));
    run_section(registry, Protocol::Tls, timeout).await
}

/// A registry with the given DNS, HTTP, email, IP and TLS modules and the resolve stage.
pub fn registry_with(
    dns: Box<dyn Module>,
    facts: Box<dyn FactsProvider>,
    http: Box<dyn Module>,
    email: Box<dyn Module>,
    ip: Box<dyn Module>,
    tls: Box<dyn Module>,
) -> Registry {
    Registry::new()
        .with(dns)
        .with(http)
        .with(email)
        .with(ip)
        .with(tls)
        .with_facts(facts)
}

/// A registry whose DNS module runs the prism golden `prism.sse`, whose HTTP module runs the
/// spectra golden `http`, whose email module runs the beacon golden `email`, whose IP module
/// answers the ifconfig golden and whose TLS module runs the tlsight golden
/// `tlsight-inspect.json` (all from `tests/fixtures/contracts/`); `None` for `http` makes the
/// HTTP section incomplete.
pub fn registry(http: Option<&str>, email: &str) -> Registry {
    registry_with(
        dns_golden("prism.sse"),
        facts_golden("prism.sse"),
        http_module(http),
        email_golden(email),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    )
}

/// Moves tokio's clock past the per-domain admission throttle (v2-admission R12/R16). Needs a
/// current_thread runtime: `pause` panics on a multi-thread one.
pub async fn pass_throttle() {
    tokio::time::pause();
    tokio::time::advance(netray_web::admission::THROTTLE).await;
    tokio::time::resume();
}

/// The resolve budget of `tests/fixtures/lens.production.toml` and of lens's defaults.
pub const RESOLVE_BUDGET: Duration = Duration::from_millis(1500);

/// The apex configuration of lens's `tests/fixtures/lens.production.toml`: two trusted proxy
/// networks, the cache on with a 300 s window, 10 per minute and a burst of 3 per IP, 100 and
/// 20 globally, a budget of 30 per hour and 8 slots; badges, OG cards, `[api]` and `[inspect]`
/// at their defaults.
pub fn production_config() -> ApexConfig {
    ApexConfig {
        trusted_proxies: vec!["172.31.0.0/24".into(), "172.30.0.0/24".into()],
        limits: AdmissionLimits {
            client_budget_per_hour: 30,
            domain_run_slots: 8,
            rate_limit_per_ip_per_minute: 10,
            rate_limit_per_ip_burst: 3,
            rate_limit_global_per_minute: 100,
            rate_limit_global_burst: 20,
        },
        cache_enabled: true,
        cache_window_seconds: 300,
        ..ApexConfig::default()
    }
}

/// The section plan of `tests/fixtures/lens.production.toml` over `registry`: 15 s windows for
/// DNS, TLS, HTTP and email, 2 s for IP.
pub fn production_plan(registry: &Registry) -> Vec<(Protocol, Duration)> {
    netray_engine::section_plan(registry, |p| {
        Some(Duration::from_millis(match p {
            Protocol::Ip => 2000,
            _ => 15000,
        }))
    })
}

/// The section plan of lens's default configuration over `registry`: DNS, TLS and IP with 2 s
/// windows; no HTTP and no email section (lens ran those only with `[backends.http]` and
/// `[backends.email]`).
pub fn default_plan(registry: &Registry) -> Vec<(Protocol, Duration)> {
    netray_engine::section_plan(registry, |p| match p {
        Protocol::Http | Protocol::Email => None,
        _ => Some(Duration::from_millis(2000)),
    })
}

/// The engine's current profile, which lens loaded without `[scoring] profile_path`.
pub fn profile() -> Profile {
    Profiles::current().clone()
}

/// The apex state of `config` over `registry` with `plan`, [`RESOLVE_BUDGET`] and the current
/// profile; no history store.
pub fn state_with(
    config: ApexConfig,
    registry: Registry,
    plan: Vec<(Protocol, Duration)>,
) -> ApexState {
    ApexState::new(config, registry, plan, RESOLVE_BUDGET, profile())
}

/// The state lens built from `tests/fixtures/lens.production.toml` over `registry`.
pub fn production_state(registry: Registry) -> ApexState {
    let plan = production_plan(&registry);
    state_with(production_config(), registry, plan)
}

/// An open, migrated history store in a fresh temporary directory; keep the directory alive.
pub async fn history_store() -> (Arc<netray_store::HistoryStore>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = netray_store::HistoryStore::open(&dir.path().join("history.db"))
        .await
        .unwrap();
    store.migrate().await.unwrap();
    (Arc::new(store), dir)
}
