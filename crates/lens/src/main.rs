use lens::config;
use lens::routes;
use lens::state;

use std::net::SocketAddr;

use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use axum::Router;
use axum::routing::get;

pub use netray_common::cors::cors_layer;
use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};

#[tokio::main]
async fn main() {
    // 1. Load config (first arg or LENS_CONFIG env var).
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("LENS_CONFIG").ok());

    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    // 2. Init tracing (with optional OpenTelemetry layer).
    netray_common::telemetry::init_subscriber(
        &config.telemetry,
        "info,lens=debug,hyper=warn,h2=warn",
    );

    tracing::info!(
        bind = %config.server.bind,
        dns_url = %config.backends.dns_url,
        tls_url = %config.backends.tls_url,
        ip_url  = %config.backends.ip_url,
        per_ip_rate = config.rate_limit.per_ip_per_minute,
        per_ip_burst = config.rate_limit.per_ip_burst,
        global_rate = config.rate_limit.global_per_minute,
        global_burst = config.rate_limit.global_burst,
        trusted_proxy_count = config.server.trusted_proxies.len(),
        cache_enabled = config.cache.enabled,
        cache_ttl_seconds = config.cache.ttl_seconds,
        "starting lens"
    );

    // 3. Build app state.
    let state = state::AppState::new(config.clone()).expect("failed to build app state");

    // 4. Build router.
    let app = Router::new()
        .merge(routes::health_router(state.clone()))
        .merge(routes::api_router(state))
        .route("/robots.txt", get(robots_txt))
        .fallback(netray_common::server::static_handler::<routes::Assets>())
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("lens", req, next)
        }))
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
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(security_headers_mw))
        .layer(cors_layer())
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(8 * 1024));

    // 5. Graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // 6. Metrics server.
    let metrics_addr = config.server.metrics_bind;
    let metrics_shutdown = shutdown_rx.clone();
    tracing::info!(
        addr = %metrics_addr,
        "metrics server starting — ensure this address is NOT publicly reachable"
    );
    tokio::spawn(async move {
        if let Err(e) = netray_common::server::serve_metrics(metrics_addr, metrics_shutdown).await {
            tracing::error!(error = %e, "metrics server failed");
        }
    });

    // 7. Bind and serve.
    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %config.server.bind, "lens listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
    .await
    .expect("server error");

    // Flush pending OTel spans on shutdown.
    netray_common::telemetry::shutdown();
}

async fn robots_txt() -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        "User-agent: *\nAllow: /\n",
    )
}

async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

async fn security_headers_mw(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec![],
        relaxed_csp_path_prefix: "/docs".to_string(),
        include_permissions_policy: true,
    });
    layer_fn(request, next).await
}
