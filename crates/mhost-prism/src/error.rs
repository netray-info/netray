use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::ApiError as _;

pub use netray_common::error::{ErrorInfo, ErrorResponse};

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

    #[allow(dead_code)] // defensive variant: POST with both body and query string; not yet wired to a handler
    #[error("ambiguous input: POST with query string")]
    AmbiguousInput,

    #[error("blocked target IP: {ip} ({reason})")]
    BlockedTargetIp { ip: String, reason: String },

    #[error("system resolvers disabled")]
    SystemResolversDisabled,

    #[error("arbitrary servers disabled")]
    ArbitraryServersDisabled,

    #[error("too many record types: {requested} exceeds limit of {max}")]
    TooManyRecordTypes { requested: usize, max: usize },

    #[error("too many servers: {requested} exceeds limit of {max}")]
    TooManyServers { requested: usize, max: usize },

    #[error("rate limited ({scope})")]
    RateLimited {
        retry_after_secs: u64,
        scope: &'static str,
    },

    #[error("resolver error: {0}")]
    ResolverError(String),
}

impl netray_common::error::ApiError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidDomain(_)
            | Self::InvalidRecordType(_)
            | Self::InvalidServer(_)
            | Self::ParseError(_)
            | Self::AmbiguousInput => StatusCode::BAD_REQUEST,

            Self::BlockedTargetIp { .. }
            | Self::SystemResolversDisabled
            | Self::ArbitraryServersDisabled
            | Self::TooManyRecordTypes { .. }
            | Self::TooManyServers { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,

            Self::ResolverError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidDomain(_) => "INVALID_DOMAIN",
            Self::InvalidRecordType(_) => "INVALID_RECORD_TYPE",
            Self::InvalidServer(_) => "INVALID_SERVER",
            Self::ParseError(_) => "PARSE_ERROR",
            Self::AmbiguousInput => "AMBIGUOUS_INPUT",
            Self::BlockedTargetIp { .. } => "BLOCKED_TARGET_IP",
            Self::SystemResolversDisabled => "SYSTEM_RESOLVERS_DISABLED",
            Self::ArbitraryServersDisabled => "ARBITRARY_SERVERS_DISABLED",
            Self::TooManyRecordTypes { .. } => "TOO_MANY_RECORD_TYPES",
            Self::TooManyServers { .. } => "TOO_MANY_SERVERS",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::ResolverError(_) => "RESOLVER_ERROR",
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

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match &self {
            Self::RateLimited { .. } => {
                tracing::warn!(error = %self, "rate limited");
            }
            Self::BlockedTargetIp { .. } => {
                tracing::warn!(error = %self, "blocked target");
            }
            Self::ResolverError(_) => {
                tracing::error!(error = %self, "resolver error");
            }
            _ if self.status_code().is_client_error() => {
                tracing::debug!(error = %self, "client error");
            }
            _ => {}
        }

        netray_common::error::into_error_response(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    async fn body_json(err: ApiError) -> serde_json::Value {
        let response = err.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn into_parts(err: ApiError) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, headers, body)
    }

    // --- Status codes ---

    #[tokio::test]
    async fn invalid_domain_is_400() {
        let r = ApiError::InvalidDomain("bad".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_record_type_is_400() {
        let r = ApiError::InvalidRecordType("X".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_server_is_400() {
        let r = ApiError::InvalidServer("x".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn parse_error_is_400() {
        let r = ApiError::ParseError("oops".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ambiguous_input_is_400() {
        let r = ApiError::AmbiguousInput.into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn blocked_target_ip_is_422() {
        let r = ApiError::BlockedTargetIp {
            ip: "127.0.0.1".into(),
            reason: "loopback".into(),
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn system_resolvers_disabled_is_422() {
        let r = ApiError::SystemResolversDisabled.into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn arbitrary_servers_disabled_is_422() {
        let r = ApiError::ArbitraryServersDisabled.into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn too_many_record_types_is_422() {
        let r = ApiError::TooManyRecordTypes {
            requested: 11,
            max: 10,
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn too_many_servers_is_422() {
        let r = ApiError::TooManyServers {
            requested: 5,
            max: 4,
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn rate_limited_is_429() {
        let r = ApiError::RateLimited {
            retry_after_secs: 5,
            scope: "per_ip",
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn resolver_error_is_500() {
        let r = ApiError::ResolverError("timeout".into()).into_response();
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // --- JSON body shape ---

    #[tokio::test]
    async fn body_has_error_code_and_message_fields() {
        let body = body_json(ApiError::InvalidDomain("bad.domain".into())).await;
        assert!(body["error"]["code"].is_string(), "missing code field");
        assert!(
            body["error"]["message"].is_string(),
            "missing message field"
        );
        // No extra top-level keys beyond "error"
        assert_eq!(
            body.as_object().unwrap().len(),
            1,
            "unexpected top-level fields"
        );
    }

    #[tokio::test]
    async fn invalid_domain_error_code() {
        let body = body_json(ApiError::InvalidDomain("x".into())).await;
        assert_eq!(body["error"]["code"], "INVALID_DOMAIN");
    }

    #[tokio::test]
    async fn parse_error_code_and_message() {
        let body = body_json(ApiError::ParseError("unexpected token".into())).await;
        assert_eq!(body["error"]["code"], "PARSE_ERROR");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap()
                .contains("unexpected token"),
            "message should contain the parse error detail"
        );
    }

    #[tokio::test]
    async fn blocked_target_ip_error_code() {
        let body = body_json(ApiError::BlockedTargetIp {
            ip: "10.0.0.1".into(),
            reason: "private".into(),
        })
        .await;
        assert_eq!(body["error"]["code"], "BLOCKED_TARGET_IP");
    }

    #[tokio::test]
    async fn too_many_record_types_error_code_and_message() {
        let body = body_json(ApiError::TooManyRecordTypes {
            requested: 15,
            max: 10,
        })
        .await;
        assert_eq!(body["error"]["code"], "TOO_MANY_RECORD_TYPES");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("15"), "message should include requested count");
        assert!(msg.contains("10"), "message should include max count");
    }

    #[tokio::test]
    async fn rate_limited_error_code() {
        let body = body_json(ApiError::RateLimited {
            retry_after_secs: 30,
            scope: "per_ip",
        })
        .await;
        assert_eq!(body["error"]["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn resolver_error_code() {
        let body = body_json(ApiError::ResolverError("dns timeout".into())).await;
        assert_eq!(body["error"]["code"], "RESOLVER_ERROR");
    }

    // --- Retry-After header ---

    #[tokio::test]
    async fn rate_limited_includes_retry_after_header() {
        let (status, headers, _body) = into_parts(ApiError::RateLimited {
            retry_after_secs: 42,
            scope: "per_ip",
        })
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        let retry_after = headers
            .get(axum::http::header::RETRY_AFTER)
            .expect("Retry-After header must be present");
        let value: u64 = retry_after.to_str().unwrap().parse().unwrap();
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn non_rate_limited_errors_have_no_retry_after() {
        let (_, headers, _) = into_parts(ApiError::InvalidDomain("x".into())).await;
        assert!(
            headers.get(axum::http::header::RETRY_AFTER).is_none(),
            "non-rate-limited errors must not include Retry-After"
        );
    }
}
