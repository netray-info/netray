//! The outer layer stack of `netray serve`, copied from lens's `lib.rs` in its order (spec
//! v2-cutover, Phase 6), and the security-headers middleware the pages router also carries.

use axum::Router;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use netray_common::cors::cors_layer;
use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use crate::api::V1Error;

/// The outer layer stack, outermost first: request id, security headers, CORS, body limit,
/// compression, trace. `api_enabled` (`[api] enabled`) relaxes the headers of `/api/v1/docs` and
/// answers a 413 under `/api/v1` in the v1 error shape.
pub fn layers(router: Router, api_enabled: bool) -> Router {
    router
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    tracing::info_span!(
                        "http_request",
                        method = %request.method(),
                        uri = %request.uri(),
                        request_id = %request_id,
                        client_ip = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>,
                     latency: std::time::Duration,
                     span: &tracing::Span| {
                        tracing::info!(
                            parent: span,
                            status = response.status().as_u16(),
                            ms = latency.as_millis(),
                            "",
                        );
                    },
                ),
        )
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(8 * 1024))
        .layer(axum::middleware::from_fn_with_state(
            api_enabled,
            api_v1_payload_too_large,
        ))
        .layer(cors_layer())
        .layer(axum::middleware::from_fn_with_state(
            api_enabled,
            security_headers_mw,
        ))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
}

/// `/api/v1/` itself: the v1 404.
pub(crate) async fn api_v1_not_found() -> Response {
    V1Error::NotFound("no such endpoint".into()).into_response()
}

/// A 413 from the body limit under `/api/v1` answers in the v1 error shape, while v1 is mounted.
async fn api_v1_payload_too_large(
    State(api_enabled): State<bool>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let under_v1 = api_enabled && (path == "/api/v1" || path.starts_with("/api/v1/"));
    let response = next.run(request).await;
    if under_v1 && response.status() == StatusCode::PAYLOAD_TOO_LARGE {
        V1Error::PayloadTooLarge.into_response()
    } else {
        response
    }
}

/// The production security headers; with `api_enabled`, `/api/v1/docs` gets the relaxed CSP,
/// otherwise `/docs` does.
pub(crate) async fn security_headers_mw(
    State(api_enabled): State<bool>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let relaxed_prefix =
        if api_enabled && (path == "/api/v1/docs" || path.starts_with("/api/v1/docs/")) {
            "/api/v1/docs"
        } else {
            "/docs"
        };
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec!["https://cdn.jsdelivr.net".to_string()],
        relaxed_csp_path_prefix: relaxed_prefix.to_string(),
        keep_inner: true,
        ..Default::default()
    });
    layer_fn(request, next).await
}
