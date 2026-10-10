//! Signal-based hot configuration reload.
//!
//! Listens for `SIGHUP` and re-reads the config file, swapping the live config
//! via `ArcSwap`. On parse or validation failure the previous config is retained.
//! Also reloads the custom CA directory from the (newly loaded) config.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::config::Config;
use crate::state;

/// Spawn a background task that reloads config on SIGHUP.
///
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
pub fn spawn_reload_watcher(
    config_path: Option<String>,
    config: Arc<ArcSwap<Config>>,
    trust_store: Arc<ArcSwap<rustls::RootCertStore>>,
) {
    tokio::spawn(async move {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to install SIGHUP handler");

        loop {
            signal.recv().await;
            tracing::info!("SIGHUP received, reloading configuration");
            reload_config(config_path.as_deref(), &config, &trust_store);
        }
    });
}

#[cfg(not(unix))]
pub fn spawn_reload_watcher(
    _config_path: Option<String>,
    _config: Arc<ArcSwap<Config>>,
    _trust_store: Arc<ArcSwap<rustls::RootCertStore>>,
) {
    tracing::debug!("SIGHUP reload not available on this platform");
}

fn reload_config(
    config_path: Option<&str>,
    config: &ArcSwap<Config>,
    trust_store: &ArcSwap<rustls::RootCertStore>,
) {
    let new_config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(error = %e, "config reload failed: parse/validation error");
            metrics::counter!("tlsight_config_reloads_total", "result" => "failure").increment(1);
            return;
        }
    };

    // Reload custom CA directory from the new config
    let ca_dir = new_config.validation.custom_ca_dir.as_deref();
    let custom_cas = state::reload_custom_cas(ca_dir, trust_store);
    tracing::info!(custom_cas = custom_cas, "CA store reloaded");

    config.store(Arc::new(new_config));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    metrics::gauge!("tlsight_config_last_reload_timestamp").set(now);
    metrics::counter!("tlsight_config_reloads_total", "result" => "success").increment(1);

    tracing::info!("configuration reloaded successfully");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_trust_store() -> Arc<ArcSwap<rustls::RootCertStore>> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Arc::new(ArcSwap::from_pointee(rustls::RootCertStore::empty()))
    }

    #[test]
    fn reload_with_no_config_file_uses_defaults() {
        let config = Config::load(None).expect("default config");
        let shared = Arc::new(ArcSwap::from_pointee(config));
        let ts = empty_trust_store();

        reload_config(None, &shared, &ts);

        let loaded = shared.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 30);
    }

    #[test]
    fn reload_with_bad_file_keeps_previous_config() {
        let config = Config::load(None).expect("default config");
        let shared = Arc::new(ArcSwap::from_pointee(config));
        let ts = empty_trust_store();

        reload_config(Some("/nonexistent/path.toml"), &shared, &ts);

        let loaded = shared.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 30);
    }
}
