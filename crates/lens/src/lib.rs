pub mod admission;
pub mod api_doc;
pub mod api_v1;
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

pub async fn run_with(
    config_arg: Option<String>,
    registry: netray_engine::Registry,
    parts: netray_web::api::ApiParts,
) {
    // 1. Load config (first arg or LENS_CONFIG env var).
    let config_path = config_arg.or_else(|| std::env::var("LENS_CONFIG").ok());

    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    run_config(config, registry, parts).await;
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
pub async fn run_config(
    config: config::Config,
    registry: netray_engine::Registry,
    parts: netray_web::api::ApiParts,
) {
    let handle = install_metrics();
    let Prepared { routes, background } = prepare(&config, registry, parts).await;
    serve(
        &config,
        layers(routes, config.api.enabled),
        background,
        handle,
    )
    .await;
}

/// Installs the global Prometheus recorder with lens's buckets and creates every series at
/// zero. Call it before [`prepare`], so the startup gauges land on it.
pub fn install_metrics() -> netray_common::server::PrometheusHandle {
    netray_common::server::install_metrics(
        crate::metrics::HISTOGRAM_BUCKETS,
        crate::metrics::init_zero_series,
    )
    .expect("failed to install the metrics recorder")
}

/// Builds lens's state, its stores and its routes, `/api/v1` calling `parts`; binds nothing.
pub async fn prepare(
    config: &config::Config,
    registry: netray_engine::Registry,
    parts: netray_web::api::ApiParts,
) -> Prepared {
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
    state.api_parts = parts;

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
        tokio::spawn(history::run_sweep_loop(std::sync::Arc::clone(&store)));
        history::set_gauges(&store).await;
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
pub async fn serve(
    config: &config::Config,
    app: Router,
    background: Background,
    metrics: netray_common::server::PrometheusHandle,
) {
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
        if let Err(e) =
            netray_common::server::serve_metrics_handle(metrics_addr, metrics_shutdown, metrics)
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
        .layer(axum::middleware::from_fn_with_state(
            false,
            security_headers_mw,
        ))
}

/// The SPA shell, `/robots.txt` and the shared pages with their security headers.
pub fn frontend_router() -> Router {
    frontend_router_with(spa::Shell(
        spa::render_shell(&config::SiteConfig::default()),
    ))
}

/// The whole HTTP app: every router, the SPA fallback, the OpenAPI routes and the layer stack.
pub fn app(state: state::AppState) -> Router {
    let api_enabled = state.config.api.enabled;
    layers(routes(state), api_enabled)
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
            let v1 = (config.snapshots.enabled && state.snapshot_store.is_some())
                .then(|| routes::v1_lookup(state.clone()));
            netray_web::entries::router(netray_web::entries::EntriesDeps {
                history: history.clone(),
                v1: v1.clone(),
                card: og::handler::entry_card_renderer(&state),
            })
            .merge(netray_web::pages::evidence_router(
                netray_web::pages::EvidenceDeps {
                    history: history.clone(),
                    v1,
                },
            ))
        }
        _ if config.snapshots.enabled => routes::snapshot_router().with_state(state.clone()),
        _ => Router::new(),
    };

    let result_routes = match &state.history_store {
        Some(history) if config.history.enabled => {
            let spa_shell = shell.clone();
            let fallback: std::sync::Arc<
                dyn Fn(
                        axum::extract::Request,
                    )
                        -> futures::future::BoxFuture<'static, axum::response::Response>
                    + Send
                    + Sync,
            > = std::sync::Arc::new(move |req: axum::extract::Request| {
                let spa_shell = spa_shell.clone();
                Box::pin(async move {
                    spa::handler(axum::extract::State(spa_shell), req.uri().clone()).await
                })
            });
            netray_web::pages::result::router(
                netray_web::pages::result::ResultState::new(
                    history.clone(),
                    std::sync::Arc::new(api_v1::LensRuns::new(state.clone())),
                    config.cache.ttl_seconds,
                )
                .with_ip_extractor(state.ip_extractor.clone())
                .with_fallback(fallback),
            )
            .layer(axum::middleware::map_response(
                |mut response: axum::response::Response| async move {
                    response
                        .headers_mut()
                        .remove("cross-origin-resource-policy");
                    response
                },
            ))
        }
        _ => Router::new(),
    };

    let limits = netray_web::api::AdmissionLimits {
        client_budget_per_hour: config.admission.client_budget_per_hour,
        domain_run_slots: config.admission.domain_run_slots,
        rate_limit_per_ip_per_minute: config.rate_limit.per_ip_per_minute,
        rate_limit_per_ip_burst: config.rate_limit.per_ip_burst,
        rate_limit_global_per_minute: config.rate_limit.global_per_minute,
        rate_limit_global_burst: config.rate_limit.global_burst,
    };
    let api_state = netray_web::api::ApiState::new(
        config.api.clone(),
        std::sync::Arc::new(api_v1::LensRuns::new(state.clone())),
    )
    .with_store(state.history_store.clone())
    .with_ip_extractor(state.ip_extractor.clone())
    .with_admission_limits(limits)
    .with_parts(state.api_parts.clone())
    .with_profile(state.scoring_profile.clone());

    let inspect_routes = netray_web::pages::inspect_router(
        netray_web::pages::InspectDeps::new(
            std::sync::Arc::new(netray_web::api::InspectJobs::new(api_state.clone())),
            config.inspect.clone(),
        )
        .with_ip_extractor(state.ip_extractor.clone())
        .with_history(state.history_store.clone())
        .with_dns_results(api_state.dns_results().clone()),
    );

    let api_routes = if config.api.enabled {
        Router::new()
            .nest("/api/v1", netray_web::api::router(api_state))
            .route("/api/v1/", axum::routing::any(api_v1_not_found))
    } else {
        Router::new()
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
        .merge(inspect_routes)
        .merge(frontend_router_with(shell))
        .merge(entry_routes)
        .merge(result_routes)
        .merge(netray_web::entries::privacy_router())
        .merge(api_routes)
        .route(
            "/api-docs/openapi.json",
            get({
                let spec = openapi.clone();
                move || async move { axum::Json(spec) }
            }),
        )
        .merge(Scalar::with_url("/docs", openapi))
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("netray", req, next)
        }))
}

/// The outer layer stack, outermost first: request id, security headers, CORS, body limit,
/// compression, trace. `api_enabled` (`[api] enabled`) relaxes the headers of `/api/v1/docs` and
/// answers a 413 under `/api/v1` in the v1 error shape.
pub fn layers(router: Router, api_enabled: bool) -> Router {
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
        .layer(axum::middleware::from_fn_with_state(
            api_enabled,
            api_v1_payload_too_large,
        ))
        .layer(cors_layer())
        .layer(axum::middleware::from_fn_with_state(
            api_enabled,
            security_headers_mw,
        ))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
}

async fn api_v1_not_found() -> axum::response::Response {
    use axum::response::IntoResponse;
    netray_web::api::V1Error::NotFound("no such endpoint".into()).into_response()
}

/// A 413 from the body limit under `/api/v1` answers in the v1 error shape, while v1 is mounted.
async fn api_v1_payload_too_large(
    axum::extract::State(api_enabled): axum::extract::State<bool>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = request.uri().path();
    let under_v1 = api_enabled && (path == "/api/v1" || path.starts_with("/api/v1/"));
    let response = next.run(request).await;
    if under_v1 && response.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
        netray_web::api::V1Error::PayloadTooLarge.into_response()
    } else {
        response
    }
}

async fn robots_txt() -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; charset=utf-8",
        )],
        netray_web::pages::robots_body(),
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
    axum::extract::State(api_enabled): axum::extract::State<bool>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path();
    let relaxed_prefix =
        if api_enabled && (path == "/api/v1/docs" || path.starts_with("/api/v1/docs/")) {
            "/api/v1/docs"
        } else {
            "/docs"
        };
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec!["https://cdn.jsdelivr.net".to_string()],
        relaxed_csp_path_prefix: relaxed_prefix.to_string(),
        keep_inner: true,
        ..Default::default()
    });
    layer_fn(request, next).await
}
