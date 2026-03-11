//! Signal-based hot configuration reload.
//!
//! Listens for `SIGHUP` and re-reads the config file, swapping the live config
//! via `ArcSwap`. On parse or validation failure the previous config is retained.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::config::Config;

/// Spawn a background task that reloads config on SIGHUP.
///
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
pub fn spawn_reload_watcher(config_path: Option<String>, config: Arc<ArcSwap<Config>>) {
    tokio::spawn(async move {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to install SIGHUP handler");

        loop {
            signal.recv().await;
            tracing::info!("SIGHUP received, reloading configuration");
            reload_config(config_path.as_deref(), &config);
        }
    });
}

#[cfg(not(unix))]
pub fn spawn_reload_watcher(_config_path: Option<String>, _config: Arc<ArcSwap<Config>>) {
    tracing::debug!("SIGHUP reload not available on this platform");
}

fn reload_config(config_path: Option<&str>, config: &ArcSwap<Config>) {
    let new_config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(error = %e, "config reload failed: parse/validation error");
            metrics::counter!("tlsight_config_reloads_total", "result" => "failure").increment(1);
            return;
        }
    };

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

    #[test]
    fn reload_with_no_config_file_uses_defaults() {
        let config = Config::load(None).expect("default config");
        let shared = Arc::new(ArcSwap::from_pointee(config));

        reload_config(None, &shared);

        let loaded = shared.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 30);
    }

    #[test]
    fn reload_with_bad_file_keeps_previous_config() {
        let config = Config::load(None).expect("default config");
        let shared = Arc::new(ArcSwap::from_pointee(config));

        reload_config(Some("/nonexistent/path.toml"), &shared);

        let loaded = shared.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 30);
    }
}
