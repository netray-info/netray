use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// JSON body returned for all error responses.
///
/// Wire format: `{"error": {"code": "...", "message": "..."}}`
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorResponse {
    pub error: ErrorInfo,
}

/// Error detail contained in an error response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ErrorInfo {
    /// Machine-readable error code (e.g. `INVALID_HOSTNAME`).
    pub code: &'static str,
    /// Human-readable error message.
    pub message: String,
}

/// Structured API errors that map to specific HTTP status codes and error codes.
///
/// Each variant corresponds to a documented error code (SDD §5.5). The `IntoResponse`
/// implementation produces a JSON body of the form:
/// ```json
/// {"error": {"code": "ERROR_CODE", "message": "human-readable message"}}
/// ```
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid hostname: {0}")]
    InvalidHostname(String),

    #[error("invalid port: {0}")]
    InvalidPort(String),

    #[error("parse error: {0}")]
    ParseError(String),

    #[error("ambiguous input: {0}")]
    AmbiguousInput(String),

    #[error("too many ports: {requested} exceeds limit of {max}")]
    TooManyPorts { requested: usize, max: usize },

    #[error("blocked target: {0}")]
    BlockedTarget(String),

    #[error("rate limited ({scope})")]
    RateLimited {
        retry_after_secs: u64,
        scope: &'static str,
    },

    #[error("DNS resolution failed: {0}")]
    DnsResolutionFailed(String),

    #[allow(dead_code)] // Used by TLS connect error paths
    #[error("connection failed: {0}")]
    ConnectionFailed(String),

    #[allow(dead_code)] // Used by TLS connect error paths
    #[error("TLS handshake failed: {0}")]
    HandshakeFailed(String),

    #[allow(dead_code)] // Used by cert parsing error paths
    #[error("certificate error: {0}")]
    CertificateError(String),

    #[error("request timeout")]
    RequestTimeout,
}

impl AppError {
    /// Returns the HTTP status code for this error variant.
    fn status_code(&self) -> StatusCode {
        match self {
            // 400 Bad Request — malformed input.
            Self::InvalidHostname(_)
            | Self::InvalidPort(_)
            | Self::ParseError(_)
            | Self::AmbiguousInput(_) => StatusCode::BAD_REQUEST,

            // 403 Forbidden — blocked target.
            Self::BlockedTarget(_) => StatusCode::FORBIDDEN,

            // 422 Unprocessable Entity — valid syntax but policy-rejected.
            Self::TooManyPorts { .. } => StatusCode::UNPROCESSABLE_ENTITY,

            // 429 Too Many Requests.
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,

            // 502 Bad Gateway — upstream connection/TLS failures.
            Self::DnsResolutionFailed(_)
            | Self::ConnectionFailed(_)
            | Self::HandshakeFailed(_)
            | Self::CertificateError(_) => StatusCode::BAD_GATEWAY,

            // 504 Gateway Timeout.
            Self::RequestTimeout => StatusCode::GATEWAY_TIMEOUT,
        }
    }

    /// Returns the machine-readable error code string for this variant.
    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidHostname(_) => "INVALID_HOSTNAME",
            Self::InvalidPort(_) => "INVALID_PORT",
            Self::ParseError(_) => "PARSE_ERROR",
            Self::AmbiguousInput(_) => "AMBIGUOUS_INPUT",
            Self::TooManyPorts { .. } => "TOO_MANY_PORTS",
            Self::BlockedTarget(_) => "BLOCKED_TARGET",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::DnsResolutionFailed(_) => "DNS_RESOLUTION_FAILED",
            Self::ConnectionFailed(_) => "CONNECTION_FAILED",
            Self::HandshakeFailed(_) => "HANDSHAKE_FAILED",
            Self::CertificateError(_) => "CERTIFICATE_ERROR",
            Self::RequestTimeout => "REQUEST_TIMEOUT",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();

        match status {
            StatusCode::BAD_GATEWAY => {
                tracing::warn!(error = %self, "upstream error");
            }
            StatusCode::GATEWAY_TIMEOUT => {
                tracing::warn!(error = %self, "request timeout");
            }
            _ => {}
        }

        let body = ErrorResponse {
            error: ErrorInfo {
                code: self.error_code(),
                message: self.to_string(),
            },
        };

        let mut response = (status, axum::Json(body)).into_response();

        // For rate-limited responses, include the Retry-After header (RFC 6585 §4).
        if let Self::RateLimited {
            retry_after_secs, ..
        } = &self
        {
            response.headers_mut().insert(
                axum::http::header::RETRY_AFTER,
                axum::http::HeaderValue::from(*retry_after_secs),
            );
        }

        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    async fn body_json(err: AppError) -> serde_json::Value {
        let response = err.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn into_parts(err: AppError) -> (StatusCode, axum::http::HeaderMap, serde_json::Value) {
        let response = err.into_response();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        (status, headers, body)
    }

    // --- Status codes ---

    #[tokio::test]
    async fn invalid_hostname_is_400() {
        let r = AppError::InvalidHostname("bad".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn invalid_port_is_400() {
        let r = AppError::InvalidPort("99999".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn parse_error_is_400() {
        let r = AppError::ParseError("oops".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn ambiguous_input_is_400() {
        let r = AppError::AmbiguousInput("POST with query string".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn too_many_ports_is_422() {
        let r = AppError::TooManyPorts {
            requested: 6,
            max: 5,
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn blocked_target_is_403() {
        let r = AppError::BlockedTarget("127.0.0.1: loopback".into()).into_response();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn rate_limited_is_429() {
        let r = AppError::RateLimited {
            retry_after_secs: 5,
            scope: "per_ip",
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn dns_resolution_failed_is_502() {
        let r = AppError::DnsResolutionFailed("NXDOMAIN".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn connection_failed_is_502() {
        let r = AppError::ConnectionFailed("connection refused".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn handshake_failed_is_502() {
        let r = AppError::HandshakeFailed("protocol error".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn certificate_error_is_502() {
        let r = AppError::CertificateError("malformed DER".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn request_timeout_is_504() {
        let r = AppError::RequestTimeout.into_response();
        assert_eq!(r.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    // --- JSON body shape ---

    #[tokio::test]
    async fn body_has_error_code_and_message_fields() {
        let body = body_json(AppError::InvalidHostname("bad.host".into())).await;
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
    async fn invalid_hostname_error_code() {
        let body = body_json(AppError::InvalidHostname("x".into())).await;
        assert_eq!(body["error"]["code"], "INVALID_HOSTNAME");
    }

    #[tokio::test]
    async fn parse_error_code_and_message() {
        let body = body_json(AppError::ParseError("unexpected token".into())).await;
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
    async fn blocked_target_error_code() {
        let body = body_json(AppError::BlockedTarget("10.0.0.1: private".into())).await;
        assert_eq!(body["error"]["code"], "BLOCKED_TARGET");
    }

    #[tokio::test]
    async fn too_many_ports_error_code_and_message() {
        let body = body_json(AppError::TooManyPorts {
            requested: 8,
            max: 5,
        })
        .await;
        assert_eq!(body["error"]["code"], "TOO_MANY_PORTS");
        let msg = body["error"]["message"].as_str().unwrap();
        assert!(msg.contains("8"), "message should include requested count");
        assert!(msg.contains("5"), "message should include max count");
    }

    #[tokio::test]
    async fn rate_limited_error_code() {
        let body = body_json(AppError::RateLimited {
            retry_after_secs: 30,
            scope: "per_ip",
        })
        .await;
        assert_eq!(body["error"]["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn request_timeout_error_code() {
        let body = body_json(AppError::RequestTimeout).await;
        assert_eq!(body["error"]["code"], "REQUEST_TIMEOUT");
    }

    // --- Retry-After header ---

    #[tokio::test]
    async fn rate_limited_includes_retry_after_header() {
        let (status, headers, _body) = into_parts(AppError::RateLimited {
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
        let (_, headers, _) = into_parts(AppError::InvalidHostname("x".into())).await;
        assert!(
            headers.get(axum::http::header::RETRY_AFTER).is_none(),
            "non-rate-limited errors must not include Retry-After"
        );
    }
}
