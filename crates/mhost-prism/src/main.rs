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
mod error;
mod parser;
mod security;

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
    // 1. Initialize tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "prism=info,tower_http=info".into()),
        )
        .init();

    // 2. Load configuration.
    let config_path = std::env::args().nth(1);
    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    tracing::info!(bind = %config.server.bind, "starting prism");

    // 3. Build shared application state.
    let state = api::AppState {
        config: Arc::new(config.clone()),
        circuit_breakers: Arc::new(circuit_breaker::CircuitBreakerRegistry::new()),
        ip_extractor: Arc::new(security::IpExtractor::new(&config.server.trusted_proxies)),
    };

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
        .layer(axum::middleware::from_fn(request_id_middleware))
        .layer(axum::middleware::from_fn(security::security_headers))
        .layer(security::cors_layer())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(8 * 1024))
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            config.limits.max_concurrent_connections,
        ));

    // 5. Start the metrics server on a separate port.
    let metrics_addr = config.server.metrics_bind;
    tokio::spawn(async move {
        if let Err(e) = serve_metrics(metrics_addr).await {
            tracing::error!(error = %e, "metrics server failed");
        }
    });

    // 6. Start the main server with graceful shutdown.
    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %config.server.bind, "prism listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .expect("server error");
}

// ---------------------------------------------------------------------------
// Request ID middleware
// ---------------------------------------------------------------------------

/// Injects a `X-Request-Id` header (UUID v7) into every response.
async fn request_id_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let request_id = uuid::Uuid::now_v7().to_string();
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        axum::http::HeaderName::from_static("x-request-id"),
        axum::http::HeaderValue::from_str(&request_id).expect("UUID is valid header value"),
    );
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

// ---------------------------------------------------------------------------
// Prometheus metrics server
// ---------------------------------------------------------------------------

async fn serve_metrics(addr: SocketAddr) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}
