pub mod ip_extract;
pub mod rate_limit;

pub use ip_extract::IpExtractor;
pub use rate_limit::RateLimitState;

use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};

pub async fn security_headers(request: Request, next: Next) -> Response {
    // Scoped to the exact Scalar API Reference package path served in
    // src/scalar_docs.html. The SRI integrity attribute on that <script>
    // pins the bundle hash; keep this prefix in sync with the pinned version.
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec![
            "https://cdn.jsdelivr.net/npm/@scalar/api-reference@1.44.25/".to_string(),
        ],
        relaxed_csp_path_prefix: "/docs".to_string(),
        include_permissions_policy: true,
    });
    let mut response = layer_fn(request, next).await;
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    response
}
