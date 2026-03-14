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
    netray_common::telemetry::init_subscriber(&config.telemetry, "prism=info,tower_http=info");

    tracing::info!(bind = %config.server.bind, "starting prism");

    // 3. Build shared application state.
    let hot_state = reload::HotState::new(&config);

    let ip_enrichment = config.ecosystem.effective_api_url().map(|url| {
        let timeout = std::time::Duration::from_millis(config.ecosystem.enrichment_timeout_ms);
        tracing::info!(url = %url, timeout_ms = config.ecosystem.enrichment_timeout_ms, "IP enrichment enabled");
        Arc::new(netray_common::enrichment::EnrichmentClient::new(url, timeout, "prism", Some("prism")))
    });

    let state = api::AppState {
        circuit_breakers: Arc::new(circuit_breaker::CircuitBreakerRegistry::new(
            &config.circuit_breaker,
        )),
        ip_extractor: Arc::new(
            security::IpExtractor::new(&config.server.trusted_proxies),
        ),
        result_cache: Arc::new(result_cache::ResultCache::new()),
        hot_state: hot_state.clone(),
        ip_enrichment,
        query_semaphore: Arc::new(tokio::sync::Semaphore::new(
            api::QUERY_SEMAPHORE_PERMITS,
        )),
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
        .merge(api::health_router())
        .merge(api::api_router(state))
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
        .layer(TraceLayer::new_for_http())
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

/// Returns a future that resolves once the shutdown watch channel is set to `true`.
async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}
