use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::ApiError;

pub use netray_common::error::ErrorResponse;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    #[error("blocked target: {0}")]
    BlockedTarget(String),

    #[error("invalid target: {0}")]
    InvalidTarget(String),

    #[error("rate limited ({scope})")]
    RateLimited {
        retry_after_secs: u64,
        scope: &'static str,
    },

    #[error("inspection timed out after {0}s")]
    Timeout(u64),

    #[allow(dead_code)] // Used by inspection error paths
    #[error("connection failed: {0}")]
    ConnectionFailed(String),
}

impl ApiError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidUrl(_) => StatusCode::BAD_REQUEST,
            Self::BlockedTarget(_) => StatusCode::FORBIDDEN,
            Self::InvalidTarget(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            Self::Timeout(_) => StatusCode::GATEWAY_TIMEOUT,
            Self::ConnectionFailed(_) => StatusCode::BAD_GATEWAY,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::InvalidUrl(_) => "INVALID_URL",
            Self::BlockedTarget(_) => "TARGET_BLOCKED",
            Self::InvalidTarget(_) => "INVALID_TARGET",
            Self::RateLimited { .. } => "RATE_LIMITED",
            Self::Timeout(_) => "TIMEOUT",
            Self::ConnectionFailed(_) => "CONNECTION_FAILED",
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
            StatusCode::TOO_MANY_REQUESTS => {
                tracing::warn!(error = %self, "rate limited");
            }
            StatusCode::FORBIDDEN => {
                tracing::warn!(error = %self, "blocked target");
            }
            s if s.is_client_error() => {
                tracing::debug!(error = %self, "client error");
            }
            _ => {}
        }

        netray_common::error::build_error_response(&self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;
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

    #[tokio::test]
    async fn invalid_url_is_400() {
        let r = AppError::InvalidUrl("bad".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn blocked_target_is_403() {
        let r = AppError::BlockedTarget("127.0.0.1".into()).into_response();
        assert_eq!(r.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn invalid_target_is_422() {
        let r = AppError::InvalidTarget("no records".into()).into_response();
        assert_eq!(r.status(), StatusCode::UNPROCESSABLE_ENTITY);
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
    async fn timeout_is_504() {
        let r = AppError::Timeout(30).into_response();
        assert_eq!(r.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn body_has_error_code_and_message() {
        let body = body_json(AppError::InvalidUrl("bad url".into())).await;
        assert_eq!(body["error"]["code"], "INVALID_URL");
        assert!(body["error"]["message"].is_string());
        assert_eq!(body.as_object().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn rate_limited_includes_retry_after_header() {
        let (status, headers, _) = into_parts(AppError::RateLimited {
            retry_after_secs: 42,
            scope: "per_ip",
        })
        .await;
        assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        let retry_after = headers
            .get(axum::http::header::RETRY_AFTER)
            .expect("Retry-After must be present");
        let value: u64 = retry_after.to_str().unwrap().parse().unwrap();
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn non_rate_limited_has_no_retry_after() {
        let (_, headers, _) = into_parts(AppError::InvalidUrl("x".into())).await;
        assert!(headers.get(axum::http::header::RETRY_AFTER).is_none());
    }
}
