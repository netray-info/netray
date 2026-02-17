use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

// Hard caps (§8.1) — configuration values are clamped to these maximums.
const HARD_CAP_TIMEOUT_SECS: u64 = 10;
const HARD_CAP_RECORD_TYPES: usize = 10;
const HARD_CAP_SERVERS: usize = 4;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_limits")]
    pub limits: LimitsConfig,
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
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
    #[serde(default = "default_global_per_minute")]
    pub global_per_minute: u32,
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

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_servers_list")]
    pub default_servers: Vec<String>,
    #[serde(default = "default_true")]
    pub allow_system_resolvers: bool,
    #[serde(default)]
    pub allow_arbitrary_servers: bool,
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
        global_per_minute: default_global_per_minute(),
        max_concurrent_connections: default_max_concurrent(),
        per_ip_max_streams: default_per_ip_max_streams(),
        max_timeout_secs: default_max_timeout(),
        max_record_types: default_max_record_types(),
        max_servers: default_max_servers(),
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
    30
}

fn default_per_ip_burst() -> u32 {
    10
}

fn default_per_target_per_minute() -> u32 {
    30
}

fn default_global_per_minute() -> u32 {
    500
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
    vec!["cloudflare".to_owned()]
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

    /// Validate and clamp configuration values to hard caps.
    ///
    /// - Values exceeding hard caps are clamped with a tracing warning.
    /// - Zero values for rate limits, connections, and query limits are rejected.
    fn validate(&mut self) -> Result<(), ConfigError> {
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

        // Reject zero values — these would disable protections or cause division-by-zero.
        reject_zero("per_ip_per_minute", self.limits.per_ip_per_minute)?;
        reject_zero("per_ip_burst", self.limits.per_ip_burst)?;
        reject_zero("per_target_per_minute", self.limits.per_target_per_minute)?;
        reject_zero("global_per_minute", self.limits.global_per_minute)?;
        reject_zero(
            "max_concurrent_connections",
            self.limits.max_concurrent_connections,
        )?;
        reject_zero("per_ip_max_streams", self.limits.per_ip_max_streams)?;
        reject_zero("max_timeout_secs", self.limits.max_timeout_secs)?;
        reject_zero("max_record_types", self.limits.max_record_types)?;
        reject_zero("max_servers", self.limits.max_servers)?;

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
