pub mod admission;
pub mod api_doc;
pub mod badge;
pub mod cache;
pub mod check;
pub mod config;
pub mod error;
pub mod history;
pub mod input;
pub mod metrics;
pub mod modules;
pub mod og;
pub mod routes;
pub mod score;
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

    run_config(config, registry).await;
}

/// What [`serve`] needs besides the app: the counters and the run registry behind it.
pub struct Background {
    client_runs: std::sync::Arc<metrics::ClientRunCounter>,
    runs: std::sync::Arc<admission::Runs>,
}

/// Lens's routes with their `http_metrics` layer but without the outer layer stack: [`layers`]
/// wraps them, alone for `netray lens` or around the host router for `netray serve`.
pub struct Prepared {
    pub routes: Router,
    pub background: Background,
}

/// Serves lens's application on a loaded configuration. Like [`run_with`], it installs no
/// subscriber.
pub async fn run_config(config: config::Config, registry: netray_engine::Registry) {
    let Prepared { routes, background } = prepare(&config, registry).await;
    serve(&config, layers(routes), background).await;
}

/// Builds lens's state, its stores and its routes; binds nothing.
pub async fn prepare(config: &config::Config, registry: netray_engine::Registry) -> Prepared {
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

    if config.history.enabled {
        let db_path = &config.history.db_path;
        let store = match netray_store::HistoryStore::open(db_path).await {
            Ok(store) => store,
            Err(e) => {
                tracing::error!(error = %e, db_path = %db_path.display(), "failed to open history store");
                std::process::exit(1);
            }
        };
        if let Err(e) = store.migrate().await {
            tracing::error!(error = %e, db_path = %db_path.display(), "history store migration failed");
            std::process::exit(1);
        }
        let store = std::sync::Arc::new(store);
        state.history_store = Some(std::sync::Arc::clone(&store));
        tokio::spawn(history::run_sweep_loop(store));
        tracing::info!(db_path = %db_path.display(), "history enabled");
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

    let client_runs = std::sync::Arc::clone(&state.client_runs);
    let runs = std::sync::Arc::clone(&state.runs);

    // 4. Build the app with all routes and middleware.
    Prepared {
        routes: routes(state),
        background: Background { client_runs, runs },
    }
}

/// `/health` and `/ready` on lens's handlers, with no state: what answers before any host is
/// looked at.
pub fn probe_router() -> Router {
    Router::new()
        .route("/health", get(routes::health_handler))
        .route("/ready", get(routes::ready_handler))
}

/// Binds and serves `app` until a shutdown signal, runs the metrics listener, and drains the
/// runs in flight.
pub async fn serve(config: &config::Config, app: Router, background: Background) {
    let Background { client_runs, runs } = background;

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

    let drain_bound = crate::check::HARD_DEADLINE + std::time::Duration::from_secs(10);
    if tokio::time::timeout(drain_bound, runs.drain())
        .await
        .is_err()
    {
        tracing::warn!(
            bound_s = drain_bound.as_secs(),
            "shutdown: runs still in flight at the drain bound"
        );
    }

    // Flush pending OTel spans on shutdown.
    netray_common::telemetry::shutdown();
}

fn frontend_router_with(shell: spa::Shell) -> Router {
    Router::new()
        .route("/robots.txt", get(robots_txt))
        .fallback(spa::handler)
        .with_state(shell)
        .merge(netray_web::pages::router())
        .layer(axum::middleware::from_fn(security_headers_mw))
}

/// The SPA shell, `/robots.txt` and the shared pages with their security headers.
pub fn frontend_router() -> Router {
    frontend_router_with(spa::Shell(
        spa::render_shell(&config::SiteConfig::default()),
    ))
}

/// The whole HTTP app: every router, the SPA fallback, the OpenAPI routes and the layer stack.
pub fn app(state: state::AppState) -> Router {
    layers(routes(state))
}

/// Every router, the SPA fallback and the OpenAPI routes, with the `http_metrics` layer, which
/// needs the `MatchedPath` only these routes set.
pub fn routes(state: state::AppState) -> Router {
    let config = state.config.clone();
    let shell = spa::Shell(state.rendered_html.clone());

    // 4. Build routers and collect OpenAPI spec fragments.
    let (health_router, health_openapi) = routes::health_router().split_for_parts();
    let (api_router, api_openapi) = routes::api_router().split_for_parts();
    let (badge_routes, badge_openapi) = routes::badge_router().split_for_parts();
    let og_routes = routes::og_router();
    let openapi = api_doc::build_openapi(health_openapi, api_openapi, badge_openapi);

    let entry_routes = match &state.history_store {
        Some(history) if config.history.enabled => {
            netray_web::entries::router(netray_web::entries::EntriesDeps {
                history: history.clone(),
                v1: (config.snapshots.enabled && state.snapshot_store.is_some())
                    .then(|| routes::v1_lookup(state.clone())),
                card: og::handler::entry_card_renderer(&state),
            })
        }
        _ if config.snapshots.enabled => routes::snapshot_router().with_state(state.clone()),
        _ => Router::new(),
    };

    // 5. Build the main app with all middleware.
    Router::new()
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
        .with_state(state)
        .merge(frontend_router_with(shell))
        .merge(entry_routes)
        .merge(netray_web::entries::privacy_router())
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
}

/// The outer layer stack, outermost first: request id, security headers, CORS, body limit,
/// compression, trace.
pub fn layers(router: Router) -> Router {
    router
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
        ))
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
        keep_inner: true,
        ..Default::default()
    });
    layer_fn(request, next).await
}
