use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

/// Hard cap for backend_timeout_secs — no request to a backend may exceed this.
const HARD_CAP_BACKEND_TIMEOUT_SECS: u64 = 25;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_backends")]
    pub backends: BackendsConfig,
    #[serde(default = "default_cache")]
    pub cache: CacheConfig,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub telemetry: netray_common::telemetry::TelemetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: SocketAddr,
    #[serde(default = "default_metrics_bind")]
    pub metrics_bind: SocketAddr,
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BackendsConfig {
    pub dns_url: String,
    pub tls_url: String,
    pub ip_url: String,
    #[serde(default)]
    pub http_url: Option<String>,
    #[serde(default = "default_backend_timeout_secs")]
    pub backend_timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cache_ttl_seconds")]
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_per_ip_per_minute")]
    pub per_ip_per_minute: u32,
    #[serde(default = "default_per_ip_burst")]
    pub per_ip_burst: u32,
    #[serde(default = "default_global_per_minute")]
    pub global_per_minute: u32,
    #[serde(default = "default_global_burst")]
    pub global_burst: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ScoringConfig {
    pub profile_path: Option<String>,
}

// --- Default implementations ---

impl Default for ServerConfig {
    fn default() -> Self {
        default_server()
    }
}

impl Default for BackendsConfig {
    fn default() -> Self {
        default_backends()
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        default_cache()
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        default_rate_limit()
    }
}

// --- Default value functions ---

fn default_server() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        metrics_bind: default_metrics_bind(),
        trusted_proxies: Vec::new(),
    }
}

fn default_backends() -> BackendsConfig {
    BackendsConfig {
        dns_url: String::new(),
        tls_url: String::new(),
        ip_url: String::new(),
        http_url: None,
        backend_timeout_secs: default_backend_timeout_secs(),
    }
}

fn default_cache() -> CacheConfig {
    CacheConfig {
        enabled: true,
        ttl_seconds: default_cache_ttl_seconds(),
    }
}

fn default_rate_limit() -> RateLimitConfig {
    RateLimitConfig {
        per_ip_per_minute: default_per_ip_per_minute(),
        per_ip_burst: default_per_ip_burst(),
        global_per_minute: default_global_per_minute(),
        global_burst: default_global_burst(),
    }
}

fn default_bind() -> SocketAddr {
    ([0, 0, 0, 0], 8082).into()
}

fn default_metrics_bind() -> SocketAddr {
    ([127, 0, 0, 1], 9090).into()
}

fn default_backend_timeout_secs() -> u64 {
    20
}

fn default_cache_ttl_seconds() -> u64 {
    300
}

fn default_per_ip_per_minute() -> u32 {
    10
}

fn default_per_ip_burst() -> u32 {
    3
}

fn default_global_per_minute() -> u32 {
    100
}

fn default_global_burst() -> u32 {
    20
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from an optional TOML file path and environment variables.
    ///
    /// Precedence (highest first): env vars (LENS_ prefix) > TOML file > built-in defaults.
    pub fn load(config_path: Option<&str>) -> Result<Self, ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(path) = config_path {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }

        // LENS_ prefix, __ section separator.
        // e.g. LENS_RATE_LIMIT__PER_IP_PER_MINUTE=20 maps to rate_limit.per_ip_per_minute.
        builder = builder.add_source(
            config::Environment::with_prefix("LENS")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        let raw = builder.build()?;
        let mut cfg: Config = raw.try_deserialize()?;
        cfg.validate()?;

        Ok(cfg)
    }

    /// Validate and clamp configuration values.
    ///
    /// - `backend_timeout_secs` is clamped to `HARD_CAP_BACKEND_TIMEOUT_SECS`.
    /// - Zero values for rate limits and timeouts are rejected.
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        // Clamp backend timeout to hard cap.
        if self.backends.backend_timeout_secs > HARD_CAP_BACKEND_TIMEOUT_SECS {
            tracing::warn!(
                configured = self.backends.backend_timeout_secs,
                clamped = HARD_CAP_BACKEND_TIMEOUT_SECS,
                "backend_timeout_secs exceeds hard cap, clamping"
            );
            self.backends.backend_timeout_secs = HARD_CAP_BACKEND_TIMEOUT_SECS;
        }

        // Reject zero values — would disable protections or cause division-by-zero.
        reject_zero(
            "backends.backend_timeout_secs",
            self.backends.backend_timeout_secs,
        )?;
        reject_zero(
            "rate_limit.per_ip_per_minute",
            self.rate_limit.per_ip_per_minute,
        )?;
        reject_zero("rate_limit.per_ip_burst", self.rate_limit.per_ip_burst)?;
        reject_zero(
            "rate_limit.global_per_minute",
            self.rate_limit.global_per_minute,
        )?;
        reject_zero("rate_limit.global_burst", self.rate_limit.global_burst)?;

        Ok(())
    }
}

fn reject_zero<T: PartialEq + From<u8>>(name: &str, value: T) -> Result<(), ConfigError> {
    if value == T::from(0) {
        return Err(ConfigError::Message(format!(
            "invalid configuration: {name} must not be zero"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_config() -> Config {
        Config {
            server: default_server(),
            backends: BackendsConfig {
                dns_url: "http://localhost:8080".to_string(),
                tls_url: "http://localhost:8081".to_string(),
                ip_url: "http://localhost:8082".to_string(),
                http_url: None,
                backend_timeout_secs: default_backend_timeout_secs(),
            },
            cache: default_cache(),
            rate_limit: default_rate_limit(),
            scoring: ScoringConfig::default(),
            telemetry: Default::default(),
        }
    }

    // --- Valid defaults ---

    #[test]
    fn default_config_passes_validation() {
        let mut cfg = valid_config();
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn default_bind_is_0000_8082() {
        let cfg = valid_config();
        assert_eq!(cfg.server.bind.to_string(), "0.0.0.0:8082");
    }

    #[test]
    fn default_metrics_bind_is_127001_9090() {
        let cfg = valid_config();
        assert_eq!(cfg.server.metrics_bind.to_string(), "127.0.0.1:9090");
    }

    #[test]
    fn default_backend_timeout_is_20() {
        let cfg = valid_config();
        assert_eq!(cfg.backends.backend_timeout_secs, 20);
    }

    #[test]
    fn default_cache_enabled_with_300s_ttl() {
        let cfg = valid_config();
        assert!(cfg.cache.enabled);
        assert_eq!(cfg.cache.ttl_seconds, 300);
    }

    #[test]
    fn default_rate_limits() {
        let cfg = valid_config();
        assert_eq!(cfg.rate_limit.per_ip_per_minute, 10);
        assert_eq!(cfg.rate_limit.per_ip_burst, 3);
        assert_eq!(cfg.rate_limit.global_per_minute, 100);
        assert_eq!(cfg.rate_limit.global_burst, 20);
    }

    // --- Hard-cap clamping ---

    #[test]
    fn clamps_backend_timeout_secs() {
        let mut cfg = valid_config();
        cfg.backends.backend_timeout_secs = HARD_CAP_BACKEND_TIMEOUT_SECS + 99;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.backends.backend_timeout_secs,
            HARD_CAP_BACKEND_TIMEOUT_SECS
        );
    }

    #[test]
    fn hard_cap_exact_value_is_accepted() {
        let mut cfg = valid_config();
        cfg.backends.backend_timeout_secs = HARD_CAP_BACKEND_TIMEOUT_SECS;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.backends.backend_timeout_secs,
            HARD_CAP_BACKEND_TIMEOUT_SECS
        );
    }

    // --- Zero-value rejection ---

    macro_rules! zero_rejects {
        ($name:ident, $field:expr) => {
            #[test]
            fn $name() {
                let mut cfg = valid_config();
                $field(&mut cfg);
                let err = cfg.validate().unwrap_err().to_string();
                assert!(
                    err.contains("must not be zero"),
                    "expected 'must not be zero' in: {err}"
                );
            }
        };
    }

    zero_rejects!(rejects_zero_backend_timeout, |c: &mut Config| {
        c.backends.backend_timeout_secs = 0
    });
    zero_rejects!(rejects_zero_per_ip_per_minute, |c: &mut Config| {
        c.rate_limit.per_ip_per_minute = 0
    });
    zero_rejects!(rejects_zero_per_ip_burst, |c: &mut Config| {
        c.rate_limit.per_ip_burst = 0
    });
    zero_rejects!(rejects_zero_global_per_minute, |c: &mut Config| {
        c.rate_limit.global_per_minute = 0
    });
    zero_rejects!(rejects_zero_global_burst, |c: &mut Config| {
        c.rate_limit.global_burst = 0
    });
}
