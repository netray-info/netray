//! Security middleware composition for the prism web service.
//!
//! Implements the four-layer defense-in-depth model from SDD SS8:
//!
//! - **Layer 1**: Query restrictions (hardcoded) — via [`QueryPolicy`]
//! - **Layer 2**: Rate limiting (governor GCRA) — via [`RateLimitState`]
//! - **Layer 3**: Client IP extraction — via [`IpExtractor`]
//! - **Layer 4**: HTTP security headers — via [`security_headers_layer`]

pub mod ip_extract;
pub mod query_policy;
pub mod rate_limit;

pub use ip_extract::IpExtractor;
pub use query_policy::QueryPolicy;
pub use rate_limit::RateLimitState;

pub use netray_common::cors::cors_layer;

/// Build the security headers middleware configured for prism.
///
/// The API docs page loads Scalar from a CDN, so /docs paths get a relaxed
/// CSP with `https://cdn.jsdelivr.net` added to `script-src`.
pub fn security_headers_layer() -> impl Fn(
    axum::extract::Request,
    axum::middleware::Next,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = axum::response::Response> + Send>,
> + Clone
+ Send
+ 'static {
    netray_common::security_headers::security_headers_layer(
        netray_common::security_headers::SecurityHeadersConfig {
            extra_script_src: vec!["https://cdn.jsdelivr.net".into()],
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request as HttpRequest, StatusCode};
    use axum::middleware;
    use axum::response::Response;
    use axum::routing::get;
    use tower::ServiceExt;

    async fn ok_handler() -> &'static str {
        "ok"
    }

    async fn make_response(path: &str) -> Response {
        let layer_fn = security_headers_layer();
        let app = Router::new()
            .route("/test", get(ok_handler))
            .route("/docs/test", get(ok_handler))
            .layer(middleware::from_fn(move |req, next| {
                let f = layer_fn.clone();
                async move { f(req, next).await }
            }));

        let request = HttpRequest::builder()
            .uri(path)
            .body(Body::empty())
            .unwrap();

        app.oneshot(request).await.unwrap()
    }

    #[tokio::test]
    async fn sets_content_security_policy() {
        let response = make_response("/test").await;
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
    async fn strict_csp_excludes_cdn_on_non_docs() {
        let response = make_response("/test").await;
        let csp = response
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(!csp.contains("cdn.jsdelivr.net"));
    }

    #[tokio::test]
    async fn relaxed_csp_on_docs_path() {
        let response = make_response("/docs/test").await;
        let csp = response
            .headers()
            .get("content-security-policy")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(csp.contains("https://cdn.jsdelivr.net"));
    }

    #[tokio::test]
    async fn sets_x_content_type_options() {
        let response = make_response("/test").await;
        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn sets_x_frame_options() {
        let response = make_response("/test").await;
        assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    }

    #[tokio::test]
    async fn sets_referrer_policy() {
        let response = make_response("/test").await;
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
    }

    #[tokio::test]
    async fn sets_strict_transport_security() {
        let response = make_response("/test").await;
        assert_eq!(
            response.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[tokio::test]
    async fn handler_still_returns_ok() {
        let response = make_response("/test").await;
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[test]
    fn cors_layer_builds_without_panic() {
        let _ = cors_layer();
    }
}
