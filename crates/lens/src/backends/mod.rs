pub mod dns;
pub mod ip;
pub mod tls;

use std::net::IpAddr;
use std::time::Duration;

use crate::check::SectionError;
use crate::scoring::engine::CheckResult;

/// Section-specific extra data produced by each backend.
#[derive(Clone, Debug)]
pub enum BackendExtra {
    Dns {
        resolved_ips: Vec<IpAddr>,
        raw_headline: String,
        detail_url: String,
    },
    Tls {
        raw_headline: String,
        detail_url: String,
    },
    Ip {
        addresses: Vec<ip::IpInfo>,
        raw_headline: String,
        detail_url: String,
    },
}

#[derive(Clone, Debug)]
pub struct BackendResult {
    pub checks: Vec<CheckResult>,
    pub extra: BackendExtra,
}

/// Cross-section context passed from wave 1 to wave 2.
#[derive(Clone)]
pub struct BackendContext {
    pub resolved_ips: Vec<IpAddr>,
}

pub trait Backend: Send + Sync {
    /// Must match the key in `ScoringProfile.sections`.
    fn section(&self) -> &'static str;

    /// Run the backend. Timeout is enforced by the caller (run_backends wraps with
    /// tokio::time::timeout). Implementations must not apply their own outer timeout.
    fn run(
        &self,
        client: &reqwest::Client,
        domain: &str,
        context: &BackendContext,
        timeout: Duration,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<BackendResult, SectionError>> + Send + '_>,
    >;
}
