//! The v1 error type: every non-2xx answer is `{"error":{"code","message"}}`.
use std::fmt;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::{ApiError, build_error_response};

#[derive(Debug)]
pub enum V1Error {
    InvalidInput(String),
    DomainInvalid(String),
    NotFound(String),
    Gone(String),
    Internal(String),
    Unavailable(String),
    MethodNotAllowed,
    PayloadTooLarge,
    RateLimited { retry_after_secs: u64 },
}

impl fmt::Display for V1Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg)
            | Self::DomainInvalid(msg)
            | Self::NotFound(msg)
            | Self::Gone(msg)
            | Self::Internal(msg)
            | Self::Unavailable(msg) => f.write_str(msg),
            Self::MethodNotAllowed => f.write_str("method not allowed"),
            Self::PayloadTooLarge => f.write_str("request body too large"),
            Self::RateLimited { .. } => f.write_str("rate limit exceeded"),
        }
    }
}

impl ApiError for V1Error {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidInput(_) | Self::DomainInvalid(_) => StatusCode::BAD_REQUEST,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::Gone(_) => StatusCode::GONE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::Unavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
            Self::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "INVALID_INPUT",
            Self::DomainInvalid(_) => "DOMAIN_INVALID",
            Self::NotFound(_) => "NOT_FOUND",
            Self::Gone(_) => "GONE",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::Unavailable(_) => "UNAVAILABLE",
            Self::MethodNotAllowed => "METHOD_NOT_ALLOWED",
            Self::PayloadTooLarge => "PAYLOAD_TOO_LARGE",
            Self::RateLimited { .. } => "RATE_LIMITED",
        }
    }

    fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }
}

impl IntoResponse for V1Error {
    fn into_response(self) -> Response {
        build_error_response(&self)
    }
}
