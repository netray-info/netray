//! Signal-based hot configuration reload.
//!
//! Listens for `SIGHUP` and re-reads the config file, swapping the hot-reloadable
//! fields via `ArcSwap`. Cold config (bind address, TLS, trusted proxies) is
//! unchanged. On parse or validation failure the previous config is retained.

use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::config::{Config, HotConfig};
use crate::security::RateLimitState;

/// Shared handle to the hot-reloadable configuration and derived state.
///
/// Handlers read `hot_config.load()` on each request (lock-free, wait-free).
/// The reload task swaps in a new `HotConfig` and rebuilds the `RateLimitState`
/// when rate limit parameters change.
#[derive(Clone)]
pub struct HotState {
    pub hot_config: Arc<ArcSwap<HotConfig>>,
    pub rate_limiter: Arc<ArcSwap<RateLimitState>>,
}

impl HotState {
    pub fn new(config: &Config) -> Self {
        Self {
            hot_config: Arc::new(ArcSwap::from_pointee(config.hot())),
            rate_limiter: Arc::new(ArcSwap::from_pointee(RateLimitState::new(&config.limits))),
        }
    }
}

/// Spawn a background task that reloads hot config on SIGHUP.
///
/// On non-Unix platforms this is a no-op.
#[cfg(unix)]
pub fn spawn_reload_watcher(config_path: Option<String>, hot_state: HotState) {
    tokio::spawn(async move {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())
            .expect("failed to install SIGHUP handler");

        loop {
            signal.recv().await;
            tracing::info!("SIGHUP received, reloading configuration");
            reload_hot_config(config_path.as_deref(), &hot_state);
        }
    });
}

#[cfg(not(unix))]
pub fn spawn_reload_watcher(_config_path: Option<String>, _hot_state: HotState) {
    tracing::debug!("SIGHUP reload not available on this platform");
}

fn reload_hot_config(config_path: Option<&str>, hot_state: &HotState) {
    let mut new_config = match Config::load(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            tracing::error!(error = %e, "config reload failed: parse error");
            metrics::counter!("prism_config_reloads_total", "result" => "failure").increment(1);
            return;
        }
    };

    if let Err(e) = new_config.validate_hot() {
        tracing::error!(error = %e, "config reload failed: validation error");
        metrics::counter!("prism_config_reloads_total", "result" => "failure").increment(1);
        return;
    }

    let new_hot = new_config.hot();
    let old_hot = hot_state.hot_config.load();

    // Rebuild rate limiters if rate limit parameters changed.
    if old_hot.limits != new_hot.limits {
        tracing::info!("rate limit configuration changed, rebuilding rate limiters");
        hot_state
            .rate_limiter
            .store(Arc::new(RateLimitState::new(&new_hot.limits)));
    }

    hot_state.hot_config.store(Arc::new(new_hot));

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    metrics::gauge!("prism_config_last_reload_timestamp").set(now);
    metrics::counter!("prism_config_reloads_total", "result" => "success").increment(1);

    tracing::info!("configuration reloaded successfully");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hot_state_initializes_from_default_config() {
        let config = Config::load(None).expect("default config");
        let hot_state = HotState::new(&config);

        let loaded = hot_state.hot_config.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 120);
        assert_eq!(loaded.dns.default_servers, vec!["google".to_owned()]);
    }

    #[test]
    fn reload_with_no_config_file_uses_defaults() {
        let config = Config::load(None).expect("default config");
        let hot_state = HotState::new(&config);

        // Reload without a file path — should succeed with defaults.
        reload_hot_config(None, &hot_state);

        let loaded = hot_state.hot_config.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 120);
    }

    #[test]
    fn reload_with_bad_file_keeps_previous_config() {
        let config = Config::load(None).expect("default config");
        let hot_state = HotState::new(&config);

        // Reload with a nonexistent file — should fail gracefully.
        reload_hot_config(Some("/nonexistent/path.toml"), &hot_state);

        // Previous config retained.
        let loaded = hot_state.hot_config.load();
        assert_eq!(loaded.limits.per_ip_per_minute, 120);
    }
}
