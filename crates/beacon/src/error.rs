use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub use netray_common::error::ErrorResponse;

#[derive(Debug, thiserror::Error)]
pub enum MailError {
    #[error("invalid domain: {0}")]
    InvalidDomain(String),

    #[error("target resolves to a private address")]
    #[allow(dead_code)]
    BlockedTarget,

    #[error("rate limited")]
    RateLimited {
        retry_after_secs: u64,
        scope: &'static str,
    },

    #[error("too many DKIM selectors (max {max})")]
    TooManySelectors { max: usize },

    #[error("invalid DKIM selector: {reason}")]
    InvalidSelector { reason: String },

    #[error("internal error: {0}")]
    #[allow(dead_code)]
    Internal(String),

    #[error("DNS resolver error: {0}")]
    DnsError(String),
}

impl netray_common::error::ApiError for MailError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidDomain(_) => StatusCode::BAD_REQUEST,
            Self::BlockedTarget => StatusCode::BAD_REQUEST,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::TooManySelectors { .. } => StatusCode::BAD_REQUEST,
            Self::InvalidSelector { .. } => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::DnsError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidDomain(_) => "INVALID_DOMAIN",
            Self::BlockedTarget => "INVALID_TARGET",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::TooManySelectors { .. } => "INVALID_DOMAIN",
            Self::InvalidSelector { .. } => "INVALID_SELECTOR",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::DnsError(_) => "INTERNAL_ERROR",
        }
    }

    fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited {
                retry_after_secs, ..
            } => Some(*retry_after_secs),
            _ => None,
        }
    }
}

impl IntoResponse for MailError {
    fn into_response(self) -> Response {
        let status = netray_common::error::ApiError::status_code(&self);
        if status.is_server_error() {
            tracing::error!(error = %self, "server error");
        } else if status == StatusCode::TOO_MANY_REQUESTS {
            tracing::warn!(error = %self, "rate limited");
        } else {
            tracing::debug!(error = %self, "client error");
        }
        netray_common::error::build_error_response(&self)
    }
}
