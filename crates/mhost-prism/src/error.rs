use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Structured API errors that map to specific HTTP status codes and error codes.
///
/// Each variant corresponds to a documented error code (SDD §5.6). The `IntoResponse`
/// implementation produces a JSON body of the form:
/// ```json
/// {"error": {"code": "ERROR_CODE", "message": "human-readable message"}}
/// ```
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid domain: {0}")]
    InvalidDomain(String),

    #[error("invalid record type: {0}")]
    InvalidRecordType(String),

    #[error("invalid server: {0}")]
    InvalidServer(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("ambiguous input: POST with query string")]
    AmbiguousInput,

    #[error("blocked query type: {0}")]
    BlockedQueryType(String),

    #[error("blocked target IP: {ip} ({reason})")]
    BlockedTargetIp { ip: String, reason: String },

    #[error("system resolvers disabled")]
    SystemResolversDisabled,

    #[error("arbitrary servers disabled")]
    ArbitraryServersDisabled,

    #[error("feature not available: {feature}")]
    FeatureNotAvailable { feature: String },

    #[error("too many record types: {requested} exceeds limit of {max}")]
    TooManyRecordTypes { requested: usize, max: usize },

    #[error("too many servers: {requested} exceeds limit of {max}")]
    TooManyServers { requested: usize, max: usize },

    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },

    #[error("resolver error: {0}")]
    ResolverError(String),

    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}

#[derive(Debug, Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl ApiError {
    /// Returns the HTTP status code for this error variant.
    fn status_code(&self) -> StatusCode {
        match self {
            // 400 Bad Request — malformed input.
            Self::InvalidDomain(_)
            | Self::InvalidRecordType(_)
            | Self::InvalidServer(_)
            | Self::ParseError(_)
            | Self::AmbiguousInput => StatusCode::BAD_REQUEST,

            // 422 Unprocessable Entity — valid syntax but policy-rejected.
            Self::BlockedQueryType(_)
            | Self::BlockedTargetIp { .. }
            | Self::SystemResolversDisabled
            | Self::ArbitraryServersDisabled
            | Self::FeatureNotAvailable { .. }
            | Self::TooManyRecordTypes { .. }
            | Self::TooManyServers { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // 429 Too Many Requests.
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,

            // 500 Internal Server Error.
            Self::ResolverError(_) | Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Returns the machine-readable error code string for this variant.
    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidDomain(_) => "INVALID_DOMAIN",
            Self::InvalidRecordType(_) => "INVALID_RECORD_TYPE",
            Self::InvalidServer(_) => "INVALID_SERVER",
            Self::ParseError(_) => "PARSE_ERROR",
            Self::AmbiguousInput => "AMBIGUOUS_INPUT",
            Self::BlockedQueryType(_) => "BLOCKED_QUERY_TYPE",
            Self::BlockedTargetIp { .. } => "BLOCKED_TARGET_IP",
            Self::SystemResolversDisabled => "SYSTEM_RESOLVERS_DISABLED",
            Self::ArbitraryServersDisabled => "ARBITRARY_SERVERS_DISABLED",
            Self::FeatureNotAvailable { .. } => "FEATURE_NOT_AVAILABLE",
            Self::TooManyRecordTypes { .. } => "TOO_MANY_RECORD_TYPES",
            Self::TooManyServers { .. } => "TOO_MANY_SERVERS",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::ResolverError(_) => "RESOLVER_ERROR",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ErrorBody {
            error: ErrorDetail {
                code: self.error_code(),
                message: self.to_string(),
            },
        };

        let mut response = (status, axum::Json(body)).into_response();

        // For rate-limited responses, include the Retry-After header (RFC 6585 §4).
        if let Self::RateLimited { retry_after_secs } = &self {
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from(*retry_after_secs),
            );
        }

        response
    }
}
