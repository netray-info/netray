//! The JSON errors the badge and OG card routes answer, and the domain validation of every apex
//! route that takes a domain. Copied from lens's `error.rs` and `input.rs` (spec v2-cutover,
//! Phase 6) with the variants those routes use.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use netray_common::error::ApiError;

/// Produces JSON of the form `{"error": {"code": "ERROR_CODE", "message": "..."}}`.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid domain: {0}")]
    DomainInvalid(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("invalid label: {0}")]
    InvalidLabel(String),

    #[error("render failed: {0}")]
    RenderFailed(String),
}

impl ApiError for AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::DomainInvalid(_) | Self::InvalidLabel(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) | Self::RenderFailed(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn error_code(&self) -> &'static str {
        match self {
            Self::DomainInvalid(_) => "DOMAIN_INVALID",
            Self::InvalidLabel(_) => "INVALID_LABEL",
            Self::Internal(_) => "INTERNAL_ERROR",
            Self::RenderFailed(_) => "RENDER_FAILED",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match &self {
            Self::DomainInvalid(_) => tracing::debug!(error = %self, "invalid domain"),
            Self::InvalidLabel(_) => tracing::debug!(error = %self, "invalid label"),
            Self::Internal(_) => tracing::error!(error = %self, "internal error"),
            Self::RenderFailed(_) => tracing::error!(error = %self, "render failed"),
        }
        netray_common::error::build_error_response(&self)
    }
}

/// Validates and normalizes a domain name: trimmed, lowercased, without a trailing dot; refuses
/// empty input, IP addresses, wildcards, single-label names, labels over 63 and names over 253
/// characters. Resolves nothing.
pub fn validate_domain(input: &str) -> Result<String, AppError> {
    let domain = netray_common::domain::validate(input).map_err(AppError::DomainInvalid)?;
    if !domain.contains('.') {
        return Err(AppError::DomainInvalid(
            "a domain needs at least two labels".into(),
        ));
    }
    Ok(domain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(err: AppError) -> serde_json::Value {
        let response = err.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn domain_invalid_is_400() {
        let r = AppError::DomainInvalid("bad".into()).into_response();
        assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn internal_is_500() {
        let r = AppError::Internal("unexpected state".into()).into_response();
        assert_eq!(r.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

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
    async fn internal_error_code() {
        let body = body_json(AppError::Internal("cache poisoned".into())).await;
        assert_eq!(body["error"]["code"], "INTERNAL_ERROR");
    }

    #[tokio::test]
    async fn errors_have_no_retry_after() {
        let r = AppError::DomainInvalid("x".into()).into_response();
        assert!(r.headers().get(axum::http::header::RETRY_AFTER).is_none());
    }

    #[test]
    fn simple_domain_accepted() {
        assert_eq!(validate_domain("example.com").unwrap(), "example.com");
    }

    #[test]
    fn deep_subdomain_accepted() {
        assert_eq!(
            validate_domain("a.b.c.example.com").unwrap(),
            "a.b.c.example.com"
        );
    }

    #[test]
    fn mixed_case_is_lowercased() {
        assert_eq!(
            validate_domain("Sub.Example.COM").unwrap(),
            "sub.example.com"
        );
    }

    #[test]
    fn whitespace_is_trimmed() {
        assert_eq!(validate_domain("  example.com  ").unwrap(), "example.com");
    }

    #[test]
    fn trailing_dot_is_stripped() {
        assert_eq!(validate_domain("example.com.").unwrap(), "example.com");
    }

    #[test]
    fn punycode_accepted() {
        assert_eq!(
            validate_domain("xn--nxasmq6b.xn--jxalpdlp").unwrap(),
            "xn--nxasmq6b.xn--jxalpdlp"
        );
    }

    #[test]
    fn single_label_refused() {
        let err = validate_domain("localhost").unwrap_err();
        assert!(matches!(
            err,
            AppError::DomainInvalid(ref m) if m == "a domain needs at least two labels"
        ));
    }

    #[test]
    fn ip_addresses_rejected() {
        for ip in ["1.2.3.4", "127.0.0.1", "2001:db8::1", "::1"] {
            assert!(
                matches!(validate_domain(ip), Err(AppError::DomainInvalid(_))),
                "{ip}"
            );
        }
    }

    #[test]
    fn wildcards_rejected() {
        for w in ["*.example.com", "*"] {
            assert!(
                matches!(validate_domain(w), Err(AppError::DomainInvalid(_))),
                "{w}"
            );
        }
    }

    #[test]
    fn length_limits() {
        let label = "a".repeat(63);
        let too_long = format!("{label}.{label}.{label}.{label}");
        assert!(matches!(
            validate_domain(&too_long),
            Err(AppError::DomainInvalid(_))
        ));
        let l61 = "b".repeat(61);
        let exactly = format!("{label}.{label}.{label}.{l61}");
        assert_eq!(exactly.len(), 253);
        assert!(validate_domain(&exactly).is_ok());
        let long_label = format!("{}.example.com", "a".repeat(64));
        assert!(matches!(
            validate_domain(&long_label),
            Err(AppError::DomainInvalid(_))
        ));
        assert!(validate_domain(&format!("{label}.com")).is_ok());
    }

    #[test]
    fn empty_input_and_empty_labels_rejected() {
        for bad in ["", "   ", "example..com"] {
            assert!(
                matches!(validate_domain(bad), Err(AppError::DomainInvalid(_))),
                "{bad:?}"
            );
        }
    }
}
