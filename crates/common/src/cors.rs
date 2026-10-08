//! Shared CORS layer construction.

use tower_http::cors::{Any, CorsLayer};

/// Returns a public-API `CorsLayer`: any origin, `GET`/`POST`/`OPTIONS`,
/// `content-type` and `accept` request headers, preflight cached for 600 s.
pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE, axum::http::header::ACCEPT])
        .max_age(std::time::Duration::from_secs(600))
}
