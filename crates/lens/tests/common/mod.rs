//! Shared by the lens tests that run the TLS, HTTP, email and IP sections through the engine
//! registry: the TLS module is `netray_tls::testing::golden_module` on a tlsight contract golden,
//! the HTTP module is `netray_http::testing::golden_module` on a spectra contract golden, the
//! email module `netray_email::testing::golden_module` on a beacon `.sse` golden, the IP module
//! `netray_ip::testing::golden_module` on the ifconfig contract golden; for a scenario where a
//! section fails, a module answering `SectionOutcome::Incomplete`. The IP module takes its
//! addresses from the DNS backend's section through `Facts`.

// Every test crate compiles this file whole but uses only some helpers.
#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lens::backends::{Backend, BackendContext, BackendResult};
use lens::check::SectionError;
use lens::modules::ModuleSection;
use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

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

/// Run the IP section of lens over `module` for a domain whose DNS section resolved `ips`: the
/// addresses reach the module as `Facts` through the adapter. `Err` is what lens turns into an
/// Errored section.
pub async fn run_ip(
    module: Box<dyn Module>,
    timeout: Duration,
    ips: &[&str],
) -> Result<BackendResult, SectionError> {
    let section = ModuleSection {
        registry: Arc::new(Registry::new().with(module)),
        protocol: Protocol::Ip,
        timeout,
        public_url: String::new(),
    };
    let ctx = BackendContext {
        resolved_ips: ips.iter().map(|s| s.parse().unwrap()).collect(),
        dkim_selectors: None,
        forward_headers: Default::default(),
    };
    section.run("example.com", &ctx).await
}

/// Run the TLS section of lens over `module`: the section needs no addresses from DNS. `Err` is
/// what lens turns into an Errored section.
pub async fn run_tls(
    module: Box<dyn Module>,
    timeout: Duration,
) -> Result<BackendResult, SectionError> {
    let section = ModuleSection {
        registry: Arc::new(Registry::new().with(module)),
        protocol: Protocol::Tls,
        timeout,
        public_url: String::new(),
    };
    let ctx = BackendContext {
        resolved_ips: vec![],
        dkim_selectors: None,
        forward_headers: Default::default(),
    };
    section.run("example.com", &ctx).await
}

/// A registry with the given HTTP, email, IP and TLS modules.
pub fn registry_with(
    http: Box<dyn Module>,
    email: Box<dyn Module>,
    ip: Box<dyn Module>,
    tls: Box<dyn Module>,
) -> Registry {
    Registry::new().with(http).with(email).with(ip).with(tls)
}

/// A registry whose HTTP module runs the spectra golden `http` and whose email module runs the
/// beacon golden `email` and whose IP module answers the ifconfig golden and whose TLS module
/// runs the tlsight golden `tlsight-inspect.json` (all from `tests/fixtures/contracts/`); `None`
/// for `http` makes the HTTP section incomplete.
pub fn registry(http: Option<&str>, email: &str) -> Registry {
    registry_with(
        http_module(http),
        email_golden(email),
        ip_golden("ifconfig-json.json"),
        tls_golden("tlsight-inspect.json"),
    )
}
