//! Shared by the lens tests that run the HTTP and email sections through the engine registry:
//! the HTTP module is `netray_http::testing::golden_module` on a spectra contract golden, the
//! email module `netray_email::testing::golden_module` on a beacon `.sse` golden; for a scenario
//! where a section fails, a module answering `SectionOutcome::Incomplete`.

// Every test crate compiles this file whole but uses only some helpers.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::Duration;

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

/// An HTTP module that runs the spectra contract golden `file`; `None` makes it incomplete.
pub fn http_module(file: Option<&str>) -> Box<dyn Module> {
    match file {
        Some(name) => netray_http::testing::golden_module(&golden(name)),
        None => Box::new(Incomplete(Protocol::Http)),
    }
}

/// A registry with the given HTTP and email modules.
pub fn registry_with(http: Box<dyn Module>, email: Box<dyn Module>) -> Registry {
    Registry::new().with(http).with(email)
}

/// A registry whose HTTP module runs the spectra golden `http` and whose email module runs the
/// beacon golden `email` (both from `tests/fixtures/contracts/`); `None` for `http` makes the
/// HTTP section incomplete.
pub fn registry(http: Option<&str>, email: &str) -> Registry {
    registry_with(http_module(http), email_golden(email))
}
