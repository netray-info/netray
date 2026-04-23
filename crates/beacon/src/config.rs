use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
    #[serde(default = "default_dnsbl")]
    pub dnsbl: DnsblConfig,
    #[serde(default = "default_http")]
    pub http: HttpConfig,
    #[serde(default = "default_rate_limit")]
    pub rate_limit: RateLimitConfig,
    #[serde(default = "default_dkim")]
    pub dkim: DkimConfig,
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
    pub bind: String,
    #[serde(default = "default_metrics_bind")]
    pub metrics_bind: String,
    #[serde(default)]
    pub trusted_proxies: Vec<String>,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent_inspections: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsConfig {
    #[serde(default = "default_resolvers")]
    pub resolvers: Vec<String>,
    #[serde(default = "default_dns_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsblConfig {
    #[serde(default = "default_dnsbl_zones")]
    pub zones: Vec<String>,
    #[serde(default = "default_dnsbl_timeout")]
    pub timeout_ms: u64,
    /// Resolvers used for DNSBL queries only. Defaults to the system resolver
    /// because most public DNSBLs (notably Spamhaus) block queries from
    /// public/open resolvers like Cloudflare or Google.
    #[serde(default = "default_dnsbl_resolvers")]
    pub resolvers: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HttpConfig {
    #[serde(default = "default_http_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    #[serde(default = "default_per_ip")]
    pub per_ip: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DkimConfig {
    #[serde(default = "default_max_user_selectors")]
    pub max_user_selectors: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TelemetryConfig {
    #[serde(default = "default_log_format")]
    pub log_format: String,
    pub otlp_endpoint: Option<String>,
    #[serde(default = "default_service_name")]
    pub service_name: String,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: f64,
}

pub use netray_common::ecosystem::EcosystemConfig;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BackendsConfig {
    #[serde(default)]
    pub ip: Option<netray_common::backend::BackendConfig>,
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(path) = path {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("BEACON")
                .separator("__")
                .try_parsing(true),
        );

        builder.build()?.try_deserialize()
    }
}

impl From<&TelemetryConfig> for netray_common::telemetry::TelemetryConfig {
    fn from(cfg: &TelemetryConfig) -> Self {
        let log_format = match cfg.log_format.as_str() {
            "json" => netray_common::telemetry::LogFormat::Json,
            _ => netray_common::telemetry::LogFormat::Text,
        };
        netray_common::telemetry::TelemetryConfig {
            enabled: cfg.otlp_endpoint.is_some(),
            log_format,
            otlp_endpoint: cfg
                .otlp_endpoint
                .clone()
                .unwrap_or_else(|| "http://localhost:4318".to_string()),
            service_name: cfg.service_name.clone(),
            sample_rate: cfg.sample_rate,
        }
    }
}

// Default functions

fn default_server() -> ServerConfig {
    ServerConfig {
        bind: default_bind(),
        metrics_bind: default_metrics_bind(),
        trusted_proxies: Vec::new(),
        max_concurrent_inspections: default_max_concurrent(),
    }
}

fn default_max_concurrent() -> usize {
    16
}

fn default_bind() -> String {
    "127.0.0.1:3000".to_string()
}

fn default_metrics_bind() -> String {
    "127.0.0.1:9090".to_string()
}

fn default_dns() -> DnsConfig {
    DnsConfig {
        resolvers: default_resolvers(),
        timeout_ms: default_dns_timeout(),
    }
}

fn default_resolvers() -> Vec<String> {
    vec!["cloudflare".to_string()]
}

fn default_dns_timeout() -> u64 {
    5000
}

fn default_dnsbl() -> DnsblConfig {
    DnsblConfig {
        zones: default_dnsbl_zones(),
        timeout_ms: default_dnsbl_timeout(),
        resolvers: default_dnsbl_resolvers(),
    }
}

fn default_dnsbl_resolvers() -> Vec<String> {
    vec!["system".to_string()]
}

fn default_dnsbl_zones() -> Vec<String> {
    vec![
        "zen.spamhaus.org".to_string(),
        "b.barracudacentral.org".to_string(),
        "bl.spamcop.net".to_string(),
        "dbl.spamhaus.org".to_string(),
    ]
}

fn default_dnsbl_timeout() -> u64 {
    2000
}

fn default_http() -> HttpConfig {
    HttpConfig {
        timeout_ms: default_http_timeout(),
    }
}

fn default_http_timeout() -> u64 {
    10000
}

fn default_rate_limit() -> RateLimitConfig {
    RateLimitConfig {
        per_ip: default_per_ip(),
    }
}

fn default_per_ip() -> String {
    "10/min".to_string()
}

fn default_dkim() -> DkimConfig {
    DkimConfig {
        max_user_selectors: default_max_user_selectors(),
    }
}

fn default_max_user_selectors() -> usize {
    5
}

fn default_log_format() -> String {
    "json".to_string()
}

fn default_service_name() -> String {
    "beacon".to_string()
}

fn default_sample_rate() -> f64 {
    1.0
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            log_format: default_log_format(),
            otlp_endpoint: None,
            service_name: default_service_name(),
            sample_rate: default_sample_rate(),
        }
    }
}
