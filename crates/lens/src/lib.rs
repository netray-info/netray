pub mod api_doc;
pub mod badge;
pub mod cache;
pub mod check;
pub mod config;
pub mod error;
pub mod input;
pub mod metrics;
pub mod modules;
pub mod og;
pub mod routes;
pub mod scoring;
pub mod security;
pub mod snapshot;
pub mod spa;
pub mod state;

use snapshot::{SnapshotStore, run_sweep_loop};

use std::net::SocketAddr;

use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use axum::Router;
use axum::routing::get;
use utoipa_scalar::{Scalar, Servable};

use netray_common::cors::cors_layer;
use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};

/// Installs the tracing subscriber (with the optional OpenTelemetry layer). The caller installs
/// it before building the registry, so the module load warnings are logged, and before
/// [`run_with`], which does not install one.
pub fn init_telemetry(config: &config::Config) {
    netray_common::telemetry::init_subscriber(
        &config.telemetry,
        "info,lens=debug,hyper=warn,h2=warn",
    );
}

pub async fn run_with(config_arg: Option<String>, registry: netray_engine::Registry) {
    // 1. Load config (first arg or LENS_CONFIG env var).
    let config_path = config_arg.or_else(|| std::env::var("LENS_CONFIG").ok());

    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    tracing::info!(
        bind = %config.server.bind,
        http_module = config.backends.http.is_some(),
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
    let mut state = state::AppState::with_registry(config.clone(), registry)
        .expect("failed to build app state");

    // 3a. Init snapshot store if enabled.
    if config.snapshots.enabled {
        match SnapshotStore::new(&config.snapshots.db_path).await {
            Ok(store) => {
                if let Err(e) = store.migrate().await {
                    tracing::error!(error = %e, "snapshot migration failed");
                    std::process::exit(1);
                }
                let arc_store = std::sync::Arc::new(store);
                state.snapshot_store = Some(std::sync::Arc::clone(&arc_store));
                tokio::spawn(run_sweep_loop(arc_store));
                tracing::info!(
                    db_path = %config.snapshots.db_path.display(),
                    "snapshots enabled"
                );
            }
            Err(e) => {
                tracing::error!(error = %e, "failed to open snapshot store");
                std::process::exit(1);
            }
        }
    }

    if config.badges.enabled {
        tracing::info!(
            ttl_seconds = config.badges.ttl_seconds,
            default_label = %config.badges.default_label,
            "badges enabled"
        );
    }

    if config.og_cards.enabled {
        tracing::info!("og cards enabled");
    }

    // 4. Build routers and collect OpenAPI spec fragments.
    let (health_router, health_openapi) = routes::health_router().split_for_parts();
    let (api_router, api_openapi) = routes::api_router().split_for_parts();
    let (badge_routes, badge_openapi) = routes::badge_router().split_for_parts();
    let og_routes = routes::og_router();
    let snapshot_routes = routes::snapshot_router();
    let openapi = api_doc::build_openapi(health_openapi, api_openapi, badge_openapi);

    let client_runs = std::sync::Arc::clone(&state.client_runs);

    // 5. Build the main app with all middleware.
    let app = Router::new()
        .merge(health_router.with_state(state.clone()))
        .merge(api_router.with_state(state.clone()))
        .merge(if config.badges.enabled {
            badge_routes.with_state(state.clone())
        } else {
            Router::new()
        })
        .merge(if config.og_cards.enabled {
            og_routes.with_state(state.clone())
        } else {
            Router::new()
        })
        .merge(if config.snapshots.enabled {
            snapshot_routes.with_state(state.clone())
        } else {
            Router::new()
        })
        .route("/robots.txt", get(robots_txt))
        .fallback(spa::handler)
        .with_state(state)
        .route(
            "/api-docs/openapi.json",
            get({
                let spec = openapi.clone();
                move || async move { axum::Json(spec) }
            }),
        )
        .merge(Scalar::with_url("/docs", openapi))
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
        .layer(CompressionLayer::new())
        .layer(RequestBodyLimitLayer::new(8 * 1024))
        .layer(cors_layer())
        .layer(axum::middleware::from_fn(security_headers_mw))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ));

    // 6. Graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    let flush_shutdown = shutdown_rx.clone();
    tokio::spawn(run_client_runs_flush(client_runs, flush_shutdown));

    // 7. Metrics server.
    let metrics_addr = config.server.metrics_bind;
    let metrics_shutdown = shutdown_rx.clone();
    tracing::info!(
        addr = %metrics_addr,
        "metrics server starting — ensure this address is NOT publicly reachable"
    );
    tokio::spawn(async move {
        if let Err(e) = netray_common::server::serve_metrics_with(
            metrics_addr,
            metrics_shutdown,
            crate::metrics::HISTOGRAM_BUCKETS,
            crate::metrics::init_zero_series,
        )
        .await
        {
            tracing::error!(error = %e, "metrics server failed");
        }
    });

    // 8. Bind and serve.
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

async fn run_client_runs_flush(
    counter: std::sync::Arc<metrics::ClientRunCounter>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(3600));
    ticker.tick().await;
    loop {
        tokio::select! {
            _ = ticker.tick() => counter.flush(),
            _ = shutdown.wait_for(|v| *v) => break,
        }
    }
}

async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
    let _ = rx.wait_for(|v| *v).await;
}

async fn security_headers_mw(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec!["https://cdn.jsdelivr.net".to_string()],
        relaxed_csp_path_prefix: "/docs".to_string(),
        ..Default::default()
    });
    layer_fn(request, next).await
}
