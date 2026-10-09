pub mod backend;
#[cfg(test)]
mod classify_tests;
pub mod config;
#[cfg(test)]
mod contract_golden;
pub mod enrichment;
pub mod error;
pub mod extractors;
pub mod format;
pub mod handlers;
pub mod middleware;
pub mod negotiate;
#[cfg(test)]
mod rate_limit_exempt_tests;
pub mod routes;
pub mod state;

use arc_swap::ArcSwap;
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::middleware as axum_mw;
use axum::response::IntoResponse;
use axum::routing::get;
use enrichment::EnrichmentContext;
use state::AppState;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::net::TcpListener;
use tower_http::compression::CompressionLayer;
use tower_http::cors::AllowOrigin;
use tower_http::trace::TraceLayer;
use tracing::{info, warn};

pub use config::Config;
pub use state::ProjectInfo;

/// Middleware that requires a valid `Authorization: Bearer <token>` header.
/// Used to protect the admin port when `server.admin_token` is configured.
async fn admin_bearer_auth(
    axum::extract::State(expected): axum::extract::State<String>,
    req: axum::http::Request<axum::body::Body>,
    next: axum_mw::Next,
) -> axum::response::Response {
    let ok = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|t| bool::from(t.as_bytes().ct_eq(expected.as_bytes())))
        .unwrap_or(false);
    if ok {
        next.run(req).await
    } else {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            [(
                axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"admin\""),
            )],
        )
            .into_response()
    }
}

pub struct AppBundle {
    pub app: Router,
    pub admin_app: Option<Router>,
    /// Handle to the shared `ArcSwap<EnrichmentContext>` for SIGHUP-based hot-reload.
    pub enrichment_handle: Arc<ArcSwap<EnrichmentContext>>,
}

pub async fn build_app(config: &Config) -> AppBundle {
    let state = AppState::new(config).await;
    let enrichment_handle = Arc::clone(&state.enrichment);

    // Try to install metrics recorder. May fail in tests where multiple
    // build_app calls run in the same process — that's fine, skip metrics.
    let metrics_handle = match metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder() {
        Ok(handle) => Some(handle),
        Err(e) => {
            tracing::error!("Failed to install Prometheus metrics recorder: {e}");
            None
        }
    };

    if metrics_handle.is_some() {
        metrics_process::Collector::default().describe();
    }

    // Spawn periodic rate limiter cleanup to prevent unbounded DashMap growth
    {
        let limiter = Arc::clone(&state.rate_limiter);
        let target_limiter = Arc::clone(&state.target_rate_limiter);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            interval.tick().await; // skip the immediate first tick
            loop {
                interval.tick().await;
                let before = limiter.len();
                limiter.retain_recent();
                limiter.shrink_to_fit();
                let after = limiter.len();
                if before != after {
                    tracing::debug!("Rate limiter cleanup: {} -> {} entries", before, after);
                }
                let before = target_limiter.len();
                target_limiter.retain_recent();
                target_limiter.shrink_to_fit();
                let after = target_limiter.len();
                if before != after {
                    tracing::debug!("Target rate limiter cleanup: {} -> {} entries", before, after);
                }
            }
        });
    }

    let api_routes = routes::router();

    let cors = if config.server.cors_allowed_origins.iter().any(|o| o == "*") {
        netray_common::cors::cors_layer()
    } else {
        let origins: Vec<axum::http::HeaderValue> = config
            .server
            .cors_allowed_origins
            .iter()
            .filter_map(|o| o.parse().ok())
            .collect();
        netray_common::cors::cors_layer().allow_origin(AllowOrigin::list(origins))
    };

    let app = Router::new()
        .merge(api_routes)
        .fallback(netray_common::server::static_handler::<routes::Assets>())
        .layer(DefaultBodyLimit::max(1_048_576))
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::etag_last_modified,
        ))
        .layer(axum_mw::from_fn(middleware::ifconfig_response_headers))
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            middleware::geoip_date_headers,
        ))
        .layer(axum_mw::from_fn_with_state(state.clone(), middleware::rate_limit))
        .layer(axum_mw::from_fn_with_state(
            state.clone(),
            extractors::requester_info_middleware,
        ))
        // Outside rate_limit and etag so 429 and 304 responses carry the headers too.
        .layer(cors)
        .layer(axum_mw::from_fn(
            netray_common::security_headers::security_headers_layer(
                netray_common::security_headers::SecurityHeadersConfig {
                    // Scalar API docs UI loads its renderer from jsDelivr.
                    extra_script_src: vec!["https://cdn.jsdelivr.net".to_string()],
                    hsts: "max-age=63072000; includeSubDomains; preload".to_string(),
                    ..Default::default()
                },
            ),
        ))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|req: &axum::http::Request<axum::body::Body>| {
                    let request_id = req
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    tracing::info_span!(
                        "http_request",
                        method = %req.method(),
                        uri = %req.uri(),
                        request_id = %request_id,
                        client_ip = tracing::field::Empty,
                    )
                })
                .on_response(
                    |response: &axum::http::Response<_>, latency: std::time::Duration, span: &tracing::Span| {
                        tracing::info!(
                            parent: span,
                            status = response.status().as_u16(),
                            ms = latency.as_millis(),
                            "",
                        );
                    },
                ),
        )
        .layer(axum_mw::from_fn(middleware::record_metrics))
        .layer(axum_mw::from_fn(middleware::request_id))
        .layer(CompressionLayer::new())
        .with_state(state.clone());

    let admin_app = config.server.admin_bind.as_ref().and_then(|_| {
        let handle = metrics_handle?;
        let admin_state = state.clone();
        let mut router = Router::new()
            .route(
                "/metrics",
                get(move || {
                    let h = handle.clone();
                    async move {
                        metrics_process::Collector::default().collect();
                        h.render().into_response()
                    }
                }),
            )
            .route("/health", get(|| async { axum::http::StatusCode::OK.into_response() }))
            .route("/ready", get(routes::ready_handler))
            .with_state(admin_state);
        if let Some(token) = config.server.admin_token.clone() {
            router = router.layer(axum_mw::from_fn_with_state(token, admin_bearer_auth));
        }
        Some(router)
    });

    AppBundle {
        app,
        admin_app,
        enrichment_handle,
    }
}

pub async fn run(config_path: Option<String>, print_config: bool, check: bool) {
    let config = Config::load(config_path.as_deref()).expect("Failed to load config");

    netray_common::telemetry::init_subscriber(
        &config.telemetry,
        "info,ifconfig_rs=debug,hyper=warn,h2=warn,mhost=warn",
    );

    if print_config {
        println!(
            "{}",
            toml::to_string_pretty(&config).expect("Failed to serialize config")
        );
        netray_common::telemetry::shutdown();
        return;
    }

    if check {
        let exit_code = run_check(&config).await;
        netray_common::telemetry::shutdown();
        std::process::exit(exit_code);
    }

    let bind_addr: SocketAddr = config.server.bind.parse().expect("Invalid bind address");
    info!("Starting server on {}", bind_addr);
    info!(
        enabled = config.batch.enabled,
        max_size = config.batch.max_size,
        "Batch endpoint"
    );

    let config = Arc::new(config);
    let bundle = build_app(&config).await;

    // Spawn SIGHUP handler for hot-reloading enrichment data
    #[cfg(unix)]
    {
        let enrichment_handle = Arc::clone(&bundle.enrichment_handle);
        let reload_config = Arc::clone(&config);
        tokio::spawn(async move {
            let mut sig = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
                .expect("Failed to register SIGHUP handler");
            loop {
                sig.recv().await;
                reload_enrichment(&enrichment_handle, &reload_config, "SIGHUP").await;
            }
        });
    }

    // Spawn filesystem watcher for auto-reload (opt-in)
    if config.watch_data_files {
        spawn_file_watcher(Arc::clone(&config), Arc::clone(&bundle.enrichment_handle));
    }

    // Spawn admin server if configured
    if let Some(admin_app) = bundle.admin_app {
        let admin_bind: SocketAddr = config
            .server
            .admin_bind
            .as_ref()
            .expect("admin_bind must be set")
            .parse()
            .expect("Invalid admin bind address");
        if !admin_bind.ip().is_loopback() && config.server.admin_token.is_none() {
            warn!(
                "Admin server binding to non-loopback address {}. \
                 The /metrics endpoint has no authentication — ensure network-level access control \
                 or set server.admin_token.",
                admin_bind
            );
        }
        let admin_listener = TcpListener::bind(admin_bind).await.expect("Failed to bind admin port");
        info!("Admin server listening on {}", admin_listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(admin_listener, admin_app)
                .with_graceful_shutdown(netray_common::server::shutdown_signal())
                .await
                .expect("Admin server error");
        });
    }

    let app = bundle.app.into_make_service_with_connect_info::<SocketAddr>();
    let listener = TcpListener::bind(bind_addr).await.expect("Failed to bind");
    info!("Listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(netray_common::server::shutdown_signal())
        .await
        .expect("Server error");

    netray_common::telemetry::shutdown();
}

async fn reload_enrichment(
    handle: &Arc<ArcSwap<crate::enrichment::EnrichmentContext>>,
    config: &Config,
    trigger: &str,
) {
    info!("{} triggered, reloading enrichment data...", trigger);
    match crate::enrichment::EnrichmentContext::load(config).await {
        Ok(new_ctx) => {
            handle.store(Arc::new(new_ctx));
            info!("Enrichment data reloaded successfully");
        }
        Err(e) => {
            warn!("Failed to reload enrichment data: {}; keeping previous context", e);
        }
    }
}

/// Validate all configured data files and print a summary.
/// Returns 0 if all mandatory sources loaded successfully, 1 otherwise.
async fn run_check(config: &Config) -> i32 {
    println!("Checking configuration and data files...\n");

    let mut ok = true;

    // Check mandatory file fields
    let mandatory = [
        ("geoip_city_db", config.geoip_city_db.as_deref()),
        ("geoip_asn_db", config.geoip_asn_db.as_deref()),
        ("user_agent_regexes", config.user_agent_regexes.as_deref()),
    ];
    for (name, path) in &mandatory {
        match path {
            None => {
                println!("[MISSING] {name}: not configured (required)");
                ok = false;
            }
            Some(p) => {
                if tokio::fs::metadata(p).await.is_ok() {
                    println!("[OK]      {name}: {p}");
                } else {
                    println!("[ERROR]   {name}: {p} — file not found");
                    ok = false;
                }
            }
        }
    }

    // Check optional file fields
    let optional = [
        ("tor_exit_nodes", config.tor_exit_nodes.as_deref()),
        ("cloud_provider_ranges", config.cloud_provider_ranges.as_deref()),
        ("feodo_botnet_ips", config.feodo_botnet_ips.as_deref()),
        ("cins_army_ips", config.cins_army_ips.as_deref()),
        ("vpn_ranges", config.vpn_ranges.as_deref()),
        ("datacenter_ranges", config.datacenter_ranges.as_deref()),
        ("bot_ranges", config.bot_ranges.as_deref()),
        ("spamhaus_drop", config.spamhaus_drop.as_deref()),
        ("asn_patterns", config.asn_patterns.as_deref()),
        ("asn_info", config.asn_info.as_deref()),
    ];
    for (name, path) in &optional {
        match path {
            None => println!("[SKIP]    {name}: not configured"),
            Some(p) => {
                if tokio::fs::metadata(p).await.is_ok() {
                    println!("[OK]      {name}: {p}");
                } else {
                    println!("[WARN]    {name}: {p} — file not found");
                }
            }
        }
    }

    // Attempt a full enrichment context load to catch parse errors
    println!("\nAttempting enrichment context load...");
    match EnrichmentContext::load(config).await {
        Ok(ctx) => {
            println!("[OK]      Enrichment context loaded successfully");
            if !ctx.missing_optional.is_empty() {
                println!("          Warnings: {}", ctx.missing_optional.join(", "));
            }
        }
        Err(e) => {
            println!("[ERROR]   Enrichment context failed to load: {e}");
            ok = false;
        }
    }

    println!();
    if ok {
        println!("Check passed.");
        0
    } else {
        println!("Check FAILED — see errors above.");
        1
    }
}

fn spawn_file_watcher(config: Arc<Config>, enrichment_handle: Arc<ArcSwap<crate::enrichment::EnrichmentContext>>) {
    use notify::{RecommendedWatcher, RecursiveMode, Watcher};

    // Collect unique parent directories of all configured data file paths
    let data_paths: Vec<&Option<String>> = vec![
        &config.geoip_city_db,
        &config.geoip_asn_db,
        &config.user_agent_regexes,
        &config.tor_exit_nodes,
        &config.cloud_provider_ranges,
        &config.feodo_botnet_ips,
        &config.cins_army_ips,
        &config.vpn_ranges,
        &config.datacenter_ranges,
        &config.bot_ranges,
        &config.spamhaus_drop,
    ];
    let watch_dirs: HashSet<PathBuf> = data_paths
        .iter()
        .filter_map(|opt| opt.as_deref())
        .filter_map(|p| {
            let path = PathBuf::from(p);
            path.parent().map(|parent| parent.to_path_buf())
        })
        .filter(|dir| dir.exists())
        .collect();

    if watch_dirs.is_empty() {
        warn!("watch_data_files enabled but no data file directories found to watch");
        return;
    }

    let (tx, mut rx) = tokio::sync::mpsc::channel::<()>(16);

    // Create watcher in a dedicated thread (notify uses sync callbacks)
    std::thread::spawn(move || {
        let tx_clone = tx.clone();
        let mut watcher: RecommendedWatcher =
            match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
                if let Ok(event) = res {
                    use notify::EventKind;
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_) => {
                            let _ = tx_clone.try_send(());
                        }
                        _ => {}
                    }
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("Failed to create filesystem watcher: {}", e);
                    return;
                }
            };

        for dir in &watch_dirs {
            match watcher.watch(dir, RecursiveMode::NonRecursive) {
                Ok(()) => info!("Watching directory for changes: {}", dir.display()),
                Err(e) => warn!("Failed to watch {}: {}", dir.display(), e),
            }
        }

        // Keep the watcher alive
        std::thread::park();
    });

    // Debounce + reload loop
    tokio::spawn(async move {
        loop {
            // Wait for first event
            if rx.recv().await.is_none() {
                break;
            }
            // Debounce: drain events for 500ms
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            while rx.try_recv().is_ok() {}

            reload_enrichment(&enrichment_handle, &config, "File change").await;
        }
    });
}
