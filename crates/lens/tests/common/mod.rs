//! Shared by the lens tests that run the HTTP section through the engine registry: the HTTP
//! module is `netray_http::testing::golden_module` on a spectra contract golden, or, for a
//! scenario where the HTTP section fails, a module answering `SectionOutcome::Incomplete`.

use std::path::PathBuf;

use netray_engine::{BoxFuture, EvidencePath, Facts, Module, Registry, RunContext, SectionOutcome};
use netray_model::{CheckId, Protocol};

/// An HTTP module whose run is always incomplete: what lens saw as "spectra answered HTTP 500".
struct IncompleteHttp;

impl Module for IncompleteHttp {
    fn protocol(&self) -> Protocol {
        Protocol::Http
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
                reason: "http module failed".to_string(),
            }
        })
    }
}

/// A registry whose HTTP module runs the spectra contract golden `file` (from
/// `tests/fixtures/contracts/`); `None` makes the HTTP section incomplete.
pub fn http_registry(file: Option<&str>) -> Registry {
    let module: Box<dyn Module> = match file {
        Some(name) => {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/contracts")
                .join(name);
            let json = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("golden {} unreadable: {e}", path.display()));
            netray_http::testing::golden_module(&json)
        }
        None => Box::new(IncompleteHttp),
    };
    Registry::new().with(module)
}
