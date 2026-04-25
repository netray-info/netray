use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

pub use config::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
    #[serde(default)]
    pub ecosystem: EcosystemConfig,
    #[serde(default = "default_cache")]
    pub cache: CacheConfig,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: RateLimitConfig,
    #[serde(default)]
    pub scoring: ScoringConfig,
    #[serde(default)]
    pub site: SiteConfig,
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

pub use netray_common::ecosystem::EcosystemConfig;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BackendsConfig {
    #[serde(default)]
    pub dns: netray_common::backend::BackendConfig,
    /// DNS server names to pass to mhost-prism (e.g. `["cloudflare"]`).
    /// When non-empty, sent as the `servers` field in the CheckRequest body.
    #[serde(default)]
    pub dns_servers: Vec<String>,
    #[serde(default)]
    pub tls: netray_common::backend::BackendConfig,
    #[serde(default)]
    pub ip: netray_common::backend::BackendConfig,
    #[serde(default)]
    pub http: Option<netray_common::backend::BackendConfig>,
    #[serde(default)]
    pub email: Option<netray_common::backend::BackendConfig>,
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

/// Apex landing-page branding.
///
/// Every field is optional; missing values fall back to the strings in
/// `SiteConfig::default()`. The 12 fields cover everything an operator can
/// rebrand on the apex without rebuilding the lens image. Product semantics
/// (grade thresholds, per-check `fix_hint` copy, check labels) are NOT
/// configurable here — see `specs/sdd/product-repositioning.md` §11.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SiteConfig {
    pub title: Option<String>,
    pub description: Option<String>,
    pub og_image: Option<String>,
    pub og_site_name: Option<String>,

    pub brand_name: Option<String>,
    pub brand_tagline: Option<String>,
    pub status_pill: Option<String>,

    pub hero_heading: Option<String>,
    pub hero_subheading: Option<String>,
    pub example_domains: Option<Vec<String>>,
    pub trust_strip: Option<String>,

    pub footer_about: Option<String>,
    pub footer_links: Option<Vec<FooterLink>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FooterLink {
    pub label: String,
    pub href: String,
    pub external: bool,
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            title: Some(
                "netray.info — your domain's health grade, in under a second".into(),
            ),
            description: Some(
                "Type a domain, get an A+ to F grade across DNS, TLS, HTTP, and email security. \
                 No account, no ads, open source."
                    .into(),
            ),
            og_image: None,
            og_site_name: Some("netray.info".into()),

            brand_name: Some("lens".into()),
            brand_tagline: Some("your domain's health grade, in under a second".into()),
            status_pill: Some("open source · self-hosted · built in Rust".into()),

            hero_heading: Some("How healthy is your domain?".into()),
            hero_subheading: Some(
                "DNS, TLS, HTTP, email, and the IPs behind them — checked in parallel, one grade, usually under a second.".into(),
            ),
            example_domains: Some(vec![
                "example.com".into(),
                "github.com".into(),
                "cloudflare.com".into(),
            ]),
            trust_strip: Some("No account · No ads · Open source · Self-hostable".into()),

            footer_about: None,
            footer_links: None,
        }
    }
}

// --- Default implementations ---

impl Default for ServerConfig {
    fn default() -> Self {
        default_server()
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

    /// Validate configuration values.
    ///
    /// - Zero values for rate limits are rejected.
    pub fn validate(&mut self) -> Result<(), ConfigError> {
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
                email: None,
            },
            ecosystem: EcosystemConfig::default(),
            cache: default_cache(),
            rate_limit: default_rate_limit(),
            scoring: ScoringConfig::default(),
            site: SiteConfig::default(),
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

    // --- SiteConfig defaults (per SDD product-repositioning §6.1) ---

    #[test]
    fn default_site_populates_all_branding_fields() {
        let s = SiteConfig::default();
        assert!(s.title.is_some());
        assert!(s.description.is_some());
        assert!(s.og_site_name.is_some());
        assert!(s.brand_name.is_some());
        assert!(s.brand_tagline.is_some());
        assert!(s.status_pill.is_some());
        assert!(s.hero_heading.is_some());
        assert!(s.hero_subheading.is_some());
        assert!(s.example_domains.is_some());
        assert!(s.trust_strip.is_some());
        // og_image, footer_about, footer_links default to None — frontend supplies fallbacks.
    }

    #[test]
    fn default_example_domains_match_sdd_requirement_24() {
        let s = SiteConfig::default();
        assert_eq!(
            s.example_domains.as_deref(),
            Some(
                &[
                    "example.com".to_string(),
                    "github.com".to_string(),
                    "cloudflare.com".to_string()
                ][..]
            )
        );
    }

    #[test]
    fn default_brand_name_is_lens() {
        let s = SiteConfig::default();
        assert_eq!(s.brand_name.as_deref(), Some("lens"));
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
