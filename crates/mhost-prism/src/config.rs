use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

// Hard caps (§8.1) — configuration values are clamped to these maximums.
const HARD_CAP_TIMEOUT_SECS: u64 = 10;
const HARD_CAP_RECORD_TYPES: usize = 10;
const HARD_CAP_SERVERS: usize = 4;
const HARD_CAP_TRACE_HOPS: u32 = 20;
const HARD_CAP_TRACE_QUERY_TIMEOUT: u64 = 10;

// parser::MAX_RECORD_TYPES and HARD_CAP_RECORD_TYPES must stay in sync:
// the config clamps user input to HARD_CAP_RECORD_TYPES, and the parser
// enforces MAX_RECORD_TYPES independently. If they diverge, one limit becomes
// unreachable or bypassable.
const _: () = assert!(
    crate::parser::MAX_RECORD_TYPES == HARD_CAP_RECORD_TYPES,
    "parser::MAX_RECORD_TYPES and config::HARD_CAP_RECORD_TYPES must be equal"
);

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    /// Display name shown in the UI. Defaults to "prism".
    #[serde(default = "default_site_name")]
    pub site_name: String,
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_limits")]
    pub limits: LimitsConfig,
    #[serde(default = "default_circuit_breaker")]
    pub circuit_breaker: CircuitBreakerConfig,
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
    #[serde(default = "default_trace")]
    pub trace: TraceConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    #[serde(default)]
    pub ecosystem: EcosystemConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LimitsConfig {
    #[serde(default = "default_per_ip_per_minute")]
    pub per_ip_per_minute: u32,
    #[serde(default = "default_per_ip_burst")]
    pub per_ip_burst: u32,
    #[serde(default = "default_per_target_per_minute")]
    pub per_target_per_minute: u32,
    #[serde(default = "default_per_target_burst")]
    pub per_target_burst: u32,
    #[serde(default = "default_global_per_minute")]
    pub global_per_minute: u32,
    #[serde(default = "default_global_burst")]
    pub global_burst: u32,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_connections: usize,
    #[serde(default = "default_per_ip_max_streams")]
    pub per_ip_max_streams: u32,
    #[serde(default = "default_max_timeout")]
    pub max_timeout_secs: u64,
    #[serde(default = "default_max_record_types")]
    pub max_record_types: usize,
    #[serde(default = "default_max_servers")]
    pub max_servers: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CircuitBreakerConfig {
    /// Sliding window length for error-rate tracking.
    #[serde(default = "default_cb_window_secs")]
    pub window_secs: u64,
    /// How long the breaker stays open before allowing a probe request.
    #[serde(default = "default_cb_cooldown_secs")]
    pub cooldown_secs: u64,
    /// Error rate (0.0–1.0, exclusive on both ends) at which the breaker trips.
    #[serde(default = "default_cb_failure_threshold")]
    pub failure_threshold: f64,
    /// Minimum number of requests in the window before the threshold is evaluated.
    #[serde(default = "default_cb_min_requests")]
    pub min_requests: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TraceConfig {
    /// Maximum delegation hops before stopping (hard cap: 20).
    #[serde(default = "default_trace_max_hops")]
    pub max_hops: u32,
    /// Per-query timeout for each raw DNS request in the trace (hard cap: 10s).
    #[serde(default = "default_trace_query_timeout")]
    pub query_timeout_secs: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_servers_list")]
    pub default_servers: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_system_resolvers: bool,
    #[serde(default)]
    pub allow_arbitrary_servers: bool,
}

pub use netray_common::telemetry::TelemetryConfig;

pub use netray_common::ecosystem::EcosystemConfig;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackendsConfig {
    #[serde(default)]
    pub ip: Option<netray_common::backend::BackendConfig>,
}

/// Hot-reloadable subset of the configuration.
///
/// Contains fields that can be swapped at runtime via SIGHUP without
/// restarting the server. Cold fields (bind address, TLS, trusted proxies)
/// are not included.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Fields read by reload.rs on SIGHUP; handler migration pending.
pub struct HotConfig {
    pub limits: LimitsConfig,
    pub circuit_breaker: CircuitBreakerConfig,
    pub dns: DnsConfig,
    pub trace: TraceConfig,
}

// --- Default value functions ---

fn default_site_name() -> String {
    "prism".to_string()
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
        global_per_minute: default_global_per_minute(),
        global_burst: default_global_burst(),
        max_concurrent_connections: default_max_concurrent(),
        per_ip_max_streams: default_per_ip_max_streams(),
        max_timeout_secs: default_max_timeout(),
        max_record_types: default_max_record_types(),
        max_servers: default_max_servers(),
    }
}

fn default_circuit_breaker() -> CircuitBreakerConfig {
    CircuitBreakerConfig {
        window_secs: default_cb_window_secs(),
        cooldown_secs: default_cb_cooldown_secs(),
        failure_threshold: default_cb_failure_threshold(),
        min_requests: default_cb_min_requests(),
    }
}

fn default_trace() -> TraceConfig {
    TraceConfig {
        max_hops: default_trace_max_hops(),
        query_timeout_secs: default_trace_query_timeout(),
    }
}

fn default_dns() -> DnsConfig {
    DnsConfig {
        default_servers: default_servers_list(),
        allow_system_resolvers: true,
        allow_arbitrary_servers: false,
    }
}

fn default_bind() -> SocketAddr {
    ([127, 0, 0, 1], 8080).into()
}

fn default_metrics_bind() -> SocketAddr {
    ([127, 0, 0, 1], 9090).into()
}

fn default_per_ip_per_minute() -> u32 {
    120
}

fn default_per_ip_burst() -> u32 {
    // Must accommodate combined mode costs: check(16) + trace(16) + dnssec(16) = 48,
    // plus a background query (up to 10×4 = 40). Set to 64 for headroom.
    64
}

fn default_per_target_per_minute() -> u32 {
    60
}

fn default_per_target_burst() -> u32 {
    // Must accommodate the largest per-target cost in a single request:
    // - Regular queries: up to max_record_types (10) per target.
    // - Check endpoint: CHECK_TOTAL_STEPS (16) per target.
    // 20 gives headroom above both.
    20
}

fn default_global_per_minute() -> u32 {
    1000
}

fn default_global_burst() -> u32 {
    50
}

fn default_cb_window_secs() -> u64 {
    60
}

fn default_cb_cooldown_secs() -> u64 {
    30
}

fn default_cb_failure_threshold() -> f64 {
    0.5
}

fn default_cb_min_requests() -> u32 {
    5
}

fn default_max_concurrent() -> usize {
    256
}

fn default_per_ip_max_streams() -> u32 {
    10
}

fn default_max_timeout() -> u64 {
    10
}

fn default_max_record_types() -> usize {
    10
}

fn default_max_servers() -> usize {
    4
}

fn default_servers_list() -> Vec<String> {
    vec!["google".to_owned()]
}

fn default_trace_max_hops() -> u32 {
    10
}

fn default_trace_query_timeout() -> u64 {
    3
}

fn default_true() -> bool {
    true
}

impl Config {
    /// Load configuration from an optional TOML file path and environment variables.
    ///
    /// Precedence (highest first): env vars (PRISM_ prefix) > TOML file > built-in defaults.
    pub fn load(config_path: Option<&str>) -> Result<Self, ConfigError> {
        let mut builder = config::Config::builder();

        // Layer 1: optional TOML file.
        if let Some(path) = config_path {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }

        // Layer 2: environment variables with PRISM_ prefix and __ section separator.
        // e.g. PRISM_LIMITS__PER_IP_PER_MINUTE=60 maps to limits.per_ip_per_minute.
        builder = builder.add_source(
            config::Environment::with_prefix("PRISM")
                .prefix_separator("_")
                .separator("__")
                .try_parsing(true),
        );

        let raw = builder.build()?;
        let mut cfg: Config = raw.try_deserialize()?;
        cfg.validate()?;

        Ok(cfg)
    }

    /// Extract the hot-reloadable subset of the configuration.
    pub fn hot(&self) -> HotConfig {
        HotConfig {
            limits: self.limits.clone(),
            circuit_breaker: self.circuit_breaker.clone(),
            dns: self.dns.clone(),
            trace: self.trace.clone(),
        }
    }

    /// Validate only the hot-reloadable fields. Used during SIGHUP reload.
    pub fn validate_hot(&mut self) -> Result<(), ConfigError> {
        self.validate_limits_and_trace()
    }

    /// Validate and clamp configuration values to hard caps.
    ///
    /// - Values exceeding hard caps are clamped with a tracing warning.
    /// - Zero values for rate limits, connections, and query limits are rejected.
    fn validate(&mut self) -> Result<(), ConfigError> {
        self.validate_limits_and_trace()?;

        // Telemetry config validation.
        if self.telemetry.enabled && !(0.0..=1.0).contains(&self.telemetry.sample_rate) {
            return Err(ConfigError::Message(
                "invalid configuration: telemetry.sample_rate must be in [0.0, 1.0]".to_owned(),
            ));
        }

        Ok(())
    }

    fn validate_limits_and_trace(&mut self) -> Result<(), ConfigError> {
        // Clamp to hard caps (§8.1).
        if self.limits.max_timeout_secs > HARD_CAP_TIMEOUT_SECS {
            tracing::warn!(
                configured = self.limits.max_timeout_secs,
                clamped = HARD_CAP_TIMEOUT_SECS,
                "max_timeout_secs exceeds hard cap, clamping"
            );
            self.limits.max_timeout_secs = HARD_CAP_TIMEOUT_SECS;
        }

        if self.limits.max_record_types > HARD_CAP_RECORD_TYPES {
            tracing::warn!(
                configured = self.limits.max_record_types,
                clamped = HARD_CAP_RECORD_TYPES,
                "max_record_types exceeds hard cap, clamping"
            );
            self.limits.max_record_types = HARD_CAP_RECORD_TYPES;
        }

        if self.limits.max_servers > HARD_CAP_SERVERS {
            tracing::warn!(
                configured = self.limits.max_servers,
                clamped = HARD_CAP_SERVERS,
                "max_servers exceeds hard cap, clamping"
            );
            self.limits.max_servers = HARD_CAP_SERVERS;
        }

        if self.trace.max_hops > HARD_CAP_TRACE_HOPS {
            tracing::warn!(
                configured = self.trace.max_hops,
                clamped = HARD_CAP_TRACE_HOPS,
                "trace.max_hops exceeds hard cap, clamping"
            );
            self.trace.max_hops = HARD_CAP_TRACE_HOPS;
        }

        if self.trace.query_timeout_secs > HARD_CAP_TRACE_QUERY_TIMEOUT {
            tracing::warn!(
                configured = self.trace.query_timeout_secs,
                clamped = HARD_CAP_TRACE_QUERY_TIMEOUT,
                "trace.query_timeout_secs exceeds hard cap, clamping"
            );
            self.trace.query_timeout_secs = HARD_CAP_TRACE_QUERY_TIMEOUT;
        }

        // Reject zero values — these would disable protections or cause division-by-zero.
        reject_zero("per_ip_per_minute", self.limits.per_ip_per_minute)?;
        reject_zero("per_ip_burst", self.limits.per_ip_burst)?;
        reject_zero("per_target_per_minute", self.limits.per_target_per_minute)?;
        reject_zero("per_target_burst", self.limits.per_target_burst)?;
        reject_zero("global_per_minute", self.limits.global_per_minute)?;
        reject_zero("global_burst", self.limits.global_burst)?;
        reject_zero(
            "max_concurrent_connections",
            self.limits.max_concurrent_connections,
        )?;
        reject_zero("per_ip_max_streams", self.limits.per_ip_max_streams)?;
        reject_zero("max_timeout_secs", self.limits.max_timeout_secs)?;
        reject_zero("max_record_types", self.limits.max_record_types)?;
        reject_zero("max_servers", self.limits.max_servers)?;
        reject_zero("trace.max_hops", self.trace.max_hops)?;
        reject_zero("trace.query_timeout_secs", self.trace.query_timeout_secs)?;
        reject_zero(
            "circuit_breaker.window_secs",
            self.circuit_breaker.window_secs,
        )?;
        reject_zero(
            "circuit_breaker.cooldown_secs",
            self.circuit_breaker.cooldown_secs,
        )?;
        reject_zero(
            "circuit_breaker.min_requests",
            self.circuit_breaker.min_requests,
        )?;
        if self.circuit_breaker.failure_threshold <= 0.0
            || self.circuit_breaker.failure_threshold > 1.0
        {
            return Err(ConfigError::Message(
                "invalid configuration: circuit_breaker.failure_threshold must be in (0.0, 1.0]"
                    .to_owned(),
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
            circuit_breaker: default_circuit_breaker(),
            dns: default_dns(),
            trace: default_trace(),
            telemetry: TelemetryConfig::default(),
            ecosystem: EcosystemConfig::default(),
            backends: BackendsConfig::default(),
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
    fn clamps_max_timeout_secs() {
        let mut cfg = valid_config();
        cfg.limits.max_timeout_secs = HARD_CAP_TIMEOUT_SECS + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_timeout_secs, HARD_CAP_TIMEOUT_SECS);
    }

    #[test]
    fn clamps_max_record_types() {
        let mut cfg = valid_config();
        cfg.limits.max_record_types = HARD_CAP_RECORD_TYPES + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_record_types, HARD_CAP_RECORD_TYPES);
    }

    #[test]
    fn clamps_max_servers() {
        let mut cfg = valid_config();
        cfg.limits.max_servers = HARD_CAP_SERVERS + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_servers, HARD_CAP_SERVERS);
    }

    #[test]
    fn clamps_trace_max_hops() {
        let mut cfg = valid_config();
        cfg.trace.max_hops = HARD_CAP_TRACE_HOPS + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.trace.max_hops, HARD_CAP_TRACE_HOPS);
    }

    #[test]
    fn clamps_trace_query_timeout_secs() {
        let mut cfg = valid_config();
        cfg.trace.query_timeout_secs = HARD_CAP_TRACE_QUERY_TIMEOUT + 99;
        cfg.validate().unwrap();
        assert_eq!(cfg.trace.query_timeout_secs, HARD_CAP_TRACE_QUERY_TIMEOUT);
    }

    // Values at exactly the hard cap should not be clamped.
    #[test]
    fn hard_cap_exact_value_is_accepted() {
        let mut cfg = valid_config();
        cfg.limits.max_timeout_secs = HARD_CAP_TIMEOUT_SECS;
        cfg.limits.max_record_types = HARD_CAP_RECORD_TYPES;
        cfg.limits.max_servers = HARD_CAP_SERVERS;
        cfg.trace.max_hops = HARD_CAP_TRACE_HOPS;
        cfg.trace.query_timeout_secs = HARD_CAP_TRACE_QUERY_TIMEOUT;
        cfg.validate().unwrap();
        assert_eq!(cfg.limits.max_timeout_secs, HARD_CAP_TIMEOUT_SECS);
        assert_eq!(cfg.limits.max_record_types, HARD_CAP_RECORD_TYPES);
        assert_eq!(cfg.limits.max_servers, HARD_CAP_SERVERS);
        assert_eq!(cfg.trace.max_hops, HARD_CAP_TRACE_HOPS);
        assert_eq!(cfg.trace.query_timeout_secs, HARD_CAP_TRACE_QUERY_TIMEOUT);
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
    zero_rejects!(rejects_zero_global_per_minute, |c: &mut Config| {
        c.limits.global_per_minute = 0
    });
    zero_rejects!(rejects_zero_global_burst, |c: &mut Config| {
        c.limits.global_burst = 0
    });
    zero_rejects!(rejects_zero_max_concurrent_connections, |c: &mut Config| {
        c.limits.max_concurrent_connections = 0
    });
    zero_rejects!(rejects_zero_per_ip_max_streams, |c: &mut Config| {
        c.limits.per_ip_max_streams = 0
    });
    zero_rejects!(rejects_zero_max_timeout_secs, |c: &mut Config| {
        c.limits.max_timeout_secs = 0
    });
    zero_rejects!(rejects_zero_max_record_types, |c: &mut Config| {
        c.limits.max_record_types = 0
    });
    zero_rejects!(rejects_zero_max_servers, |c: &mut Config| {
        c.limits.max_servers = 0
    });
    zero_rejects!(rejects_zero_trace_max_hops, |c: &mut Config| {
        c.trace.max_hops = 0
    });
    zero_rejects!(rejects_zero_trace_query_timeout_secs, |c: &mut Config| {
        c.trace.query_timeout_secs = 0
    });
    zero_rejects!(rejects_zero_cb_window_secs, |c: &mut Config| {
        c.circuit_breaker.window_secs = 0
    });
    zero_rejects!(rejects_zero_cb_cooldown_secs, |c: &mut Config| {
        c.circuit_breaker.cooldown_secs = 0
    });
    zero_rejects!(rejects_zero_cb_min_requests, |c: &mut Config| {
        c.circuit_breaker.min_requests = 0
    });

    // --- failure_threshold range ---

    #[test]
    fn rejects_failure_threshold_zero() {
        let mut cfg = valid_config();
        cfg.circuit_breaker.failure_threshold = 0.0;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("failure_threshold"), "{err}");
    }

    #[test]
    fn rejects_failure_threshold_below_zero() {
        let mut cfg = valid_config();
        cfg.circuit_breaker.failure_threshold = -0.1;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("failure_threshold"), "{err}");
    }

    #[test]
    fn rejects_failure_threshold_above_one() {
        let mut cfg = valid_config();
        cfg.circuit_breaker.failure_threshold = 1.1;
        let err = cfg.validate().unwrap_err().to_string();
        assert!(err.contains("failure_threshold"), "{err}");
    }

    #[test]
    fn accepts_failure_threshold_exactly_one() {
        let mut cfg = valid_config();
        cfg.circuit_breaker.failure_threshold = 1.0;
        assert!(cfg.validate().is_ok());
    }
}
