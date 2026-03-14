use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::response::IntoResponse;
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
mod ip_enrichment;
mod parser;
mod record_format;
mod reload;
mod result_cache;
mod security;
mod telemetry;

pub use netray_common::middleware::RequestId;

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
    telemetry::init_subscriber(&config.telemetry);

    tracing::info!(bind = %config.server.bind, "starting prism");

    // 3. Build shared application state.
    let hot_state = reload::HotState::new(&config);

    let ip_enrichment = config.ecosystem.effective_api_url().map(|url| {
        let timeout = std::time::Duration::from_millis(config.ecosystem.enrichment_timeout_ms);
        tracing::info!(url = %url, timeout_ms = config.ecosystem.enrichment_timeout_ms, "IP enrichment enabled");
        Arc::new(ip_enrichment::IpEnrichmentService::new(url, timeout))
    });

    let state = api::AppState {
        circuit_breakers: Arc::new(circuit_breaker::CircuitBreakerRegistry::new(
            &config.circuit_breaker,
        )),
        ip_extractor: Arc::new(
            security::IpExtractor::new(&config.server.trusted_proxies)
                .expect("invalid trusted_proxies configuration"),
        ),
        result_cache: Arc::new(result_cache::ResultCache::new()),
        hot_state: hot_state.clone(),
        ip_enrichment,
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
        .fallback(static_handler)
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("prism", req, next)
        }))
        .layer(axum::middleware::from_fn(netray_common::middleware::request_id))
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
        shutdown_signal().await;
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
        if let Err(e) = serve_metrics(metrics_addr, metrics_shutdown).await {
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
    telemetry::shutdown();
}

// ---------------------------------------------------------------------------
// Static file serving (rust-embed)
// ---------------------------------------------------------------------------

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    match Assets::get(if path.is_empty() { "index.html" } else { path }) {
        Some(file) => {
            let mime = mime_guess::from_path(if path.is_empty() { "index.html" } else { path })
                .first_or_octet_stream();
            // index.html: no-cache so the SPA picks up new deployments.
            // Hashed assets (JS/CSS bundles from Vite): immutable.
            let cache = if path.is_empty() || path == "index.html" {
                "no-cache"
            } else {
                "public, max-age=31536000, immutable"
            };
            (
                [
                    (axum::http::header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (axum::http::header::CACHE_CONTROL, cache.to_string()),
                ],
                file.data.to_vec(),
            )
                .into_response()
        }
        None => {
            // SPA fallback: serve index.html for unrecognized paths so
            // client-side routing works.
            match Assets::get("index.html") {
                Some(index) => (
                    [(axum::http::header::CONTENT_TYPE, "text/html".to_string())],
                    index.data.to_vec(),
                )
                    .into_response(),
                None => (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "frontend not found",
                )
                    .into_response(),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Graceful shutdown
// ---------------------------------------------------------------------------

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }

    tracing::info!("shutdown signal received");
}

/// Returns a future that resolves once the shutdown watch channel is set to `true`.
async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

// ---------------------------------------------------------------------------
// Prometheus metrics server
// ---------------------------------------------------------------------------

async fn serve_metrics(
    addr: SocketAddr,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
    let handle = builder.install_recorder()?;

    let app = Router::new().route(
        "/metrics",
        axum::routing::get(move || {
            let handle = handle.clone();
            async move { handle.render() }
        }),
    );

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "metrics server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(wait_for_shutdown(shutdown))
        .await?;

    Ok(())
}
