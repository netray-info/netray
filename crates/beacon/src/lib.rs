//! Beacon — DNS-only email-security inspector (fifth pillar of the netray
//! suite: IP -> DNS -> TLS -> HTTP -> Email).
//!
//! Given a domain (e.g. `example.com`), beacon evaluates twelve email
//! security categories (MX, SPF, DKIM, DMARC, MTA-STS, TLS-RPT, DANE, DNSSEC,
//! BIMI, FCrDNS, DNSBL, cross-validation) and produces an aggregate A–F
//! grade. Results stream to the client over Server-Sent Events as they
//! complete.
//!
//! The crate is split into self-contained modules: [`checks`] implements the
//! per-category probes and the three-phase orchestration; [`config`] loads
//! TOML plus `BEACON_*` environment overrides; [`dns`] wraps `mhost` into a
//! shared round-robin resolver; [`quality`] defines the `Verdict` / `Grade`
//! /`CheckResult` model and grade computation; [`input`] validates domains
//! and DKIM selectors; [`routes`] wires the Axum handlers and the utoipa
//! OpenAPI document; [`security`] holds IP extraction, rate limiting, and
//! security-header middleware; [`state`] builds the shared [`state::AppState`].
//!
//! See `CLAUDE.md` and `specs/done/sdd/beacon.md` for design rationale.

pub mod checks;
pub mod config;
pub mod dns;
pub mod error;
pub mod input;
pub mod quality;
pub mod routes;
pub mod security;
pub mod state;

pub use netray_common::middleware::RequestId;

use std::net::SocketAddr;

use axum::Router;
use axum::routing::get;
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;
use tracing::Span;

use state::AppState;

#[derive(rust_embed::RustEmbed)]
#[folder = "frontend/dist"]
struct Assets;

pub async fn run(config_arg: Option<String>) -> anyhow::Result<()> {
    // Load config
    let (config_path, config_source) =
        config::resolve_path(config_arg, std::env::var("BEACON_CONFIG").ok());

    let config = config::Config::load(Some(&config_path)).expect("failed to load config");

    // Init telemetry
    let telemetry_config = netray_common::telemetry::TelemetryConfig::from(&config.telemetry);
    netray_common::telemetry::init_subscriber(
        &telemetry_config,
        "info,beacon=debug,hyper=warn,h2=warn",
    );

    metrics::describe_gauge!("beacon_sse_clients_active", "Active SSE inspection streams");

    tracing::info!(
        config_path = %config_path,
        config_source = config_source.as_str(),
        bind = %config.server.bind,
        metrics_bind = %config.server.metrics_bind,
        dns_resolvers = ?config.dns.resolvers,
        dns_timeout_ms = config.dns.timeout_ms,
        dnsbl_resolvers = ?config.dnsbl.resolvers,
        dnsbl_zones = ?config.dnsbl.zones,
        dnsbl_timeout_ms = config.dnsbl.timeout_ms,
        per_ip_rate = %config.rate_limit.per_ip,
        max_concurrent_inspections = config.server.max_concurrent_inspections,
        trusted_proxy_count = config.server.trusted_proxies.len(),
        ip_backend_url = if config.backends.ip_url.is_empty() {
            "disabled"
        } else {
            config.backends.ip_url.as_str()
        },
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
        .layer(RequestBodyLimitLayer::new(64 * 1024))
        .layer(security::cors_layer())
        .layer(axum::middleware::from_fn(security::security_headers));

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
        if let Err(e) = netray_common::server::serve_metrics(metrics_addr, metrics_shutdown).await {
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
    let addr: SocketAddr = config.server.bind.parse().expect("invalid bind address");
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

async fn robots_txt() -> (
    [(axum::http::header::HeaderName, &'static str); 1],
    &'static str,
) {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        "User-agent: *\nAllow: /\n",
    )
}
