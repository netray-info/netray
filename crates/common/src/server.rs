//! Shared server utilities: graceful shutdown signal, metrics server, and static file handler.

// ---------------------------------------------------------------------------
// Graceful shutdown
// ---------------------------------------------------------------------------

/// Waits for SIGINT (Ctrl-C) or SIGTERM, whichever comes first.
/// Use with axum's `with_graceful_shutdown`.
pub async fn shutdown_signal() {
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

// ---------------------------------------------------------------------------
// Prometheus metrics server
// ---------------------------------------------------------------------------

#[cfg(feature = "prometheus")]
pub use prometheus_server::{
    PrometheusHandle, install_metrics, metrics_recorder, serve_metrics_handle,
};

#[cfg(feature = "prometheus")]
mod prometheus_server {
    use axum::Router;
    use metrics_exporter_prometheus::Matcher;
    pub use metrics_exporter_prometheus::PrometheusHandle;
    use std::net::SocketAddr;

    /// Build, without installing, a Prometheus recorder. Each `(name, buckets)` entry
    /// makes the histogram `name` render with those buckets; an entry with an empty
    /// bucket slice is skipped (the exporter rejects it), so that histogram stays a summary.
    pub fn metrics_recorder(
        buckets: &[(&str, &[f64])],
    ) -> metrics_exporter_prometheus::PrometheusRecorder {
        let mut builder = metrics_exporter_prometheus::PrometheusBuilder::new();
        for (name, bounds) in buckets {
            if bounds.is_empty() {
                continue;
            }
            builder = builder
                .set_buckets_for_metric(Matcher::Full((*name).to_string()), bounds)
                .expect("bucket slice is non-empty");
        }
        builder.build_recorder()
    }

    /// Install the global Prometheus recorder with per-histogram `buckets` and run
    /// `after_install`, so metrics registered before the metrics server binds are kept.
    /// Returns the handle [`serve_metrics_handle`] renders from.
    pub fn install_metrics(
        buckets: &[(&str, &[f64])],
        after_install: impl FnOnce(),
    ) -> Result<PrometheusHandle, Box<dyn std::error::Error + Send + Sync>> {
        let recorder = metrics_recorder(buckets);
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder)?;
        after_install();
        Ok(handle)
    }

    /// Bind a metrics-only HTTP server on `addr` rendering `handle` at `/metrics`.
    pub async fn serve_metrics_handle(
        addr: SocketAddr,
        shutdown: tokio::sync::watch::Receiver<bool>,
        handle: PrometheusHandle,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

    async fn wait_for_shutdown(mut rx: tokio::sync::watch::Receiver<bool>) {
        let _ = rx.wait_for(|v| *v).await;
    }
}
