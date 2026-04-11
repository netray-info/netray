use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;

use crate::backends::Backend;
use crate::backends::dns::DnsBackend;
use crate::backends::http::HttpBackend;
use crate::backends::ip::IpBackend;
use crate::backends::tls::TlsBackend;
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
    /// Ordered: WAVE1_SECTIONS order, then WAVE2_SECTIONS order.
    pub backends: Arc<Vec<Box<dyn Backend + Send + Sync>>>,
}

impl AppState {
    /// Build `AppState` from a validated `Config`.
    pub fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let http_client = reqwest::Client::builder()
            .use_rustls_tls()
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

        let eco = &config.ecosystem;

        let mut backends: Vec<Box<dyn Backend + Send + Sync>> = vec![
            Box::new(DnsBackend {
                dns_url: config.backends.dns.url.clone().unwrap_or_default(),
                public_url: eco.dns_base_url.clone().unwrap_or_default(),
                timeout: Duration::from_millis(config.backends.dns.timeout_ms),
                client: http_client.clone(),
                dns_servers: config.backends.dns_servers.clone(),
            }),
            Box::new(TlsBackend {
                tls_url: config.backends.tls.url.clone().unwrap_or_default(),
                public_url: eco.tls_base_url.clone().unwrap_or_default(),
                timeout: Duration::from_millis(config.backends.tls.timeout_ms),
                client: http_client.clone(),
            }),
        ];
        if let Some(ref http_cfg) = config.backends.http
            && let Some(ref url) = http_cfg.url
        {
            backends.push(Box::new(HttpBackend {
                http_url: url.clone(),
                public_url: eco.http_base_url.clone().unwrap_or_default(),
                timeout: Duration::from_millis(http_cfg.timeout_ms),
                client: http_client.clone(),
            }));
        }
        backends.push(Box::new(IpBackend {
            ip_url: config.backends.ip.url.clone().unwrap_or_default(),
            public_url: eco.ip_base_url.clone().unwrap_or_default(),
            timeout: Duration::from_millis(config.backends.ip.timeout_ms),
            client: http_client.clone(),
        }));

        Ok(Self {
            config: Arc::new(config),
            per_ip_limiter,
            global_limiter,
            http_client,
            cache,
            scoring_profile,
            backends: Arc::new(backends),
        })
    }
}

/// Load the scoring profile from a file path, or fall back to the embedded default.
fn load_scoring_profile(path: Option<&str>) -> Result<ScoringProfile, Box<dyn std::error::Error>> {
    match path {
        Some(p) => {
            let contents = std::fs::read_to_string(p)?;
            let profile = ScoringProfile::from_toml(&contents)?;
            Ok(profile)
        }
        None => Ok(ScoringProfile::embedded_default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        BackendsConfig, CacheConfig, EcosystemConfig, RateLimitConfig, ScoringConfig, ServerConfig,
    };

    fn test_config() -> Config {
        Config {
            server: ServerConfig {
                bind: ([0, 0, 0, 0], 8082).into(),
                metrics_bind: ([127, 0, 0, 1], 8090).into(),
                trusted_proxies: Vec::new(),
            },
            backends: BackendsConfig {
                dns: netray_common::backend::BackendConfig {
                    url: Some("http://localhost:8080".to_string()),
                    ..Default::default()
                },
                dns_servers: Vec::new(),
                tls: netray_common::backend::BackendConfig {
                    url: Some("http://localhost:8081".to_string()),
                    ..Default::default()
                },
                ip: netray_common::backend::BackendConfig {
                    url: Some("http://localhost:8082".to_string()),
                    ..Default::default()
                },
                http: None,
            },
            ecosystem: EcosystemConfig::default(),
            telemetry: Default::default(),
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

    #[test]
    fn backends_registered_correctly() {
        let config = test_config();
        let state = AppState::new(config).unwrap();
        assert_eq!(state.backends.len(), 3);
        assert_eq!(state.backends[0].section(), "dns");
        assert_eq!(state.backends[1].section(), "tls");
        assert_eq!(state.backends[2].section(), "ip");
    }

    #[test]
    fn http_backend_registered_when_url_set() {
        let mut config = test_config();
        config.backends.http = Some(netray_common::backend::BackendConfig {
            url: Some("http://localhost:8083".to_string()),
            ..Default::default()
        });
        let state = AppState::new(config).unwrap();
        assert_eq!(state.backends.len(), 4);
        assert_eq!(state.backends[0].section(), "dns");
        assert_eq!(state.backends[1].section(), "tls");
        assert_eq!(state.backends[2].section(), "http");
        assert_eq!(state.backends[3].section(), "ip");
    }
}
