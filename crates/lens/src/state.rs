use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use crate::cache::CachedResult;
use crate::config::Config;
use crate::scoring::ScoringProfile;
use crate::security::rate_limit::{GlobalRateLimiter, PerIpRateLimiter};

/// Shared application state passed to every axum handler via `axum::extract::State`.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub per_ip_limiter: Arc<PerIpRateLimiter>,
    pub global_limiter: Arc<GlobalRateLimiter>,
    pub http_client: reqwest::Client,
    pub cache: Option<Arc<Cache<String, Arc<CachedResult>>>>,
    pub scoring_profile: Arc<ScoringProfile>,
}

impl AppState {
    /// Build `AppState` from a validated `Config`.
    ///
    /// - Constructs a `reqwest::Client` with rustls, a timeout from config, and a
    ///   `User-Agent` of `lens/<version>`.
    /// - Constructs per-IP and global GCRA rate limiters.
    /// - Optionally constructs a `moka` async cache when `config.cache.enabled`.
    /// - Loads the scoring profile from `config.scoring.profile_path` if set, or
    ///   falls back to the embedded `profiles/default.toml`.
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let timeout = Duration::from_secs(config.backends.backend_timeout_secs);

        let http_client = reqwest::Client::builder()
            .use_rustls_tls()
            .timeout(timeout)
            .user_agent(concat!("lens/", env!("CARGO_PKG_VERSION")))
            .build()?;

        let per_ip_limiter = Arc::new(PerIpRateLimiter::new(&config.rate_limit));
        let global_limiter = Arc::new(GlobalRateLimiter::new(&config.rate_limit));

        let cache = if config.cache.enabled {
            let ttl = Duration::from_secs(config.cache.ttl_seconds);
            let c: Cache<String, Arc<CachedResult>> = Cache::builder().time_to_live(ttl).build();
            Some(Arc::new(c))
        } else {
            None
        };

        let scoring_profile = Arc::new(load_scoring_profile(
            config.scoring.profile_path.as_deref(),
        )?);

        Ok(Self {
            config: Arc::new(config),
            per_ip_limiter,
            global_limiter,
            http_client,
            cache,
            scoring_profile,
        })
    }
}

/// Load the scoring profile from a file path, or fall back to the embedded default.
fn load_scoring_profile(path: Option<&str>) -> Result<ScoringProfile, Box<dyn std::error::Error>> {
    match path {
        Some(p) => {
            let contents = std::fs::read_to_string(p)?;
            Ok(ScoringProfile::from_toml(&contents)?)
        }
        None => Ok(ScoringProfile::embedded_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BackendsConfig, CacheConfig, RateLimitConfig, ScoringConfig, ServerConfig,
    };

    fn test_config() -> Config {
        Config {
            server: ServerConfig {
                bind: ([0, 0, 0, 0], 8082).into(),
                metrics_bind: ([127, 0, 0, 1], 8090).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                dns_url: "http://localhost:8080".to_string(),
                tls_url: "http://localhost:8081".to_string(),
                ip_url: "http://localhost:8082".to_string(),
                backend_timeout_secs: 20,
            },
            cache: CacheConfig {
                enabled: false,
                ttl_seconds: 300,
            },
            rate_limit: RateLimitConfig {
                per_ip_per_minute: 10,
                per_ip_burst: 3,
                global_per_minute: 100,
                global_burst: 20,
            },
            scoring: ScoringConfig::default(),
        }
    }

    #[test]
    fn builds_state_from_config() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        assert!(
            state.cache.is_none(),
            "cache should be disabled in test config"
        );
    }

    #[test]
    fn state_is_clone() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        let _cloned = state.clone();
    }

    #[test]
    fn cache_built_when_enabled() {
        let mut config = test_config();
        config.cache.enabled = true;
        config.cache.ttl_seconds = 60;
        let state = AppState::new(config).unwrap();
        assert!(state.cache.is_some(), "cache should be built when enabled");
    }

    #[test]
    fn embedded_default_profile_loads() {
        let profile = load_scoring_profile(None);
        assert!(
            profile.is_ok(),
            "embedded default profile must parse: {:?}",
            profile.err()
        );
    }
}
