use std::ffi::OsString;
use std::net::SocketAddr;

use serde::Deserialize;

pub use config::ConfigError;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
    #[serde(default)]
    pub meta: MetaConfig,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectConfig {
    #[serde(default = "default_request_timeout_secs")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_total_timeout_secs")]
    pub total_timeout_secs: u64,
    #[serde(default = "default_max_redirects")]
    pub max_redirects: usize,
    #[serde(default = "default_user_agent")]
    pub user_agent: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
pub struct EnrichmentConfig {
    #[serde(default)]
    pub ip_url: Option<String>,
    #[serde(default = "default_enrichment_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MetaConfig {
    #[serde(default)]
    pub ip_base_url: Option<String>,
    #[serde(default)]
    pub dns_base_url: Option<String>,
    #[serde(default)]
    pub tls_base_url: Option<String>,
    #[serde(default)]
    pub http_base_url: Option<String>,
    #[serde(default)]
    pub email_base_url: Option<String>,
    #[serde(default)]
    pub lens_base_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
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
    /// Loads the TOML file at `path` (if any), then `SPECTRA__<SECTION>__<KEY>`
    /// env overrides. Unknown keys in either source are a load error.
    pub fn load(path: Option<&str>) -> Result<Self, ConfigError> {
        Self::load_with_env(path, std::env::vars_os())
    }

    /// Rejects the values `RateLimitState::new` would otherwise panic on.
    fn validate(&self) -> Result<(), ConfigError> {
        for (name, value) in [
            ("per_ip_per_minute", self.limits.per_ip_per_minute),
            ("per_ip_burst", self.limits.per_ip_burst),
            ("per_target_per_minute", self.limits.per_target_per_minute),
            ("per_target_burst", self.limits.per_target_burst),
        ] {
            if value == 0 {
                return Err(ConfigError::Message(format!(
                    "invalid configuration: limits.{name} must not be zero"
                )));
            }
        }
        Ok(())
    }

    fn load_with_env(
        path: Option<&str>,
        env: impl IntoIterator<Item = (OsString, OsString)>,
    ) -> Result<Self, ConfigError> {
        let cfg: Self = netray_common::config::load_with_env(path, "SPECTRA__", env)?;
        cfg.validate()?;
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
                Some("text") => netray_common::telemetry::LogFormat::Text,
                _ => netray_common::telemetry::LogFormat::Json, // default: json (production-friendly)
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
        // body_read_limit_bytes removed (YAGNI — re-add when body sniffing is implemented)
    }

    fn load_toml(name: &str, toml: &str) -> Result<Config, ConfigError> {
        let path = std::env::temp_dir().join(format!("spectra-{}-{name}.toml", std::process::id()));
        std::fs::write(&path, toml).unwrap();
        let result = Config::load(Some(path.to_str().unwrap()));
        let _ = std::fs::remove_file(&path);
        result
    }

    #[test]
    fn unknown_top_level_section_is_rejected() {
        let err = load_toml(
            "unknown-section",
            "[ecosystem]\nip_base_url = \"https://ip.example.com\"\n",
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("unknown field `ecosystem`"), "{err}");
    }

    #[test]
    fn unknown_nested_key_is_rejected() {
        for (section, key) in [
            ("server", "bnid"),
            ("inspect", "body_read_limit_bytes"),
            ("limits", "per_ip_per_hour"),
            ("enrichment", "url"),
            ("telemetry", "endpoint"),
            ("meta", "ip_url"),
        ] {
            let err = load_toml(
                &format!("unknown-{section}"),
                &format!("[{section}]\n{key} = 1\n"),
            )
            .unwrap_err()
            .to_string();
            assert!(
                err.contains(&format!("unknown field `{key}`")),
                "[{section}] {key}: {err}"
            );
        }
    }

    #[test]
    fn repo_config_files_load() {
        for file in ["spectra.example.toml", "spectra.dev.toml"] {
            let path = format!("{}/{file}", env!("CARGO_MANIFEST_DIR"));
            Config::load(Some(&path)).unwrap_or_else(|e| panic!("{file}: {e}"));
        }
    }

    #[test]
    fn env_overrides_apply() {
        let env = [
            ("SPECTRA__ENRICHMENT__IP_URL", "http://ip.example.com"),
            ("SPECTRA__LIMITS__PER_IP_BURST", "7"),
            ("SPECTRA__META__LENS_BASE_URL", "https://lens.example.com"),
        ]
        .map(|(k, v)| (OsString::from(k), OsString::from(v)));
        let cfg = Config::load_with_env(None, env).unwrap();
        assert_eq!(
            cfg.enrichment.ip_url.as_deref(),
            Some("http://ip.example.com")
        );
        assert_eq!(cfg.limits.per_ip_burst, 7);
        assert_eq!(
            cfg.meta.lens_base_url.as_deref(),
            Some("https://lens.example.com")
        );
    }

    #[test]
    fn unknown_env_key_is_rejected() {
        let env = [(
            OsString::from("SPECTRA__BACKENDS__IP__URL"),
            OsString::from("http://ip.example.com"),
        )];
        let err = Config::load_with_env(None, env).unwrap_err().to_string();
        assert!(err.contains("unknown field `backends`"), "{err}");
    }

    /// The production template (argus-oci `spectra.toml.j2`) rendered in the
    /// key layout this code reads: every key the template sets must load.
    #[test]
    fn production_shaped_config_loads() {
        let cfg = load_toml(
            "production",
            r#"
[server]
bind = "0.0.0.0:8082"
metrics_bind = "0.0.0.0:9090"
trusted_proxies = ["10.0.0.0/8", "172.16.0.0/12"]

[inspect]
request_timeout_secs = 10
total_timeout_secs = 30
max_redirects = 10
user_agent = "netray-spectra"

[limits]
per_ip_per_minute = 20
per_ip_burst = 8
per_target_per_minute = 40
per_target_burst = 12
max_concurrent_connections = 256

[enrichment]
ip_url = "http://ifconfig-rs:8000"
timeout_ms = 500

[meta]
ip_base_url = "https://ip.example.com"
dns_base_url = "https://dns.example.com"
tls_base_url = "https://tls.example.com"
http_base_url = "https://http.example.com"
email_base_url = "https://email.example.com"
lens_base_url = "https://lens.example.com"

[telemetry]
log_format = "json"
service_name = "spectra"
"#,
        )
        .unwrap();
        assert_eq!(
            cfg.server.metrics_bind,
            SocketAddr::from(([0, 0, 0, 0], 9090))
        );
        assert_eq!(cfg.server.trusted_proxies, ["10.0.0.0/8", "172.16.0.0/12"]);
        assert_eq!(cfg.limits.per_ip_per_minute, 20);
        assert_eq!(cfg.limits.per_target_burst, 12);
        assert_eq!(
            cfg.enrichment.ip_url.as_deref(),
            Some("http://ifconfig-rs:8000")
        );
        assert_eq!(
            cfg.meta.ip_base_url.as_deref(),
            Some("https://ip.example.com")
        );
        assert_eq!(
            cfg.meta.email_base_url.as_deref(),
            Some("https://email.example.com")
        );
        assert_eq!(
            cfg.meta.lens_base_url.as_deref(),
            Some("https://lens.example.com")
        );
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
