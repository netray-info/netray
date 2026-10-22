//! The HTTP inspection as a function with no axum type: parse, limit, resolve under the target
//! policy, inspect. netray-web runs it.

use std::net::{IpAddr, SocketAddr};
use std::time::Instant;

use netray_engine::BoxFuture;
use serde::Deserialize;
use url::Url;
use utoipa::ToSchema;

use crate::error::AppError;
use crate::inspect::assembler::InspectResponse;
use crate::security::RateLimitState;

/// What `inspect_url` needs from the outside: the target resolution and the inspection itself.
pub trait HttpInspect: Send + Sync + 'static {
    /// Resolve the URL's host and apply the target policy.
    fn target<'a>(&'a self, url: &'a Url) -> BoxFuture<'a, Result<SocketAddr, AppError>>;

    /// Inspect `url` at `target`; `start` is when the request began.
    fn inspect<'a>(
        &'a self,
        url: &'a Url,
        target: SocketAddr,
        start: Instant,
    ) -> BoxFuture<'a, Result<InspectResponse, AppError>>;
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct HttpInspectRequest {
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpRefusal {
    #[error("{0}")]
    InvalidInput(String),
    #[error("{0}")]
    Blocked(String),
    #[error("rate limit exceeded")]
    RateLimited { retry_after_secs: u64 },
    #[error("{0}")]
    Unavailable(String),
}

impl From<AppError> for HttpRefusal {
    fn from(err: AppError) -> Self {
        let text = err.to_string();
        match err {
            AppError::InvalidUrl(_) => Self::InvalidInput(text),
            AppError::BlockedTarget(_) => Self::Blocked(text),
            AppError::RateLimited {
                retry_after_secs, ..
            } => Self::RateLimited { retry_after_secs },
            AppError::InvalidTarget(_) | AppError::Timeout(_) => Self::Unavailable(text),
        }
    }
}

/// Parse the URL, charge the caller and the host, resolve the target, inspect.
async fn inspect_url(
    raw_url: &str,
    client: IpAddr,
    limits: &RateLimitState,
    backend: &dyn HttpInspect,
) -> Result<InspectResponse, AppError> {
    let start = Instant::now();
    let url = crate::input::parse_url(raw_url)?;
    limits.check(client, url.host_str().unwrap_or_default())?;
    let target = backend.target(&url).await?;
    backend.inspect(&url, target, start).await
}

/// `inspect_url` with the refusals as `HttpRefusal`.
pub async fn inspect(
    req: &HttpInspectRequest,
    client: IpAddr,
    limits: &RateLimitState,
    backend: &dyn HttpInspect,
) -> Result<InspectResponse, HttpRefusal> {
    inspect_url(&req.url, client, limits, backend)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_app_error_maps_to_its_refusal() {
        let cases = [
            (
                AppError::InvalidUrl("x".into()),
                HttpRefusal::InvalidInput("invalid URL: x".into()),
            ),
            (
                AppError::BlockedTarget("x".into()),
                HttpRefusal::Blocked("blocked target: x".into()),
            ),
            (
                AppError::RateLimited {
                    retry_after_secs: 7,
                    scope: "per_ip",
                },
                HttpRefusal::RateLimited {
                    retry_after_secs: 7,
                },
            ),
            (
                AppError::InvalidTarget("x".into()),
                HttpRefusal::Unavailable("invalid target: x".into()),
            ),
            (
                AppError::Timeout(30),
                HttpRefusal::Unavailable("inspection timed out after 30s".into()),
            ),
        ];
        for (err, want) in cases {
            assert_eq!(HttpRefusal::from(err), want);
        }
    }
}
