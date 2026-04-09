use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_inspect")]
    pub inspect: InspectConfig,
    #[serde(default = "default_limits")]
    pub limits: LimitsConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
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
pub struct InspectConfig {
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_total_timeout_secs")]
    pub total_timeout_secs: u64,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    #[allow(dead_code)] // Referenced by SDD; will be used when body sniffing is implemented
    #[serde(default = "default_body_read_limit_bytes")]
    pub body_read_limit_bytes: usize,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
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
}

#[derive(Debug, Clone, Deserialize)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub ip_url: Option<String>,
    #[serde(default = "default_enrichment_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default)]
    pub log_format: Option<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub otlp_endpoint: Option<String>,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self, ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(p) = path {
            builder = builder.add_source(config::File::with_name(p).required(true));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("SPECTRA")
                .separator("__")
                .try_parsing(true),
        );

        let cfg: Config = builder.build()?.try_deserialize()?;
        Ok(cfg)
    }
}

impl From<&TelemetryConfig> for netray_common::telemetry::TelemetryConfig {
    fn from(tc: &TelemetryConfig) -> Self {
        Self {
            enabled: tc.enabled,
            otlp_endpoint: tc
                .otlp_endpoint
                .clone()
                .unwrap_or_else(|| "http://localhost:4318".to_string()),
            service_name: tc.service_name.clone(),
            sample_rate: tc.sample_rate,
            log_format: match tc.log_format.as_deref() {
                Some("json") => netray_common::telemetry::LogFormat::Json,
                _ => netray_common::telemetry::LogFormat::Text,
            },
        }
    }
}

// --- Defaults ---

fn default_server() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        metrics_bind: default_metrics_bind(),
        trusted_proxies: Vec::new(),
    }
}

fn default_bind() -> SocketAddr {
    ([127, 0, 0, 1], 3000).into()
}

fn default_metrics_bind() -> SocketAddr {
    ([127, 0, 0, 1], 9090).into()
}

fn default_inspect() -> InspectConfig {
    InspectConfig {
        request_timeout_secs: default_request_timeout_secs(),
        total_timeout_secs: default_total_timeout_secs(),
        max_redirects: default_max_redirects(),
        body_read_limit_bytes: default_body_read_limit_bytes(),
        user_agent: default_user_agent(),
    }
}

fn default_request_timeout_secs() -> u64 {
    10
}
fn default_total_timeout_secs() -> u64 {
    30
}
fn default_max_redirects() -> usize {
    10
}
fn default_body_read_limit_bytes() -> usize {
    1024
}
fn default_user_agent() -> String {
    "netray-spectra".to_string()
}

fn default_limits() -> LimitsConfig {
    LimitsConfig {
        per_ip_per_minute: default_per_ip_per_minute(),
        per_ip_burst: default_per_ip_burst(),
        per_target_per_minute: default_per_target_per_minute(),
        per_target_burst: default_per_target_burst(),
        max_concurrent_connections: default_max_concurrent_connections(),
    }
}

fn default_per_ip_per_minute() -> u32 {
    10
}
fn default_per_ip_burst() -> u32 {
    5
}
fn default_per_target_per_minute() -> u32 {
    30
}
fn default_per_target_burst() -> u32 {
    10
}
fn default_max_concurrent_connections() -> usize {
    256
}

fn default_enrichment_timeout_ms() -> u64 {
    500
}

fn default_service_name() -> String {
    "spectra".to_string()
}
fn default_sample_rate() -> f64 {
    1.0
}

impl Default for EnrichmentConfig {
    fn default() -> Self {
        Self {
            ip_url: None,
            timeout_ms: default_enrichment_timeout_ms(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_loads() {
        let cfg = Config::load(None).unwrap();
        assert_eq!(cfg.server.bind, SocketAddr::from(([127, 0, 0, 1], 3000)));
        assert_eq!(cfg.inspect.request_timeout_secs, 10);
        assert_eq!(cfg.inspect.total_timeout_secs, 30);
        assert_eq!(cfg.inspect.max_redirects, 10);
        assert_eq!(cfg.limits.per_ip_per_minute, 10);
    }

    #[test]
    fn telemetry_conversion() {
        let tc = TelemetryConfig {
            log_format: Some("json".to_string()),
            enabled: true,
            otlp_endpoint: Some("http://otel:4318".to_string()),
            service_name: "test".to_string(),
            sample_rate: 0.5,
        };
        let nc: netray_common::telemetry::TelemetryConfig = (&tc).into();
        assert!(nc.enabled);
        assert_eq!(nc.service_name, "test");
        assert_eq!(nc.sample_rate, 0.5);
    }
}
