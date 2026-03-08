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
// TODO: Wire query dedup into SSE handlers and resolver pool into build_resolver_group.
#[allow(dead_code)]
mod query_dedup;
mod record_format;
mod reload;
#[allow(dead_code)]
mod resolver_pool;
mod result_cache;
mod security;
mod telemetry;

// ---------------------------------------------------------------------------
// Request ID newtype
// ---------------------------------------------------------------------------

/// Wraps the per-request UUID v7 so it can be stored in request extensions
/// and extracted by SSE handlers to correlate the `X-Request-Id` response
/// header with SSE event payloads.
#[derive(Clone)]
pub struct RequestId(pub String);

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
    let resolver_pool = Arc::new(resolver_pool::ResolverPool::new(
        config.performance.resolver_pool_ttl_secs,
        config.performance.resolver_pool_max_size,
    ));
    resolver_pool.spawn_cleanup_task(config.performance.resolver_pool_cleanup_interval_secs);

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
        rate_limiter: Arc::new(security::RateLimitState::new(&config.limits)),
        result_cache: Arc::new(result_cache::ResultCache::new()),
        resolver_pool,
        query_dedup: query_dedup::QueryDedup::new(),
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
    let app = Router::new()
        .merge(api::health_router())
        .merge(api::api_router(state))
        .fallback(static_handler)
        .layer(axum::middleware::from_fn(http_metrics_middleware))
        .layer(axum::middleware::from_fn(request_id_middleware))
        .layer(axum::middleware::from_fn(security::security_headers))
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
// Request ID middleware
// ---------------------------------------------------------------------------

/// Injects a `X-Request-Id` header (UUID v7) into every response and stores
/// the same ID in request extensions so SSE handlers can correlate the header
/// with their `request_id` SSE field.
pub(crate) async fn request_id_middleware(
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let id = uuid::Uuid::now_v7().to_string();
    request.extensions_mut().insert(RequestId(id.clone()));
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("x-request-id"),
        axum::http::HeaderValue::from_str(&id)
            .unwrap_or_else(|_| axum::http::HeaderValue::from_static("invalid")),
    );
    response
}

// ---------------------------------------------------------------------------
// HTTP metrics middleware
// ---------------------------------------------------------------------------

/// Increments `prism_http_requests_total{method, path, status}` for every
/// HTTP request, enabling error-rate SLO calculations.
async fn http_metrics_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let method = request.method().to_string();
    let path = request.uri().path().to_owned();
    let response = next.run(request).await;
    let status = response.status().as_u16().to_string();
    metrics::counter!(
        "prism_http_requests_total",
        "method" => method,
        "path" => path,
        "status" => status,
    )
    .increment(1);
    response
}

// ---------------------------------------------------------------------------
// Static file serving (rust-embed)
// ---------------------------------------------------------------------------

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    match Assets::get(if path.is_empty() { "index.html" } else { path }) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
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
