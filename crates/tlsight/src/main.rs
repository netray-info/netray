use axum::Router;
use axum::response::IntoResponse;
use std::net::SocketAddr;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

mod config;
mod dns;
mod enrichment;
mod error;
mod input;
mod quality;
mod reload;
mod routes;
mod security;
mod state;
mod telemetry;
mod tls;
mod validate;

pub use netray_common::middleware::RequestId;

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

#[tokio::main]
async fn main() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");

    // 1. Load configuration (before tracing, since telemetry config controls subscriber setup).
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("TLSIGHT_CONFIG").ok());
    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    // 2. Initialize tracing (with optional OpenTelemetry layer).
    telemetry::init_subscriber(&config.telemetry);

    tracing::info!(bind = %config.server.bind, "starting tlsight");

    let mut state = state::AppState::new(&config);

    if config.validation.check_caa || config.validation.check_dane {
        match dns::DnsResolver::new(config.dns.timeout_secs).await {
            Ok(resolver) => {
                state.dns_resolver = Some(std::sync::Arc::new(resolver));
                tracing::info!("DNS resolver initialized for CAA/DANE lookups");
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to initialize DNS resolver, CAA/DANE checks disabled");
            }
        }
    }

    if state.enrichment_client.is_some() {
        tracing::info!("IP enrichment enabled");
    }

    reload::spawn_reload_watcher(config_path.clone(), state.config.clone());

    let app = Router::new()
        .merge(routes::health_router())
        .merge(routes::api_router(state))
        .fallback(static_handler)
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("tlsight", req, next)
        }))
        .layer(axum::middleware::from_fn(netray_common::middleware::request_id))
        .layer(axum::middleware::from_fn(security::security_headers))
        .layer(security::cors_layer())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(4 * 1024))
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            config.limits.max_concurrent_connections,
        ));

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

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

    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %config.server.bind, "tlsight listening");

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

async fn static_handler(uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    match Assets::get(if path.is_empty() { "index.html" } else { path }) {
        Some(file) => {
            let effective_path = if path.is_empty() { "index.html" } else { path };
            let mime = mime_guess::from_path(effective_path).first_or_octet_stream();
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
        None => match Assets::get("index.html") {
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
        },
    }
}

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

async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

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
