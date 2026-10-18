//! The v1 error type: every non-2xx answer is `{"error":{"code","message"}}`.
use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::{ApiError, build_error_response};
use netray_email::EmailRefusal;
use netray_http::HttpRefusal;
use netray_ip::IpRefusal;
use netray_tls::TlsRefusal;

#[derive(Debug)]
pub enum V1Error {
    InvalidInput(String),
    BatchTooMany(String),
    TargetBlocked(String),
    DomainInvalid(String),
    NotFound(String),
    Gone(String),
    Internal(String),
    Unavailable(String),
    MethodNotAllowed,
    PayloadTooLarge,
    RateLimited { retry_after_secs: u64 },
    Throttled { retry_after_secs: u64 },
    BudgetExhausted { retry_after_secs: u64 },
    Busy { retry_after_secs: u64 },
    PrefetchRefused,
}

impl fmt::Display for V1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg)
            | Self::BatchTooMany(msg)
            | Self::TargetBlocked(msg)
            | Self::DomainInvalid(msg)
            | Self::NotFound(msg)
            | Self::Gone(msg)
            | Self::Internal(msg)
            | Self::Unavailable(msg) => f.write_str(msg),
            Self::MethodNotAllowed => f.write_str("method not allowed"),
            Self::PayloadTooLarge => f.write_str("request body too large"),
            Self::RateLimited { .. } => f.write_str("rate limit exceeded"),
            Self::Throttled { .. } => f.write_str("the domain was checked moments ago"),
            Self::BudgetExhausted { .. } => f.write_str("the hourly run budget is spent"),
            Self::Busy { .. } => f.write_str("every run slot is taken"),
            Self::PrefetchRefused => f.write_str("a prefetch does not start a run"),
        }
    }
}

impl ApiError for V1Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) | Self::BatchTooMany(_) | Self::DomainInvalid(_) => {
                StatusCode::BAD_REQUEST
            }
            Self::TargetBlocked(_) => StatusCode::FORBIDDEN,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Gone(_) => StatusCode::GONE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited { .. }
            | Self::Throttled { .. }
            | Self::BudgetExhausted { .. }
            | Self::Busy { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::PrefetchRefused => StatusCode::SERVICE_UNAVAILABLE,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::BatchTooMany(_) => "BATCH_TOO_MANY",
            Self::TargetBlocked(_) => "TARGET_BLOCKED",
            Self::DomainInvalid(_) => "DOMAIN_INVALID",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Gone(_) => "GONE",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Unavailable(_) => "UNAVAILABLE",
            Self::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            Self::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Throttled { .. } => "THROTTLED",
            Self::BudgetExhausted { .. } => "BUDGET_EXHAUSTED",
            Self::Busy { .. } => "BUSY",
            Self::PrefetchRefused => "PREFETCH_REFUSED",
        }
    }

    fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_secs }
            | Self::Throttled { retry_after_secs }
            | Self::BudgetExhausted { retry_after_secs }
            | Self::Busy { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }
}

impl From<IpRefusal> for V1Error {
    fn from(refusal: IpRefusal) -> Self {
        match refusal {
            IpRefusal::InvalidInput(msg) => Self::InvalidInput(msg),
            IpRefusal::Unavailable(msg) => Self::Unavailable(msg),
        }
    }
}

impl From<TlsRefusal> for V1Error {
    fn from(refusal: TlsRefusal) -> Self {
        match refusal {
            TlsRefusal::InvalidInput(msg) => Self::InvalidInput(msg),
            TlsRefusal::Blocked(msg) => Self::TargetBlocked(msg),
            TlsRefusal::RateLimited { retry_after_secs } => Self::RateLimited { retry_after_secs },
            TlsRefusal::Unavailable(msg) => Self::Unavailable(msg),
        }
    }
}

impl From<HttpRefusal> for V1Error {
    fn from(refusal: HttpRefusal) -> Self {
        match refusal {
            HttpRefusal::InvalidInput(msg) => Self::InvalidInput(msg),
            HttpRefusal::Blocked(msg) => Self::TargetBlocked(msg),
            HttpRefusal::RateLimited { retry_after_secs } => Self::RateLimited { retry_after_secs },
            HttpRefusal::Unavailable(msg) => Self::Unavailable(msg),
        }
    }
}

impl From<EmailRefusal> for V1Error {
    fn from(refusal: EmailRefusal) -> Self {
        match refusal {
            EmailRefusal::InvalidInput(msg) => Self::InvalidInput(msg),
            EmailRefusal::RateLimited { retry_after_secs } => {
                Self::RateLimited { retry_after_secs }
            }
            EmailRefusal::Busy => Self::Unavailable(
                "the email inspector is busy: every inspection slot is taken".into(),
            ),
            EmailRefusal::Unavailable(msg) => Self::Unavailable(msg),
        }
    }
}

impl IntoResponse for V1Error {
    fn into_response(self) -> Response {
        let mut response = build_error_response(&self);
        if matches!(self, Self::PrefetchRefused) {
            response.headers_mut().insert(
                axum::http::header::CACHE_CONTROL,
                axum::http::HeaderValue::from_static("no-store"),
            );
        }
        response
    }
}
