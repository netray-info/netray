use axum::Router;
use std::net::SocketAddr;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

mod config;
mod error;
mod input;
mod inspect;
mod quality;
mod routes;
mod security;
mod state;

pub use netray_common::middleware::RequestId;

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

#[tokio::main]
async fn main() {
    // 1. Load configuration.
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SPECTRA_CONFIG").ok());
    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    // 2. Initialize tracing.
    let telemetry_config: netray_common::telemetry::TelemetryConfig = (&config.telemetry).into();
    netray_common::telemetry::init_subscriber(
        &telemetry_config,
        "info,spectra=debug,hyper=warn,h2=warn",
    );

    tracing::info!(
        bind = %config.server.bind,
        per_ip_per_minute = config.limits.per_ip_per_minute,
        per_ip_burst = config.limits.per_ip_burst,
        per_target_per_minute = config.limits.per_target_per_minute,
        per_target_burst = config.limits.per_target_burst,
        trusted_proxy_count = config.server.trusted_proxies.len(),
        "starting spectra"
    );

    let state = state::AppState::new(&config);

    if state.enrichment_client.is_some() {
        tracing::info!("IP enrichment enabled");
    }

    let app = Router::new()
        .merge(routes::health_router(state.clone()))
        .merge(routes::api_router(state))
        .route("/robots.txt", axum::routing::get(robots_txt))
        .fallback(netray_common::server::static_handler::<Assets>())
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("spectra", req, next)
        }))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(security::security_headers))
        .layer(security::cors_layer())
        .layer(CompressionLayer::new())
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<_>| {
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
        .layer(RequestBodyLimitLayer::new(4 * 1024))
        .layer(tower::limit::ConcurrencyLimitLayer::new(
            config.limits.max_concurrent_connections,
        ));

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

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

    let listener = tokio::net::TcpListener::bind(config.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %config.server.bind, "spectra listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
    .await
    .expect("server error");

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
