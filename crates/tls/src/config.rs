use serde::Deserialize;

pub use config::ConfigError;

// Hard caps (SDD §8.1) — configuration values are clamped to these maximums.
const HARD_CAP_HANDSHAKE_TIMEOUT: u64 = 5;
const HARD_CAP_REQUEST_TIMEOUT: u64 = 15;
const HARD_CAP_MAX_PORTS: usize = 7;
const HARD_CAP_MAX_IPS: usize = 10;
const HARD_CAP_HTTP_CHECK_TIMEOUT: u64 = 5;

/// The inspection's runtime config: the `[modules.tls]` sections, validated and clamped.
#[derive(Debug, Clone)]
pub struct Config {
    pub limits: LimitsConfig,
    pub dns: DnsConfig,
    pub validation: ValidationConfig,
    pub quality: QualityConfig,
}

/// The check sections the TLS module reads from a `[modules.tls]` table; tlsight's key names
/// and defaults.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    #[serde(default)]
    pub limits: ModuleLimits,
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
    #[serde(default = "default_validation")]
    pub validation: ValidationConfig,
    #[serde(default = "default_quality")]
    pub quality: QualityConfig,
}

/// The subset of `[limits]` that applies to the module: no per-client limit and no
/// `allow_blocked_targets`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLimits {
    #[serde(default = "default_handshake_timeout_secs")]
    pub handshake_timeout_secs: u64,
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_max_ports")]
    pub max_ports: usize,
    #[serde(default = "default_max_concurrent_handshakes")]
    pub max_concurrent_handshakes: usize,
    #[serde(default = "default_max_ips_per_hostname")]
    pub max_ips_per_hostname: usize,
    #[serde(default = "default_per_target_per_minute")]
    pub per_target_per_minute: u32,
    #[serde(default = "default_per_target_burst")]
    pub per_target_burst: u32,
    /// Handshakes one lens check may spend; 0.23.x got this from tlsight's `per_ip_burst`.
    #[serde(default = "default_check_budget")]
    pub check_budget: u32,
}

impl Default for ModuleLimits {
    fn default() -> Self {
        Self {
            handshake_timeout_secs: default_handshake_timeout_secs(),
            request_timeout_secs: default_request_timeout_secs(),
            max_ports: default_max_ports(),
            max_concurrent_handshakes: default_max_concurrent_handshakes(),
            max_ips_per_hostname: default_max_ips_per_hostname(),
            per_target_per_minute: default_per_target_per_minute(),
            per_target_burst: default_per_target_burst(),
            check_budget: default_check_budget(),
        }
    }
}

impl ModuleConfig {
    /// The runtime `Config` these sections stand for, validated and clamped.
    pub fn into_config(self) -> Result<Config, ConfigError> {
        let l = self.limits;
        reject_zero("check_budget", l.check_budget)?;
        let mut cfg = Config {
            limits: LimitsConfig {
                per_target_per_minute: l.per_target_per_minute,
                per_target_burst: l.per_target_burst,
                max_concurrent_handshakes: l.max_concurrent_handshakes,
                handshake_timeout_secs: l.handshake_timeout_secs,
                request_timeout_secs: l.request_timeout_secs,
                max_ports: l.max_ports,
                max_ips_per_hostname: l.max_ips_per_hostname,
                allow_blocked_targets: false,
            },
            dns: self.dns,
            validation: self.validation,
            quality: self.quality,
        };
        cfg.validate()?;
        cfg.check_startup()?;
        Ok(cfg)
    }
}

#[derive(Debug, Clone)]
pub struct LimitsConfig {
    pub per_target_per_minute: u32,
    pub per_target_burst: u32,
    pub max_concurrent_handshakes: usize,
    pub handshake_timeout_secs: u64,
    pub request_timeout_secs: u64,
    pub max_ports: usize,
    pub max_ips_per_hostname: usize,
    /// Allow inspection of blocked IP ranges (loopback, private, etc.); tests only.
    pub allow_blocked_targets: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DnsConfig {
    #[serde(default = "default_dns_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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

#[cfg(test)]
fn default_limits() -> LimitsConfig {
    LimitsConfig {
        per_target_per_minute: default_per_target_per_minute(),
        per_target_burst: default_per_target_burst(),
        max_concurrent_handshakes: default_max_concurrent_handshakes(),
        handshake_timeout_secs: default_handshake_timeout_secs(),
        request_timeout_secs: default_request_timeout_secs(),
        max_ports: default_max_ports(),
        max_ips_per_hostname: default_max_ips_per_hostname(),
        allow_blocked_targets: false,
    }
}

fn default_check_budget() -> u32 {
    10
}

fn default_per_target_per_minute() -> u32 {
    60
}

fn default_per_target_burst() -> u32 {
    20
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
    /// Refuses a `validation.custom_ca_dir` that is not a directory.
    pub fn check_startup(&self) -> Result<(), ConfigError> {
        if let Some(dir) = &self.validation.custom_ca_dir
            && !std::path::Path::new(dir).is_dir()
        {
            return Err(ConfigError::Message(format!(
                "invalid configuration: validation.custom_ca_dir does not exist or is not a directory: {dir}"
            )));
        }
        Ok(())
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
        reject_zero("per_target_per_minute", self.limits.per_target_per_minute)?;
        reject_zero("per_target_burst", self.limits.per_target_burst)?;
        reject_zero(
            "max_concurrent_handshakes",
            self.limits.max_concurrent_handshakes,
        )?;
        reject_zero("handshake_timeout_secs", self.limits.handshake_timeout_secs)?;
        reject_zero("request_timeout_secs", self.limits.request_timeout_secs)?;
        reject_zero("max_ports", self.limits.max_ports)?;
        reject_zero("max_ips_per_hostname", self.limits.max_ips_per_hostname)?;
        reject_zero("dns.timeout_secs", self.dns.timeout_secs)?;
        reject_zero(
            "validation.expiry_warning_days",
            self.validation.expiry_warning_days,
        )?;
        reject_zero(
            "validation.expiry_critical_days",
            self.validation.expiry_critical_days,
        )?;

        // Quality assessment HTTP check timeout: clamp to hard cap.
        if self.quality.http_check_timeout_secs > HARD_CAP_HTTP_CHECK_TIMEOUT {
            tracing::warn!(
                configured = self.quality.http_check_timeout_secs,
                clamped = HARD_CAP_HTTP_CHECK_TIMEOUT,
                "quality.http_check_timeout_secs exceeds hard cap, clamping"
            );
            self.quality.http_check_timeout_secs = HARD_CAP_HTTP_CHECK_TIMEOUT;
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
            limits: default_limits(),
            dns: default_dns(),
            validation: default_validation(),
            quality: default_quality(),
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

    zero_rejects!(rejects_zero_per_target_per_minute, |c: &mut Config| {
        c.limits.per_target_per_minute = 0
    });
    zero_rejects!(rejects_zero_per_target_burst, |c: &mut Config| {
        c.limits.per_target_burst = 0
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
    zero_rejects!(rejects_zero_dns_timeout_secs, |c: &mut Config| {
        c.dns.timeout_secs = 0
    });
    zero_rejects!(rejects_zero_expiry_warning_days, |c: &mut Config| {
        c.validation.expiry_warning_days = 0
    });
    zero_rejects!(rejects_zero_expiry_critical_days, |c: &mut Config| {
        c.validation.expiry_critical_days = 0
    });

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
