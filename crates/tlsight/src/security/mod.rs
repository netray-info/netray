//! Security middleware composition for the tlsight web service.
//!
//! Implements the four-layer defense-in-depth model from SDD §8:
//!
//! - **Layer 1**: Target restrictions (hardcoded) — via [`target_policy`]
//! - **Layer 2**: Rate limiting (governor GCRA) — via [`RateLimitState`]
//! - **Layer 3**: Client IP extraction — via [`IpExtractor`]
//! - **Layer 4**: HTTP security headers — via [`security_headers`] middleware

pub mod ip_extract;
pub mod rate_limit;
pub mod target_policy;

pub use ip_extract::IpExtractor;
pub use rate_limit::RateLimitState;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};
use tower_http::cors::CorsLayer;

/// Create the CORS layer for the application.
///
/// No origin allowlist is configured, so `CorsLayer` rejects all cross-origin
/// requests by default. Same-origin requests from the embedded SPA never
/// trigger CORS preflight and are unaffected.
///
/// Only GET and POST are permitted. No custom request headers are allowed
/// (the frontend uses standard `fetch` for JSON requests).
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::ACCEPT])
        .max_age(std::time::Duration::from_secs(3600))
}

/// Axum middleware that injects security headers on every response.
///
/// Uses `netray_common::security_headers::security_headers_layer` with:
/// - Relaxed CSP for `/docs` (Scalar CDN)
/// - Permissions-Policy enabled
///
/// Compatible with axum 0.8 `middleware::from_fn`.
pub async fn security_headers(request: Request, next: Next) -> Response {
    // Captures are cheap (all Strings/bools), and `from_fn` calls this per-request.
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec!["https://cdn.jsdelivr.net".to_string()],
        relaxed_csp_path_prefix: "/docs".to_string(),
        include_permissions_policy: true,
    });
    layer_fn(request, next).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::middleware;
    use axum::routing::get;
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    async fn make_response_with_security_headers() -> Response {
        let app = Router::new()
            .route("/test", get(ok_handler))
            .layer(middleware::from_fn(security_headers));

        let request = HttpRequest::builder()
            .uri("/test")
            .body(Body::empty())
            .unwrap();

        app.oneshot(request).await.unwrap()
    }

    #[tokio::test]
    async fn sets_content_security_policy() {
        let response = make_response_with_security_headers().await;
        let csp = response
            .headers()
            .get("content-security-policy")
            .expect("CSP header present")
            .to_str()
            .unwrap();

        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("script-src 'self'"));
        assert!(csp.contains("style-src 'self' 'unsafe-inline'"));
        assert!(csp.contains("connect-src 'self'"));
        assert!(csp.contains("img-src 'self' data:"));
        assert!(csp.contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn sets_x_content_type_options() {
        let response = make_response_with_security_headers().await;
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn sets_x_frame_options() {
        let response = make_response_with_security_headers().await;
        assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn sets_referrer_policy() {
        let response = make_response_with_security_headers().await;
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }

    #[tokio::test]
    async fn sets_strict_transport_security() {
        let response = make_response_with_security_headers().await;
        assert_eq!(
            response.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[tokio::test]
    async fn handler_still_returns_ok() {
        let response = make_response_with_security_headers().await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn sets_permissions_policy() {
        let response = make_response_with_security_headers().await;
        let pp = response
            .headers()
            .get("permissions-policy")
            .expect("Permissions-Policy header present")
            .to_str()
            .unwrap();
        assert!(pp.contains("geolocation=()"));
        assert!(pp.contains("microphone=()"));
        assert!(pp.contains("camera=()"));
    }

    #[test]
    fn cors_layer_builds_without_panic() {
        let _ = cors_layer();
    }
}
