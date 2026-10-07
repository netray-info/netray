use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

mod api;
mod circuit_breaker;
mod config;
mod dns_dnssec;
mod dns_raw;
mod dns_trace;
mod error;
mod parser;
mod record_format;
mod reload;
mod result_cache;
mod security;

pub use netray_common::middleware::RequestId;
pub use netray_common::middleware::request_id as request_id_middleware;

// ---------------------------------------------------------------------------
// Embedded frontend assets
// ---------------------------------------------------------------------------

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    // 1. Load configuration (before tracing, since telemetry config controls subscriber setup).
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("PRISM_CONFIG").ok());
    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    // 2. Initialize tracing (with optional OpenTelemetry layer).
    netray_common::telemetry::init_subscriber(
        &config.telemetry,
        "info,prism=debug,hyper=warn,h2=warn",
    );

    tracing::info!(bind = %config.server.bind, "starting prism");
    tracing::info!(
        per_ip_per_minute = config.limits.per_ip_per_minute,
        per_ip_burst = config.limits.per_ip_burst,
        per_target_per_minute = config.limits.per_target_per_minute,
        per_target_burst = config.limits.per_target_burst,
        global_per_minute = config.limits.global_per_minute,
        global_burst = config.limits.global_burst,
        max_concurrent = config.limits.max_concurrent_connections,
        "rate limits configured"
    );
    tracing::info!(
        trusted_proxy_count = config.server.trusted_proxies.len(),
        "trusted proxies configured"
    );
    tracing::info!(
        window_secs = config.circuit_breaker.window_secs,
        cooldown_secs = config.circuit_breaker.cooldown_secs,
        failure_threshold = config.circuit_breaker.failure_threshold,
        min_requests = config.circuit_breaker.min_requests,
        "circuit breaker configured"
    );

    // 3. Build shared application state.
    let hot_state = reload::HotState::new(&config);

    let ip_enrichment = config.backends.ip.as_ref().and_then(|ip_cfg| {
        ip_cfg.url.as_ref().map(|url| {
            let timeout_ms = ip_cfg.timeout_ms;
            tracing::info!(url = %url, timeout_ms, "IP enrichment enabled");
            Arc::new(netray_common::enrichment::EnrichmentClient::new(
                url,
                std::time::Duration::from_millis(timeout_ms),
                "prism",
                Some("prism"),
            ))
        })
    });

    let http_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent(concat!("prism/", env!("CARGO_PKG_VERSION")))
        .build()
        .expect("failed to build HTTP client");

    let state = api::AppState {
        circuit_breakers: Arc::new(circuit_breaker::CircuitBreakerRegistry::new(
            &config.circuit_breaker,
        )),
        ip_extractor: Arc::new(security::IpExtractor::new(&config.server.trusted_proxies)),
        result_cache: Arc::new(result_cache::ResultCache::new()),
        hot_state: hot_state.clone(),
        ip_enrichment,
        query_semaphore: Arc::new(tokio::sync::Semaphore::new(api::QUERY_SEMAPHORE_PERMITS)),
        http_client,
        config: Arc::new(config.clone()),
    };

    // Spawn SIGHUP-based hot config reload watcher.
    reload::spawn_reload_watcher(config_path.clone(), hot_state);

    // 4. Compose the application router.
    //
    // Layer order matters: outermost layers run first on requests (last on
    // responses). The concurrency limit is outermost so excess connections are
    // shed before any work. Security headers and CORS wrap everything. Tracing
    // and compression are innermost around the actual handlers.
    let security_headers_fn = security::security_headers_layer();
    let app = Router::new()
        .merge(api::health_router(state.clone()))
        .merge(api::api_router(state))
        // robots.txt — explicit route so crawlers get text/plain, not the SPA fallback
        .route("/robots.txt", axum::routing::get(robots_txt))
        .fallback(netray_common::server::static_handler::<Assets>())
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("prism", req, next)
        }))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(move |req, next| {
            let f = security_headers_fn.clone();
            async move { f(req, next).await }
        }))
        .layer(security::cors_layer())
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<axum::body::Body>| {
                    let request_id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("");
                    tracing::info_span!(
                        "http_request",
                        method = %req.method(),
                        uri = %req.uri(),
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
        .layer(RequestBodyLimitLayer::new(8 * 1024))
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            config.limits.max_concurrent_connections,
        ));

    // 5. Single shutdown channel shared by both servers.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // 6. Start the metrics server on a separate port.
    //
    // SECURITY: The metrics endpoint must not be reachable from the public
    // internet. In production, bind to a loopback or private interface
    // (e.g. 127.0.0.1:9090) and restrict access at the network/firewall level.
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

    // 7. Start the main server with graceful shutdown.
    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %config.server.bind, "prism listening");

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

/// Returns a future that resolves once the shutdown watch channel is set to `true`.
async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}
