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
/// Headers applied (per SDD §8.4):
/// - `Content-Security-Policy`: Restricts resource loading to same origin.
///   `style-src 'unsafe-inline'` is required for inline styles.
///   `frame-ancestors 'none'` prevents clickjacking (equivalent to X-Frame-Options: DENY).
/// - `X-Content-Type-Options: nosniff`: Prevents MIME-type sniffing.
/// - `X-Frame-Options: DENY`: Legacy clickjacking protection (superseded by CSP
///   frame-ancestors but still respected by older browsers).
/// - `Referrer-Policy: strict-origin-when-cross-origin`: Limits referrer leakage.
/// - `Strict-Transport-Security`: Signals HTTPS-only to browsers. Only meaningful
///   when TLS is terminated by a reverse proxy in front of tlsight; harmless otherwise.
///
/// Body size is limited to 4KB for API requests (hostname + optional JSON body).
///
/// Compatible with axum 0.8 `middleware::from_fn`.
pub async fn security_headers(request: Request, next: Next) -> Response {
    // Capture path before consuming the request.
    let is_docs = request.uri().path().starts_with("/docs");

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    // The API docs page loads Scalar from a CDN, so it needs a relaxed script-src.
    // All other pages keep the strict same-origin policy.
    let csp = if is_docs {
        "default-src 'self'; script-src 'self' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'"
    } else {
        "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self'; img-src 'self' data:; frame-ancestors 'none'"
    };
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        csp.parse().expect("valid CSP header value"),
    );

    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        "nosniff".parse().expect("valid header value"),
    );

    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        "DENY".parse().expect("valid header value"),
    );

    headers.insert(
        axum::http::header::REFERRER_POLICY,
        "strict-origin-when-cross-origin"
            .parse()
            .expect("valid header value"),
    );

    headers.insert(
        axum::http::header::STRICT_TRANSPORT_SECURITY,
        "max-age=31536000; includeSubDomains"
            .parse()
            .expect("valid header value"),
    );

    headers.insert(
        axum::http::HeaderName::from_static("permissions-policy"),
        "geolocation=(), microphone=(), camera=(), payment=()"
            .parse()
            .expect("valid header value"),
    );

    response
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
