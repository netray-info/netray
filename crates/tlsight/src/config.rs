use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

// Hard caps (SDD §8.1) — configuration values are clamped to these maximums.
const HARD_CAP_HANDSHAKE_TIMEOUT: u64 = 5;
const HARD_CAP_REQUEST_TIMEOUT: u64 = 15;
const HARD_CAP_MAX_PORTS: usize = 5;
const HARD_CAP_MAX_IPS: usize = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    #[allow(dead_code)] // Used by config deserialization; Phase 2 will use it
    #[serde(default = "default_resolver")]
    pub resolver: String,
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
    #[allow(dead_code)] // Used by state.rs TODO for custom CA loading
    #[serde(default)]
    pub custom_ca_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EcosystemConfig {
    pub dns_base_url: Option<String>,
    pub ip_base_url: Option<String>,
}

// --- Default value functions ---

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
    }
}

fn default_dns() -> DnsConfig {
    DnsConfig {
        resolver: default_resolver(),
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
    5
}

fn default_max_ips_per_hostname() -> usize {
    10
}

fn default_max_domain_length() -> usize {
    253
}

fn default_resolver() -> String {
    "cloudflare".to_owned()
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
    fn validate(&mut self) -> Result<(), ConfigError> {
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
            server: default_server(),
            limits: default_limits(),
            dns: default_dns(),
            validation: default_validation(),
            ecosystem: EcosystemConfig::default(),
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
}
