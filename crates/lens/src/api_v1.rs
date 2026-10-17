//! lens's side of the API v1 seam: the `DomainRuns` the apex API runs domains through.
use std::net::IpAddr;

use futures::future::BoxFuture;
use netray_web::api::{DomainRefusal, DomainRuns, RunStart, RunTrigger};

use crate::input;
use crate::state::AppState;

pub struct LensRuns {
    pub state: AppState,
}

impl LensRuns {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

impl DomainRuns for LensRuns {
    fn validate(&self, domain: &str) -> Result<String, DomainRefusal> {
        input::validate_domain(domain).map_err(|e| DomainRefusal {
            code: "DOMAIN_INVALID",
            message: e.to_string(),
        })
    }

    fn start<'a>(
        &'a self,
        _domain: &'a str,
        _client: IpAddr,
        _trigger: RunTrigger,
        _prefetch: bool,
    ) -> BoxFuture<'a, RunStart> {
        Box::pin(async { RunStart::NotStarted })
    }
}
