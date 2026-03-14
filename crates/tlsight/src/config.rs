use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

// Hard caps (SDD §8.1) — configuration values are clamped to these maximums.
const HARD_CAP_HANDSHAKE_TIMEOUT: u64 = 5;
const HARD_CAP_REQUEST_TIMEOUT: u64 = 15;
const HARD_CAP_MAX_PORTS: usize = 7;
const HARD_CAP_MAX_IPS: usize = 10;
const HARD_CAP_ENRICHMENT_TIMEOUT_MS: u64 = 2000;
const HARD_CAP_HTTP_CHECK_TIMEOUT: u64 = 5;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Display name shown in the UI. Defaults to "tlsight".
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_limits")]
    pub limits: LimitsConfig,
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
    #[serde(default = "default_validation")]
    pub validation: ValidationConfig,
    #[serde(default)]
    pub ecosystem: EcosystemConfig,
    #[serde(default = "default_quality")]
    pub quality: QualityConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
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
pub struct LimitsConfig {
    #[serde(default = "default_per_ip_per_minute")]
    pub per_ip_per_minute: u32,
    #[serde(default = "default_per_ip_burst")]
    pub per_ip_burst: u32,
    #[serde(default = "default_per_target_per_minute")]
    pub per_target_per_minute: u32,
    #[serde(default = "default_per_target_burst")]
    pub per_target_burst: u32,
    #[serde(default = "default_max_concurrent_connections")]
    pub max_concurrent_connections: usize,
    #[serde(default = "default_max_concurrent_handshakes")]
    pub max_concurrent_handshakes: usize,
    #[serde(default = "default_handshake_timeout_secs")]
    pub handshake_timeout_secs: u64,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_max_ports")]
    pub max_ports: usize,
    #[serde(default = "default_max_ips_per_hostname")]
    pub max_ips_per_hostname: usize,
    #[serde(default = "default_max_domain_length")]
    pub max_domain_length: usize,
    /// Allow inspection of blocked IP ranges (loopback, private, etc.).
    /// **Development only** — must never be enabled in production.
    #[serde(default)]
    pub allow_blocked_targets: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_dns_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ValidationConfig {
    #[serde(default = "default_expiry_warning_days")]
    pub expiry_warning_days: u32,
    #[serde(default = "default_expiry_critical_days")]
    pub expiry_critical_days: u32,
    #[serde(default = "default_true")]
    pub check_dane: bool,
    #[serde(default = "default_true")]
    pub check_caa: bool,
    #[serde(default)]
    pub check_ct: bool,
    #[serde(default)]
    pub custom_ca_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EcosystemConfig {
    pub dns_base_url: Option<String>,
    pub ip_base_url: Option<String>,
    /// Base URL for the ifconfig-rs IP enrichment API (e.g. `https://ip.netray.info`).
    /// When set, enrichment lookups run concurrently with TLS handshakes.
    pub ip_api_url: Option<String>,
    /// Timeout for each enrichment HTTP call in milliseconds.
    #[serde(default = "default_enrichment_timeout_ms")]
    pub enrichment_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QualityConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_http_check_timeout_secs")]
    pub http_check_timeout_secs: u64,
    #[serde(default)]
    pub skip_http_checks: bool,
}

impl Default for QualityConfig {
    fn default() -> Self {
        default_quality()
    }
}

impl Default for EcosystemConfig {
    fn default() -> Self {
        Self {
            dns_base_url: None,
            ip_base_url: None,
            ip_api_url: None,
            enrichment_timeout_ms: default_enrichment_timeout_ms(),
        }
    }
}

impl EcosystemConfig {
    pub fn enrichment_enabled(&self) -> bool {
        self.ip_api_url.is_some()
    }
}

fn default_enrichment_timeout_ms() -> u64 {
    500
}

fn default_quality() -> QualityConfig {
    QualityConfig {
        enabled: true,
        http_check_timeout_secs: default_http_check_timeout_secs(),
        skip_http_checks: false,
    }
}

fn default_http_check_timeout_secs() -> u64 {
    5
}

/// Log output format.
///
/// Configurable via `telemetry.log_format` in the TOML config or
/// `TLSIGHT_TELEMETRY__LOG_FORMAT=json` environment variable.
///
/// - `text` (default): human-readable, colour-coded output for local development.
/// - `json`: structured JSON lines for log aggregators (Loki, CloudWatch, Datadog).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    #[default]
    Text,
    Json,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_otlp_endpoint")]
    pub otlp_endpoint: String,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
    #[serde(default)]
    pub log_format: LogFormat,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            otlp_endpoint: default_otlp_endpoint(),
            service_name: default_service_name(),
            sample_rate: default_sample_rate(),
            log_format: LogFormat::default(),
        }
    }
}

fn default_otlp_endpoint() -> String {
    "http://localhost:4318".to_owned()
}
fn default_service_name() -> String {
    "tlsight".to_owned()
}
fn default_sample_rate() -> f64 {
    1.0
}

// --- Default value functions ---

fn default_site_name() -> String {
    "tlsight".to_string()
}

fn default_server() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        metrics_bind: default_metrics_bind(),
        trusted_proxies: Vec::new(),
    }
}

fn default_limits() -> LimitsConfig {
    LimitsConfig {
        per_ip_per_minute: default_per_ip_per_minute(),
        per_ip_burst: default_per_ip_burst(),
        per_target_per_minute: default_per_target_per_minute(),
        per_target_burst: default_per_target_burst(),
        max_concurrent_connections: default_max_concurrent_connections(),
        max_concurrent_handshakes: default_max_concurrent_handshakes(),
        handshake_timeout_secs: default_handshake_timeout_secs(),
        request_timeout_secs: default_request_timeout_secs(),
        max_ports: default_max_ports(),
        max_ips_per_hostname: default_max_ips_per_hostname(),
        max_domain_length: default_max_domain_length(),
        allow_blocked_targets: false,
    }
}

fn default_dns() -> DnsConfig {
    DnsConfig {
        timeout_secs: default_dns_timeout_secs(),
    }
}

fn default_validation() -> ValidationConfig {
    ValidationConfig {
        expiry_warning_days: default_expiry_warning_days(),
        expiry_critical_days: default_expiry_critical_days(),
        check_dane: true,
        check_caa: true,
        check_ct: false,
        custom_ca_dir: None,
    }
}

fn default_bind() -> SocketAddr {
    ([127, 0, 0, 1], 8080).into()
}

fn default_metrics_bind() -> SocketAddr {
    ([127, 0, 0, 1], 9090).into()
}

fn default_per_ip_per_minute() -> u32 {
    30
}

fn default_per_ip_burst() -> u32 {
    10
}

fn default_per_target_per_minute() -> u32 {
    60
}

fn default_per_target_burst() -> u32 {
    20
}

fn default_max_concurrent_connections() -> usize {
    256
}

fn default_max_concurrent_handshakes() -> usize {
    10
}

fn default_handshake_timeout_secs() -> u64 {
    5
}

fn default_request_timeout_secs() -> u64 {
    15
}

fn default_max_ports() -> usize {
    7
}

fn default_max_ips_per_hostname() -> usize {
    10
}

fn default_max_domain_length() -> usize {
    253
}

fn default_dns_timeout_secs() -> u64 {
    3
}

fn default_expiry_warning_days() -> u32 {
    30
}

fn default_expiry_critical_days() -> u32 {
    14
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from an optional TOML file path and environment variables.
    ///
    /// Precedence (highest first): env vars (TLSIGHT_ prefix) > TOML file > built-in defaults.
    pub fn load(config_path: Option<&str>) -> Result<Self, ConfigError> {
        let mut builder = config::Config::builder();

        // Layer 1: optional TOML file.
        if let Some(path) = config_path {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }

        // Layer 2: environment variables with TLSIGHT_ prefix and __ section separator.
        // e.g. TLSIGHT_LIMITS__PER_IP_PER_MINUTE=60 maps to limits.per_ip_per_minute.
        builder = builder.add_source(
            config::Environment::with_prefix("TLSIGHT")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        let raw = builder.build()?;
        let mut cfg: Config = raw.try_deserialize()?;
        cfg.validate()?;

        Ok(cfg)
    }

    /// Validate and clamp configuration values to hard caps.
    ///
    /// - Values exceeding hard caps are clamped with a tracing warning.
    /// - Zero values for rate limits, connections, and query limits are rejected.
    pub fn validate(&mut self) -> Result<(), ConfigError> {
        // Clamp to hard caps (SDD §8.1).
        if self.limits.handshake_timeout_secs > HARD_CAP_HANDSHAKE_TIMEOUT {
            tracing::warn!(
                configured = self.limits.handshake_timeout_secs,
                clamped = HARD_CAP_HANDSHAKE_TIMEOUT,
                "handshake_timeout_secs exceeds hard cap, clamping"
            );
            self.limits.handshake_timeout_secs = HARD_CAP_HANDSHAKE_TIMEOUT;
        }

        if self.limits.request_timeout_secs > HARD_CAP_REQUEST_TIMEOUT {
            tracing::warn!(
                configured = self.limits.request_timeout_secs,
                clamped = HARD_CAP_REQUEST_TIMEOUT,
                "request_timeout_secs exceeds hard cap, clamping"
            );
            self.limits.request_timeout_secs = HARD_CAP_REQUEST_TIMEOUT;
        }

        if self.limits.max_ports > HARD_CAP_MAX_PORTS {
            tracing::warn!(
                configured = self.limits.max_ports,
                clamped = HARD_CAP_MAX_PORTS,
                "max_ports exceeds hard cap, clamping"
            );
            self.limits.max_ports = HARD_CAP_MAX_PORTS;
        }

        if self.limits.max_ips_per_hostname > HARD_CAP_MAX_IPS {
            tracing::warn!(
                configured = self.limits.max_ips_per_hostname,
                clamped = HARD_CAP_MAX_IPS,
                "max_ips_per_hostname exceeds hard cap, clamping"
            );
            self.limits.max_ips_per_hostname = HARD_CAP_MAX_IPS;
        }

        // Reject zero values — these would disable protections or cause division-by-zero.
        reject_zero("per_ip_per_minute", self.limits.per_ip_per_minute)?;
        reject_zero("per_ip_burst", self.limits.per_ip_burst)?;
        reject_zero("per_target_per_minute", self.limits.per_target_per_minute)?;
        reject_zero("per_target_burst", self.limits.per_target_burst)?;
        reject_zero(
            "max_concurrent_connections",
            self.limits.max_concurrent_connections,
        )?;
        reject_zero(
            "max_concurrent_handshakes",
            self.limits.max_concurrent_handshakes,
        )?;
        reject_zero("handshake_timeout_secs", self.limits.handshake_timeout_secs)?;
        reject_zero("request_timeout_secs", self.limits.request_timeout_secs)?;
        reject_zero("max_ports", self.limits.max_ports)?;
        reject_zero("max_ips_per_hostname", self.limits.max_ips_per_hostname)?;
        reject_zero("max_domain_length", self.limits.max_domain_length)?;
        reject_zero("dns.timeout_secs", self.dns.timeout_secs)?;
        reject_zero(
            "validation.expiry_warning_days",
            self.validation.expiry_warning_days,
        )?;
        reject_zero(
            "validation.expiry_critical_days",
            self.validation.expiry_critical_days,
        )?;

        // Enrichment timeout: clamp to hard cap, reject zero when enabled.
        if self.ecosystem.enrichment_timeout_ms > HARD_CAP_ENRICHMENT_TIMEOUT_MS {
            tracing::warn!(
                configured = self.ecosystem.enrichment_timeout_ms,
                clamped = HARD_CAP_ENRICHMENT_TIMEOUT_MS,
                "enrichment_timeout_ms exceeds hard cap, clamping"
            );
            self.ecosystem.enrichment_timeout_ms = HARD_CAP_ENRICHMENT_TIMEOUT_MS;
        }
        if self.ecosystem.enrichment_enabled() {
            reject_zero(
                "ecosystem.enrichment_timeout_ms",
                self.ecosystem.enrichment_timeout_ms,
            )?;
        }

        // Quality assessment HTTP check timeout: clamp to hard cap.
        if self.quality.http_check_timeout_secs > HARD_CAP_HTTP_CHECK_TIMEOUT {
            tracing::warn!(
                configured = self.quality.http_check_timeout_secs,
                clamped = HARD_CAP_HTTP_CHECK_TIMEOUT,
                "quality.http_check_timeout_secs exceeds hard cap, clamping"
            );
            self.quality.http_check_timeout_secs = HARD_CAP_HTTP_CHECK_TIMEOUT;
        }

        // Telemetry config validation.
        if self.telemetry.enabled && !(0.0..=1.0).contains(&self.telemetry.sample_rate) {
            return Err(ConfigError::Message(
                "invalid configuration: telemetry.sample_rate must be in [0.0, 1.0]".to_owned(),
            ));
        }

        Ok(())
    }
}

/// Reject a zero value for a named configuration field.
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
            site_name: default_site_name(),
            server: default_server(),
            limits: default_limits(),
            dns: default_dns(),
            validation: default_validation(),
            ecosystem: EcosystemConfig::default(),
            quality: default_quality(),
            telemetry: TelemetryConfig::default(),
        }
    }

    // --- Valid defaults ---

    #[test]
    fn default_config_passes_validation() {
        let mut cfg = valid_config();
        assert!(cfg.validate().is_ok());
    }

    // --- Hard-cap clamping ---

    #[test]
    fn clamps_handshake_timeout_secs() {
        let mut cfg = valid_config();
        cfg.limits.handshake_timeout_secs = HARD_CAP_HANDSHAKE_TIMEOUT + 99;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.limits.handshake_timeout_secs,
            HARD_CAP_HANDSHAKE_TIMEOUT
        );
    }

    #[test]
    fn clamps_request_timeout_secs() {
        let mut cfg = valid_config();
        cfg.limits.request_timeout_secs = HARD_CAP_REQUEST_TIMEOUT + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.request_timeout_secs, HARD_CAP_REQUEST_TIMEOUT);
    }

    #[test]
    fn clamps_max_ports() {
        let mut cfg = valid_config();
        cfg.limits.max_ports = HARD_CAP_MAX_PORTS + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_ports, HARD_CAP_MAX_PORTS);
    }

    #[test]
    fn clamps_max_ips_per_hostname() {
        let mut cfg = valid_config();
        cfg.limits.max_ips_per_hostname = HARD_CAP_MAX_IPS + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_ips_per_hostname, HARD_CAP_MAX_IPS);
    }

    // Values at exactly the hard cap should not be clamped.
    #[test]
    fn hard_cap_exact_value_is_accepted() {
        let mut cfg = valid_config();
        cfg.limits.handshake_timeout_secs = HARD_CAP_HANDSHAKE_TIMEOUT;
        cfg.limits.request_timeout_secs = HARD_CAP_REQUEST_TIMEOUT;
        cfg.limits.max_ports = HARD_CAP_MAX_PORTS;
        cfg.limits.max_ips_per_hostname = HARD_CAP_MAX_IPS;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.limits.handshake_timeout_secs,
            HARD_CAP_HANDSHAKE_TIMEOUT
        );
        assert_eq!(cfg.limits.request_timeout_secs, HARD_CAP_REQUEST_TIMEOUT);
        assert_eq!(cfg.limits.max_ports, HARD_CAP_MAX_PORTS);
        assert_eq!(cfg.limits.max_ips_per_hostname, HARD_CAP_MAX_IPS);
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
        c.limits.per_ip_per_minute = 0
    });
    zero_rejects!(rejects_zero_per_ip_burst, |c: &mut Config| {
        c.limits.per_ip_burst = 0
    });
    zero_rejects!(rejects_zero_per_target_per_minute, |c: &mut Config| {
        c.limits.per_target_per_minute = 0
    });
    zero_rejects!(rejects_zero_per_target_burst, |c: &mut Config| {
        c.limits.per_target_burst = 0
    });
    zero_rejects!(rejects_zero_max_concurrent_connections, |c: &mut Config| {
        c.limits.max_concurrent_connections = 0
    });
    zero_rejects!(rejects_zero_max_concurrent_handshakes, |c: &mut Config| {
        c.limits.max_concurrent_handshakes = 0
    });
    zero_rejects!(rejects_zero_handshake_timeout_secs, |c: &mut Config| {
        c.limits.handshake_timeout_secs = 0
    });
    zero_rejects!(rejects_zero_request_timeout_secs, |c: &mut Config| {
        c.limits.request_timeout_secs = 0
    });
    zero_rejects!(rejects_zero_max_ports, |c: &mut Config| {
        c.limits.max_ports = 0
    });
    zero_rejects!(rejects_zero_max_ips_per_hostname, |c: &mut Config| {
        c.limits.max_ips_per_hostname = 0
    });
    zero_rejects!(rejects_zero_max_domain_length, |c: &mut Config| {
        c.limits.max_domain_length = 0
    });
    zero_rejects!(rejects_zero_dns_timeout_secs, |c: &mut Config| {
        c.dns.timeout_secs = 0
    });
    zero_rejects!(rejects_zero_expiry_warning_days, |c: &mut Config| {
        c.validation.expiry_warning_days = 0
    });
    zero_rejects!(rejects_zero_expiry_critical_days, |c: &mut Config| {
        c.validation.expiry_critical_days = 0
    });

    // --- Enrichment config ---

    #[test]
    fn clamps_enrichment_timeout_ms() {
        let mut cfg = valid_config();
        cfg.ecosystem.ip_api_url = Some("https://ip.netray.info".to_owned());
        cfg.ecosystem.enrichment_timeout_ms = 9999;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.ecosystem.enrichment_timeout_ms,
            HARD_CAP_ENRICHMENT_TIMEOUT_MS
        );
    }

    #[test]
    fn rejects_zero_enrichment_timeout_when_enabled() {
        let mut cfg = valid_config();
        cfg.ecosystem.ip_api_url = Some("https://ip.netray.info".to_owned());
        cfg.ecosystem.enrichment_timeout_ms = 0;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(
            err.contains("must not be zero"),
            "expected 'must not be zero' in: {err}"
        );
    }

    #[test]
    fn allows_zero_enrichment_timeout_when_disabled() {
        let mut cfg = valid_config();
        cfg.ecosystem.ip_api_url = None;
        cfg.ecosystem.enrichment_timeout_ms = 0;
        assert!(cfg.validate().is_ok());
    }

    // --- Quality config ---

    #[test]
    fn quality_defaults_are_valid() {
        let cfg = valid_config();
        assert!(cfg.quality.enabled);
        assert_eq!(cfg.quality.http_check_timeout_secs, 5);
        assert!(!cfg.quality.skip_http_checks);
    }

    #[test]
    fn clamps_quality_http_check_timeout() {
        let mut cfg = valid_config();
        cfg.quality.http_check_timeout_secs = 99;
        cfg.validate().unwrap();
        assert_eq!(
            cfg.quality.http_check_timeout_secs,
            HARD_CAP_HTTP_CHECK_TIMEOUT
        );
    }
}
