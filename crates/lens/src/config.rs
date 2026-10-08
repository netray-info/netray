use std::ffi::OsString;
use std::net::SocketAddr;

use serde::{Deserialize, Serialize};

pub use config::ConfigError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
    pub badges: BadgesConfig,
    #[serde(default)]
    pub og_cards: OgCardsConfig,
    #[serde(default)]
    pub snapshots: SnapshotsConfig,
    #[serde(default)]
    pub telemetry: netray_common::telemetry::TelemetryConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BadgesConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_badge_ttl")]
    pub ttl_seconds: u64,
    #[serde(default = "default_badge_label")]
    pub default_label: String,
    #[serde(default = "default_badge_max_label_len")]
    pub max_label_len: usize,
}

fn default_badge_ttl() -> u64 {
    3600
}

fn default_badge_label() -> String {
    "lens".into()
}

fn default_badge_max_label_len() -> usize {
    32
}

impl Default for BadgesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_seconds: default_badge_ttl(),
            default_label: default_badge_label(),
            max_label_len: default_badge_max_label_len(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct BackendsConfig {
    #[serde(default)]
    pub dns: BackendConfig,
    /// DNS server names to pass to mhost-prism (e.g. `["cloudflare"]`).
    /// When non-empty, sent as the `servers` field in the CheckRequest body.
    #[serde(default)]
    pub dns_servers: Vec<String>,
    #[serde(default)]
    pub tls: BackendConfig,
    #[serde(default)]
    pub ip: BackendConfig,
    #[serde(default)]
    pub http: Option<BackendConfig>,
    #[serde(default)]
    pub email: Option<BackendConfig>,
}

/// One backend service. lens calls backends with its own reqwest client, so
/// only `url` and `timeout_ms` are read — not the concurrency and cache keys of
/// `crate::config::BackendConfig`, which lens deliberately rejects.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendConfig {
    /// Base URL of the backend service. `None` disables this backend.
    pub url: Option<String>,
    /// Per-call timeout in milliseconds. The email backend ignores it (fixed 15 s).
    #[serde(default = "default_backend_timeout_ms")]
    pub timeout_ms: u64,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            url: None,
            timeout_ms: default_backend_timeout_ms(),
        }
    }
}

fn default_backend_timeout_ms() -> u64 {
    2000
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CacheConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_cache_ttl_seconds")]
    pub ttl_seconds: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
///
/// Each field carries a `#[serde(default = "...")]` so that a partial TOML
/// override (the common case: set one URL, inherit the rest) keeps every
/// unspecified field at its built-in default. Without per-field defaults,
/// the presence of any `[site]` table in the TOML would fall through to
/// `Option::default()` = `None` for every other field — visibly breaking
/// the apex `<head>`.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    #[serde(default = "default_site_title")]
    pub title: Option<String>,
    #[serde(default = "default_site_description")]
    pub description: Option<String>,
    #[serde(default = "default_site_og_image")]
    pub og_image: Option<String>,
    #[serde(default = "default_site_og_site_name")]
    pub og_site_name: Option<String>,

    #[serde(default = "default_site_brand_name")]
    pub brand_name: Option<String>,
    #[serde(default = "default_site_brand_tagline")]
    pub brand_tagline: Option<String>,
    #[serde(default = "default_site_status_pill")]
    pub status_pill: Option<String>,

    #[serde(default = "default_site_hero_heading")]
    pub hero_heading: Option<String>,
    #[serde(default = "default_site_hero_subheading")]
    pub hero_subheading: Option<String>,
    #[serde(default = "default_site_example_domains")]
    pub example_domains: Option<Vec<String>>,
    #[serde(default = "default_site_trust_strip")]
    pub trust_strip: Option<String>,

    #[serde(default = "default_site_footer_about")]
    pub footer_about: Option<String>,
    #[serde(default = "default_site_footer_links")]
    pub footer_links: Option<Vec<FooterLink>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FooterLink {
    pub label: String,
    pub href: String,
    pub external: bool,
}

fn default_site_title() -> Option<String> {
    Some("netray.info — your domain's health grade, in under a second".into())
}

fn default_site_description() -> Option<String> {
    Some(
        "Type a domain, get an A+ to F grade across DNS, TLS, HTTP, and email security. \
         No account, no ads, open source."
            .into(),
    )
}

fn default_site_og_image() -> Option<String> {
    None
}

fn default_site_og_site_name() -> Option<String> {
    Some("netray.info".into())
}

fn default_site_brand_name() -> Option<String> {
    Some("lens".into())
}

fn default_site_brand_tagline() -> Option<String> {
    Some("your domain's health grade, in under a second".into())
}

fn default_site_status_pill() -> Option<String> {
    Some("open source · built in Rust".into())
}

fn default_site_hero_heading() -> Option<String> {
    Some("How healthy is your domain?".into())
}

fn default_site_hero_subheading() -> Option<String> {
    Some(
        "DNS, TLS, HTTP, email, and the IPs behind them — checked in parallel, one grade, usually under a second.".into(),
    )
}

// netray.info leads as a self-demonstration ("eat your own dog food"); SDD §3
// Requirement 24 listed only the three external examples, but for the
// netray.info-flavored lens build it makes sense to surface our own apex
// first. Override via [site] in the config file.
fn default_site_example_domains() -> Option<Vec<String>> {
    Some(vec![
        "netray.info".into(),
        "example.com".into(),
        "github.com".into(),
        "cloudflare.com".into(),
    ])
}

fn default_site_trust_strip() -> Option<String> {
    Some("No account · No ads · Open source".into())
}

fn default_site_footer_about() -> Option<String> {
    None
}

fn default_site_footer_links() -> Option<Vec<FooterLink>> {
    None
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            title: default_site_title(),
            description: default_site_description(),
            og_image: default_site_og_image(),
            og_site_name: default_site_og_site_name(),

            brand_name: default_site_brand_name(),
            brand_tagline: default_site_brand_tagline(),
            status_pill: default_site_status_pill(),

            hero_heading: default_site_hero_heading(),
            hero_subheading: default_site_hero_subheading(),
            example_domains: default_site_example_domains(),
            trust_strip: default_site_trust_strip(),

            footer_about: default_site_footer_about(),
            footer_links: default_site_footer_links(),
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

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OgCardsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for OgCardsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_snapshots_db_path")]
    pub db_path: std::path::PathBuf,
}

fn default_snapshots_db_path() -> std::path::PathBuf {
    std::path::PathBuf::from("snapshots.db")
}

impl Default for SnapshotsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            db_path: default_snapshots_db_path(),
        }
    }
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from an optional TOML file path and environment variables.
    ///
    /// Precedence (highest first): env vars (LENS_ prefix) > TOML file > built-in defaults.
    pub fn load(config_path: Option<&str>) -> Result<Self, ConfigError> {
        Self::load_with_env(config_path, std::env::vars_os())
    }

    // e.g. LENS_RATE_LIMIT__PER_IP_PER_MINUTE=20 maps to rate_limit.per_ip_per_minute.
    // LENS_LIVE_TESTS is a test switch, not a config key.
    fn load_with_env(
        config_path: Option<&str>,
        env: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Result<Self, ConfigError> {
        let env = env.into_iter().filter(|(k, _)| k != "LENS_LIVE_TESTS");
        let mut cfg: Config = netray_common::config::load_with_env(config_path, "LENS_", env)?;
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
        reject_zero("badges.ttl_seconds", self.badges.ttl_seconds)?;
        netray_common::telemetry::validate(&self.telemetry).map_err(ConfigError::Message)?;

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
                dns: crate::config::BackendConfig {
                    url: Some("http://localhost:8080".to_string()),
                    ..Default::default()
                },
                dns_servers: Vec::new(),
                tls: crate::config::BackendConfig {
                    url: Some("http://localhost:8081".to_string()),
                    ..Default::default()
                },
                ip: crate::config::BackendConfig {
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
            badges: BadgesConfig::default(),
            og_cards: OgCardsConfig::default(),
            snapshots: SnapshotsConfig::default(),
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
    fn badges_defaults_enabled_with_3600s_ttl() {
        let cfg = BadgesConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.ttl_seconds, 3600);
        assert_eq!(cfg.default_label, "lens");
        assert_eq!(cfg.max_label_len, 32);
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
    fn default_example_domains_lead_with_netray_info() {
        // SDD §3 Requirement 24 specified the three external examples;
        // for the netray.info-flavored build we prepend netray.info as a
        // self-demonstration. [site] / env overrides replace it.
        let s = SiteConfig::default();
        assert_eq!(
            s.example_domains.as_deref(),
            Some(
                &[
                    "netray.info".to_string(),
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

    // Regression: a TOML file that contains a [site] table with only a
    // subset of fields (the common operator pattern: override one URL,
    // inherit everything else) used to drop every other field to None
    // because #[serde(default)] on the Config field only fires when the
    // entire [site] table is missing — once it exists, partial fields
    // fall through to Option::default() = None, not SiteConfig::default().
    // Caused empty <title>, og:title, og:description in production.
    #[test]
    fn partial_site_toml_inherits_defaults_for_missing_fields() {
        let toml_str = r#"og_image = "https://netray.info/og/landing.png""#;
        let s: SiteConfig = toml::from_str(toml_str).expect("partial [site] should deserialize");

        assert_eq!(
            s.og_image.as_deref(),
            Some("https://netray.info/og/landing.png"),
            "explicit override wins"
        );
        assert!(s.title.is_some(), "title must inherit default");
        assert!(s.description.is_some(), "description must inherit default");
        assert!(
            s.og_site_name.is_some(),
            "og_site_name must inherit default"
        );
        assert!(s.brand_name.is_some(), "brand_name must inherit default");
        assert!(
            s.hero_heading.is_some(),
            "hero_heading must inherit default"
        );
        assert!(
            s.hero_subheading.is_some(),
            "hero_subheading must inherit default"
        );
        assert!(
            s.example_domains.is_some(),
            "example_domains must inherit default"
        );
        assert!(s.trust_strip.is_some(), "trust_strip must inherit default");
    }

    // --- Unknown keys are load errors; shipped configs load ---

    fn load_toml(contents: &str) -> Result<Config, ConfigError> {
        let mut file = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        std::io::Write::write_all(&mut file, contents.as_bytes()).unwrap();
        Config::load(Some(file.path().to_str().unwrap()))
    }

    #[test]
    fn unknown_top_level_section_is_rejected() {
        let err = load_toml("[serverr]\nbind = \"0.0.0.0:8082\"\n").unwrap_err();
        assert!(err.to_string().contains("serverr"), "got: {err}");
    }

    #[test]
    fn unknown_key_in_section_is_rejected() {
        let err = load_toml("[rate_limit]\nper_ip_per_minut = 10\n").unwrap_err();
        assert!(err.to_string().contains("per_ip_per_minut"), "got: {err}");
    }

    #[test]
    fn unread_backend_keys_are_rejected() {
        for key in [
            "max_concurrent = 10",
            "cache_ttl_secs = 300",
            "cache_capacity = 1024",
        ] {
            let toml = format!("[backends.ip]\nurl = \"http://ip.example.com\"\n{key}\n");
            assert!(load_toml(&toml).is_err(), "{key} must be rejected");
        }
    }

    #[test]
    fn unknown_key_in_footer_link_is_rejected() {
        let toml =
            "[site]\nfooter_links = [{ label = \"a\", href = \"/\", external = false, x = 1 }]\n";
        assert!(load_toml(toml).is_err());
    }

    #[test]
    fn non_config_env_vars_are_not_config_keys() {
        let vars = [
            ("LENS_CONFIG", "lens.toml"),
            ("LENS_LIVE_TESTS", "1"),
            ("LENS_SERVER__BIND", "0.0.0.0:8082"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let cfg = Config::load_with_env(None, vars).unwrap();
        assert_eq!(
            cfg.server.bind,
            "0.0.0.0:8082".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn repo_config_files_load() {
        for name in [
            "lens.example.toml",
            "lens.dev.toml",
            "tests/fixtures/lens.production.toml",
        ] {
            let path = format!("{}/{name}", env!("CARGO_MANIFEST_DIR"));
            if let Err(e) = Config::load(Some(&path)) {
                panic!("{name} must load: {e}");
            }
        }
    }

    #[test]
    fn production_fixture_values() {
        let path = format!(
            "{}/tests/fixtures/lens.production.toml",
            env!("CARGO_MANIFEST_DIR")
        );
        let cfg = Config::load(Some(&path)).unwrap();
        assert_eq!(cfg.server.trusted_proxies.len(), 2);
        assert_eq!(cfg.backends.ip.timeout_ms, 2000);
        assert_eq!(
            cfg.backends.email.as_ref().and_then(|e| e.url.as_deref()),
            Some("http://beacon:8084")
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
