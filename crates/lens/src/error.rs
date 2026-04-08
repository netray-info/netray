use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::ApiError;

pub use netray_common::error::{ErrorInfo, ErrorResponse};

/// Structured API errors for lens, mapping to specific HTTP status codes.
///
/// Produces JSON of the form:
/// ```json
/// {"error": {"code": "ERROR_CODE", "message": "human-readable message"}}
/// ```
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid domain: {0}")]
    DomainInvalid(String),

    #[error("domain blocked: {0}")]
    DomainBlocked(String),

    #[error("rate limited")]
    RateLimited { retry_after_secs: u64 },

    #[error("backend error from {backend}: {message}")]
    BackendError {
        backend: &'static str,
        message: String,
    },

    #[error("request timeout")]
    Timeout,

    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::DomainInvalid(_) => StatusCode::BAD_REQUEST,
            Self::DomainBlocked(_) => StatusCode::BAD_REQUEST,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::BackendError { .. } => StatusCode::BAD_GATEWAY,
            Self::Timeout => StatusCode::GATEWAY_TIMEOUT,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::DomainInvalid(_) => "DOMAIN_INVALID",
            Self::DomainBlocked(_) => "DOMAIN_BLOCKED",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::BackendError { .. } => "BACKEND_ERROR",
            Self::Timeout => "TIMEOUT",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::RateLimited { retry_after_secs } => Some(*retry_after_secs),
            _ => None,
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self.status_code() {
            StatusCode::BAD_GATEWAY => {
                tracing::warn!(error = %self, "upstream backend error");
            }
            StatusCode::GATEWAY_TIMEOUT => {
                tracing::warn!(error = %self, "request timeout");
            }
            StatusCode::INTERNAL_SERVER_ERROR => {
                tracing::error!(error = %self, "internal error");
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
    async fn domain_invalid_is_400() {
        let r = AppError::DomainInvalid("bad".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn domain_blocked_is_400() {
        let r = AppError::DomainBlocked("127.0.0.1 resolves to loopback".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rate_limited_is_429() {
        let r = AppError::RateLimited {
            retry_after_secs: 10,
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn backend_error_is_502() {
        let r = AppError::BackendError {
            backend: "dns",
            message: "connection refused".into(),
        }
        .into_response();
        assert_eq!(r.status(), StatusCode::BAD_GATEWAY);
    }

    #[tokio::test]
    async fn timeout_is_504() {
        let r = AppError::Timeout.into_response();
        assert_eq!(r.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn internal_is_500() {
        let r = AppError::Internal("unexpected state".into()).into_response();
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    // --- JSON body shape ---

    #[tokio::test]
    async fn body_has_error_code_and_message_fields() {
        let body = body_json(AppError::DomainInvalid("bad.domain".into())).await;
        assert!(body["error"]["code"].is_string(), "missing code field");
        assert!(
            body["error"]["message"].is_string(),
            "missing message field"
        );
        assert_eq!(
            body.as_object().unwrap().len(),
            1,
            "unexpected top-level fields"
        );
    }

    #[tokio::test]
    async fn domain_invalid_error_code() {
        let body = body_json(AppError::DomainInvalid("x".into())).await;
        assert_eq!(body["error"]["code"], "DOMAIN_INVALID");
    }

    #[tokio::test]
    async fn domain_blocked_error_code() {
        let body = body_json(AppError::DomainBlocked("resolves to RFC1918".into())).await;
        assert_eq!(body["error"]["code"], "DOMAIN_BLOCKED");
    }

    #[tokio::test]
    async fn rate_limited_error_code() {
        let body = body_json(AppError::RateLimited {
            retry_after_secs: 30,
        })
        .await;
        assert_eq!(body["error"]["code"], "RATE_LIMITED");
    }

    #[tokio::test]
    async fn backend_error_code_and_message() {
        let body = body_json(AppError::BackendError {
            backend: "tls",
            message: "upstream unavailable".into(),
        })
        .await;
        assert_eq!(body["error"]["code"], "BACKEND_ERROR");
        assert!(
            body["error"]["message"].as_str().unwrap().contains("tls"),
            "message should contain the backend name"
        );
    }

    #[tokio::test]
    async fn timeout_error_code() {
        let body = body_json(AppError::Timeout).await;
        assert_eq!(body["error"]["code"], "TIMEOUT");
    }

    #[tokio::test]
    async fn internal_error_code() {
        let body = body_json(AppError::Internal("cache poisoned".into())).await;
        assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    }

    // --- Retry-After header ---

    #[tokio::test]
    async fn rate_limited_includes_retry_after_header() {
        let (status, headers, _body) = into_parts(AppError::RateLimited {
            retry_after_secs: 42,
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
        let (_, headers, _) = into_parts(AppError::DomainInvalid("x".into())).await;
        assert!(
            headers.get(axum::http::header::RETRY_AFTER).is_none(),
            "non-rate-limited errors must not include Retry-After"
        );
    }
}
