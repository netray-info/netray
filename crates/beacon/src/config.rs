use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default, deserialize_with = "deserialize_ecosystem")]
    pub ecosystem: EcosystemConfig,
    #[serde(default)]
    pub backends: BackendsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct DnsConfig {
    #[serde(default = "default_resolvers")]
    pub resolvers: Vec<String>,
    #[serde(default = "default_dns_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct HttpConfig {
    #[serde(default = "default_http_timeout")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RateLimitConfig {
    #[serde(default = "default_per_ip")]
    pub per_ip: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DkimConfig {
    #[serde(default = "default_max_user_selectors")]
    pub max_user_selectors: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// Strict mirror of [`EcosystemConfig`]: the upstream struct lives in
/// netray-common and accepts unknown keys, so `[ecosystem]` is parsed through
/// this type to reject typos like the other sections do. The exhaustive
/// struct literal below stops compiling if upstream adds a field.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StrictEcosystemConfig {
    ip_base_url: Option<String>,
    dns_base_url: Option<String>,
    tls_base_url: Option<String>,
    http_base_url: Option<String>,
    email_base_url: Option<String>,
    lens_base_url: Option<String>,
}

fn deserialize_ecosystem<'de, D>(deserializer: D) -> Result<EcosystemConfig, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let s = StrictEcosystemConfig::deserialize(deserializer)?;
    Ok(EcosystemConfig {
        ip_base_url: s.ip_base_url,
        dns_base_url: s.dns_base_url,
        tls_base_url: s.tls_base_url,
        http_base_url: s.http_base_url,
        email_base_url: s.email_base_url,
        lens_base_url: s.lens_base_url,
    })
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendsConfig {
    /// IP enrichment service base URL. Empty string disables enrichment.
    #[serde(default)]
    pub ip_url: String,
    /// Shared timeout for all backend HTTP calls, in milliseconds.
    #[serde(default = "default_backends_timeout_ms")]
    pub timeout_ms: u64,
}

fn default_backends_timeout_ms() -> u64 {
    5000
}

/// Config file loaded when neither argv nor `BEACON_CONFIG` names one;
/// relative to the working directory (the container's `WORKDIR`).
pub const DEFAULT_CONFIG_PATH: &str = "beacon.toml";

/// Where the config file path came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSource {
    Argv,
    Env,
    Default,
}

impl ConfigSource {
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigSource::Argv => "argv",
            ConfigSource::Env => "BEACON_CONFIG",
            ConfigSource::Default => "default",
        }
    }
}

/// Picks the config file path: first CLI argument, then `BEACON_CONFIG`,
/// then [`DEFAULT_CONFIG_PATH`].
pub fn resolve_path(arg: Option<String>, env: Option<String>) -> (String, ConfigSource) {
    match (arg, env) {
        (Some(path), _) => (path, ConfigSource::Argv),
        (None, Some(path)) => (path, ConfigSource::Env),
        (None, None) => (DEFAULT_CONFIG_PATH.to_string(), ConfigSource::Default),
    }
}

impl Config {
    pub fn load(path: Option<&str>) -> Result<Self, config::ConfigError> {
        Self::load_with_env(path, None)
    }

    /// `env` replaces the process environment as the override source when set
    /// (tests); `None` reads the process environment.
    fn load_with_env(
        path: Option<&str>,
        env: Option<config::Map<String, String>>,
    ) -> Result<Self, config::ConfigError> {
        let mut builder = config::Config::builder();

        if let Some(path) = path {
            builder = builder.add_source(config::File::with_name(path).required(true));
        }

        builder = builder.add_source(
            config::Environment::with_prefix("BEACON")
                .separator("__")
                .try_parsing(true)
                .source(env),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_path_prefers_argv_over_env() {
        let (path, source) = resolve_path(
            Some("a.toml".to_string()),
            Some("/etc/beacon/b.toml".to_string()),
        );
        assert_eq!(path, "a.toml");
        assert_eq!(source, ConfigSource::Argv);
    }

    #[test]
    fn resolve_path_uses_env_without_argv() {
        let (path, source) = resolve_path(None, Some("/etc/beacon/b.toml".to_string()));
        assert_eq!(path, "/etc/beacon/b.toml");
        assert_eq!(source, ConfigSource::Env);
    }

    #[test]
    fn resolve_path_falls_back_to_baked_default() {
        let (path, source) = resolve_path(None, None);
        assert_eq!(path, DEFAULT_CONFIG_PATH);
        assert_eq!(source, ConfigSource::Default);
    }

    fn repo_file(rel: &str) -> String {
        format!("{}/{rel}", env!("CARGO_MANIFEST_DIR"))
    }

    /// Writes `body` to a per-test temp file and loads it with no env overrides.
    fn load_str(name: &str, body: &str) -> Result<Config, config::ConfigError> {
        let path = std::env::temp_dir().join(format!(
            "beacon-config-test-{name}-{}.toml",
            std::process::id()
        ));
        std::fs::write(&path, body).unwrap();
        let result = Config::load_with_env(path.to_str(), Some(config::Map::new()));
        std::fs::remove_file(&path).ok();
        result
    }

    fn assert_unknown_field(result: Result<Config, config::ConfigError>, field: &str) {
        let err = result
            .expect_err("unknown key must be rejected")
            .to_string();
        assert!(
            err.contains(&format!("unknown field `{field}`")),
            "error should name the unknown field `{field}`: {err}"
        );
    }

    #[test]
    fn unknown_key_in_section_is_rejected() {
        assert_unknown_field(
            load_str("section", "[server]\nbnd = \"0.0.0.0:8084\"\n"),
            "bnd",
        );
    }

    #[test]
    fn unknown_top_level_section_is_rejected() {
        assert_unknown_field(load_str("toplevel", "[servr]\nbind = \"x\"\n"), "servr");
    }

    #[test]
    fn unknown_key_in_ecosystem_is_rejected() {
        assert_unknown_field(
            load_str(
                "ecosystem",
                "[ecosystem]\nip_url = \"https://ip.example.com\"\n",
            ),
            "ip_url",
        );
    }

    #[test]
    fn nested_backends_ip_table_is_rejected() {
        // The shape argus-oci's template wrote before 2026-10: beacon reads
        // `[backends] ip_url`, not `[backends.ip] url`.
        assert_unknown_field(
            load_str(
                "backends",
                "[backends.ip]\nurl = \"http://ifconfig-rs:8000\"\n",
            ),
            "ip",
        );
    }

    #[test]
    fn repo_configs_load() {
        for rel in ["beacon.toml", "beacon.dev.toml"] {
            Config::load_with_env(Some(&repo_file(rel)), Some(config::Map::new()))
                .unwrap_or_else(|e| panic!("{rel} must load: {e}"));
        }
        // `.example` is no format the loader recognises; load it as TOML.
        let example = std::fs::read_to_string(repo_file("beacon.toml.example")).unwrap();
        load_str("example", &example).expect("beacon.toml.example must load");
    }

    #[test]
    fn production_shaped_config_loads() {
        let cfg = Config::load_with_env(
            Some(&repo_file("tests/fixtures/beacon.production.toml")),
            Some(config::Map::new()),
        )
        .expect("production-shaped config must load");
        assert_eq!(cfg.server.metrics_bind, "0.0.0.0:9090");
        assert_eq!(cfg.server.trusted_proxies.len(), 2);
        assert_eq!(cfg.rate_limit.per_ip, "10/min");
        assert_eq!(
            cfg.ecosystem.lens_base_url.as_deref(),
            Some("https://lens.example.com")
        );
        assert_eq!(cfg.backends.ip_url, "http://ifconfig-rs:8000");
        assert_eq!(cfg.backends.timeout_ms, 500);
    }

    #[test]
    fn env_overrides_still_apply() {
        let env = config::Map::from([
            (
                "BEACON__SERVER__BIND".to_string(),
                "0.0.0.0:8084".to_string(),
            ),
            (
                "BEACON__BACKENDS__IP_URL".to_string(),
                "http://ip.example.com".to_string(),
            ),
            (
                "BEACON__DKIM__MAX_USER_SELECTORS".to_string(),
                "7".to_string(),
            ),
            // Single underscore: not under the `BEACON__` prefix, so it must
            // not reach the strict deserializer as a `config` key.
            (
                "BEACON_CONFIG".to_string(),
                "/etc/beacon/beacon.toml".to_string(),
            ),
        ]);
        let cfg = Config::load_with_env(Some(&repo_file("beacon.toml")), Some(env))
            .expect("env overrides must load");
        assert_eq!(cfg.server.bind, "0.0.0.0:8084");
        assert_eq!(cfg.backends.ip_url, "http://ip.example.com");
        assert_eq!(cfg.dkim.max_user_selectors, 7);
    }

    #[test]
    fn unknown_env_override_is_rejected() {
        let env = config::Map::from([("BEACON__SERVER__BINDD".to_string(), "x".to_string())]);
        assert_unknown_field(
            Config::load_with_env(Some(&repo_file("beacon.toml")), Some(env)),
            "bindd",
        );
    }
}
