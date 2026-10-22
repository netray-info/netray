//! `netray serve`: the apex, the inspector hosts and the probes in one process, composed from
//! netray-web, netray-engine and netray-store (spec v2-cutover, requirement 5).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use netray_engine::domain_run::HARD_DEADLINE;
use netray_web::host::{Apex, HostRouters};
use netray_web::metrics::ClientRunCounter;

use crate::config::Config;
use crate::registry::{module_registry, reload_ip_on_sighup};

/// Installs the tracing subscriber (with the optional OpenTelemetry layer), before the registry
/// is built, so the module load warnings are logged.
pub fn init_telemetry(cfg: &Config) {
    netray_common::telemetry::init_subscriber(
        &cfg.telemetry,
        "info,netray=debug,netray_web=debug,netray_engine=debug,netray_store=debug,hyper=warn,h2=warn",
    );
}

/// `netray serve --check-config`: the config and its module tables, no data files.
pub async fn check_config(path: &str) -> Result<(), String> {
    let cfg = Config::load(Some(path), std::env::vars_os().collect()).map_err(|e| e.to_string())?;
    module_registry(&cfg.module_tables(), false).await.map(drop)
}

/// Opens and migrates the history store at `[store] db_path`, runs its sweep and sets its
/// gauges; exits 1 naming the history store when it cannot be opened or migrated.
async fn open_store(cfg: &Config) -> Option<Arc<netray_store::HistoryStore>> {
    if !cfg.store.enabled {
        return None;
    }
    let db_path = &cfg.store.db_path;
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
    let store = Arc::new(store);
    tokio::spawn(netray_store::run_sweep_loop(Arc::clone(&store)));
    netray_store::set_gauges(&store).await;
    tracing::info!(db_path = %db_path.display(), "history enabled");
    Some(store)
}

/// Serves the configuration at `path` (else `NETRAY_CONFIG`) until a shutdown signal.
pub async fn run(path: Option<String>) -> anyhow::Result<()> {
    let path = path.or_else(|| std::env::var("NETRAY_CONFIG").ok());
    let cfg = match Config::load(path.as_deref(), std::env::vars_os().collect()) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("failed to load configuration: {e}");
            std::process::exit(1)
        }
    };
    init_telemetry(&cfg);
    // The recorder goes in before the registry, so the startup gauges land on it.
    let metrics = netray_web::metrics::install_metrics();
    let (registry, ip, parts) = module_registry(&cfg.module_tables(), true)
        .await
        .map_err(|e| anyhow::anyhow!(e))?
        .split();
    if let Some(ip) = ip {
        reload_ip_on_sighup(ip);
    }
    let profile = netray_web::admission::load_scoring_profile(cfg.engine.profile_path.as_deref())
        .map_err(|e| anyhow::anyhow!("failed to load the scoring profile: {e}"))?;
    let apex = cfg.apex_config();
    tracing::info!(
        bind = %cfg.server.bind,
        per_ip_rate = apex.limits.rate_limit_per_ip_per_minute,
        per_ip_burst = apex.limits.rate_limit_per_ip_burst,
        global_rate = apex.limits.rate_limit_global_per_minute,
        global_burst = apex.limits.rate_limit_global_burst,
        trusted_proxy_count = apex.trusted_proxies.len(),
        cache_enabled = apex.cache_enabled,
        cache_window_seconds = apex.cache_window_seconds,
        "starting netray"
    );
    let store = open_store(&cfg).await;
    if apex.badges.enabled {
        tracing::info!(
            ttl_seconds = apex.badges.ttl_seconds,
            default_label = %apex.badges.default_label,
            "badges enabled"
        );
    }
    if apex.og_cards.enabled {
        tracing::info!("og cards enabled");
    }
    let api_enabled = apex.api.enabled;
    let plan = netray_engine::section_plan(&registry, |p| Some(cfg.section_window(p)));
    let Apex {
        apex_router,
        dns,
        tls,
        http,
        ip,
        api,
        client_runs,
        runs,
    } = netray_web::host::build(
        apex,
        registry,
        plan,
        cfg.resolve_budget(),
        profile,
        store,
        parts,
    );
    let app = netray_web::host::router(HostRouters {
        apex: cfg.server.apex.clone(),
        probes: netray_web::host::probes(),
        apex_router,
        dns: Some(dns),
        tls: Some(tls),
        http: Some(http),
        ip: Some(ip),
        ip_api: Some(api),
    });
    let app = netray_web::host::layers(app, api_enabled);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    tokio::spawn(run_client_runs_flush(client_runs, shutdown_rx.clone()));

    let metrics_addr = cfg.server.metrics_bind;
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

    let listener = tokio::net::TcpListener::bind(cfg.server.bind)
        .await
        .expect("failed to bind server address");
    tracing::info!(addr = %cfg.server.bind, "netray listening");

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
    .await
    .expect("server error");

    let drain_bound = HARD_DEADLINE + Duration::from_secs(10);
    if tokio::time::timeout(drain_bound, runs.drain())
        .await
        .is_err()
    {
        tracing::warn!(
            bound_s = drain_bound.as_secs(),
            "shutdown: runs still in flight at the drain bound"
        );
    }

    netray_common::telemetry::shutdown();
    Ok(())
}

/// Flushes the per-client run counts into `netray_client_hourly_runs` every hour.
async fn run_client_runs_flush(
    counter: Arc<ClientRunCounter>,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut ticker = tokio::time::interval(Duration::from_secs(3600));
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
