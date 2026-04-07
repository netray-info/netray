use lens::config;
use lens::routes;
use lens::state;

use std::net::SocketAddr;

use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::trace::TraceLayer;

use axum::Router;
use axum::routing::get;

use netray_common::security_headers::{SecurityHeadersConfig, security_headers_layer};
pub use netray_common::cors::cors_layer;

#[tokio::main]
async fn main() {
    // 1. Load config (first arg or LENS_CONFIG env var).
    let config_path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("LENS_CONFIG").ok());

    let config =
        config::Config::load(config_path.as_deref()).expect("failed to load configuration");

    // 2. Init tracing.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "lens=info,tower_http=info".parse().unwrap()),
        )
        .init();

    tracing::info!(
        bind = %config.server.bind,
        dns_url = %config.backends.dns_url,
        tls_url = %config.backends.tls_url,
        ip_url  = %config.backends.ip_url,
        "starting lens"
    );

    // 3. Build app state.
    let state = state::AppState::new(config.clone()).expect("failed to build app state");

    // 4. Build router.
    let app = Router::new()
        .merge(routes::health_router())
        .merge(routes::api_router(state))
        .route("/robots.txt", get(robots_txt))
        .fallback(netray_common::server::static_handler::<routes::Assets>())
        .layer(axum::middleware::from_fn(|req, next| {
            netray_common::middleware::http_metrics("lens", req, next)
        }))
        .layer(axum::middleware::from_fn(
            netray_common::middleware::request_id,
        ))
        .layer(axum::middleware::from_fn(security_headers_mw))
        .layer(cors_layer())
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(8 * 1024));

    // 5. Graceful shutdown channel.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        netray_common::server::shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });

    // 6. Metrics server.
    let metrics_addr = config.server.metrics_bind;
    let metrics_shutdown = shutdown_rx.clone();
    tracing::info!(
        addr = %metrics_addr,
        "metrics server starting — ensure this address is NOT publicly reachable"
    );
    tokio::spawn(async move {
        if let Err(e) =
            netray_common::server::serve_metrics(metrics_addr, metrics_shutdown).await
        {
            tracing::error!(error = %e, "metrics server failed");
        }
    });

    // 7. Bind and serve.
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

async fn security_headers_mw(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let layer_fn = security_headers_layer(SecurityHeadersConfig {
        extra_script_src: vec![],
        relaxed_csp_path_prefix: "/docs".to_string(),
        include_permissions_policy: true,
    });
    layer_fn(request, next).await
}
