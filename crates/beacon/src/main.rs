use std::net::SocketAddr;

use axum::routing::get;
use axum::Router;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::Span;

mod checks;
mod config;
mod dns;
mod error;
mod input;
mod quality;
mod routes;
mod security;
mod state;

use state::AppState;

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load config
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("BEACON_CONFIG").ok());

    let config = config::Config::load(config_path.as_deref())
        .expect("failed to load config");

    // Init telemetry
    let telemetry_config = netray_common::telemetry::TelemetryConfig::from(&config.telemetry);
    netray_common::telemetry::init_subscriber(
        &telemetry_config,
        "info,beacon=debug,hyper=warn,h2=warn",
    );

    tracing::info!(
        bind = %config.server.bind,
        metrics_bind = %config.server.metrics_bind,
        "starting beacon"
    );

    // Build state
    let state = AppState::new(&config).await?;

    // Build router
    let app = Router::new()
        .merge(routes::health_router(state.clone()))
        .merge(routes::api_router(state))
        .route("/robots.txt", get(robots_txt))
        .fallback(netray_common::server::static_handler::<Assets>())
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("beacon", req, next)
        }))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(security::security_headers))
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<_>| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
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
                     span: &Span| {
                        tracing::info!(
                            parent: span,
                            status = response.status().as_u16(),
                            ms = latency.as_millis(),
                            ""
                        );
                    },
                ),
        )
        .layer(RequestBodyLimitLayer::new(64 * 1024));

    // Graceful shutdown
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Metrics server
    let metrics_addr: SocketAddr = config
        .server
        .metrics_bind
        .parse()
        .expect("invalid metrics_bind address");
    let metrics_shutdown = shutdown_rx.clone();
    tokio::spawn(async move {
        if let Err(e) =
            netray_common::server::serve_metrics(metrics_addr, metrics_shutdown).await
        {
            tracing::error!(error = %e, "metrics server failed");
        }
    });

    // Signal handler
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        tracing::info!("shutdown signal received");
        let _ = shutdown_tx.send(true);
    });

    // Main server
    let addr: SocketAddr = config
        .server
        .bind
        .parse()
        .expect("invalid bind address");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");

    let mut rx = shutdown_rx.clone();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        rx.changed().await.ok();
    })
    .await?;

    netray_common::telemetry::shutdown();
    tracing::info!("shutdown complete");

    Ok(())
}

async fn robots_txt() -> ([(axum::http::header::HeaderName, &'static str); 1], &'static str) {
    (
        [(axum::http::header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "User-agent: *\nAllow: /\n",
    )
}
