//! `netray.toml`: the one configuration of `netray serve` (specs/features/v2-cutover, V5.2).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;

use netray_common::config::ConfigError;
use serde::Deserialize;

const MODULES: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

const LEGACY_PREFIXES: [&str; 11] = [
    "LENS_",
    "NETRAY_DNS_",
    "NETRAY_TLS_",
    "NETRAY_HTTP_",
    "NETRAY_EMAIL_",
    "NETRAY_IP_",
    "PRISM_",
    "TLSIGHT_",
    "SPECTRA_",
    "BEACON_",
    "IFCONFIG_",
];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    pub engine: EngineConfig,
    pub store: StoreConfig,
    pub admission: AdmissionConfig,
    pub badges: lens::config::BadgesConfig,
    pub og_cards: lens::config::OgCardsConfig,
    pub telemetry: netray_common::telemetry::TelemetryConfig,
    /// Per-module tables (`[modules.http]`): each module's own keys and its section window
    /// `deadline_ms`.
    pub modules: BTreeMap<String, toml::Table>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ServerConfig {
    pub bind: SocketAddr,
    pub metrics_bind: SocketAddr,
    pub trusted_proxies: Vec<String>,
    pub apex: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        let lens = lens::config::ServerConfig::default();
        Self {
            bind: lens.bind,
            metrics_bind: lens.metrics_bind,
            trusted_proxies: lens.trusted_proxies,
            apex: "netray.info".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EngineConfig {
    /// Budget of the resolve stage in milliseconds, counted from the start of a check.
    pub resolve_timeout_ms: u64,
    pub profile_path: Option<String>,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            resolve_timeout_ms: lens::config::BackendsConfig::default().resolve_timeout_ms,
            profile_path: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct StoreConfig {
    pub enabled: bool,
    pub db_path: PathBuf,
}

impl Default for StoreConfig {
    fn default() -> Self {
        let lens = lens::config::HistoryConfig::default();
        Self {
            enabled: lens.enabled,
            db_path: lens.db_path,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AdmissionConfig {
    pub client_budget_per_hour: u32,
    pub domain_run_slots: u32,
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub global_per_minute: u32,
    pub global_burst: u32,
    pub cache_enabled: bool,
    pub cache_window_seconds: u64,
}

impl Default for AdmissionConfig {
    fn default() -> Self {
        let admission = lens::config::AdmissionConfig::default();
        let rate = lens::config::RateLimitConfig::default();
        let cache = lens::config::CacheConfig::default();
        Self {
            client_budget_per_hour: admission.client_budget_per_hour,
            domain_run_slots: admission.domain_run_slots,
            per_ip_per_minute: rate.per_ip_per_minute,
            per_ip_burst: rate.per_ip_burst,
            global_per_minute: rate.global_per_minute,
            global_burst: rate.global_burst,
            cache_enabled: cache.enabled,
            cache_window_seconds: cache.ttl_seconds,
        }
    }
}

impl Config {
    /// Loads the file at `path` (if any) and the `NETRAY_` variables of `env`.
    ///
    /// Precedence (highest first): env vars > TOML file > built-in defaults. The retired env
    /// prefixes and the legacy key paths of the file are refused before anything is read into
    /// the config.
    pub fn load(path: Option<&str>, env: Vec<(OsString, OsString)>) -> Result<Self, ConfigError> {
        for legacy in LEGACY_PREFIXES {
            netray_common::config::refuse_legacy_prefix(legacy, "NETRAY_", &env)?;
        }
        if let Some(raw) = path
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| text.parse::<toml::Table>().ok())
        {
            crate::legacy::refuse(&raw).map_err(ConfigError::Message)?;
        }
        let env = env.into_iter().filter(|(k, _)| k != "NETRAY_LIVE_TESTS");
        let cfg: Config = netray_common::config::load_with_env(path, "NETRAY_", env)?;
        cfg.validate()?;
        Ok(cfg)
    }

    /// The section window of module `name` in milliseconds.
    fn deadline_ms(&self, name: &str) -> Result<u64, ConfigError> {
        let bad = |what: String| {
            ConfigError::Message(format!(
                "invalid configuration: modules.{name}.deadline_ms {what}"
            ))
        };
        match self.modules.get(name).and_then(|t| t.get("deadline_ms")) {
            None => Ok(lens::config::BackendConfig::default().timeout_ms),
            Some(toml::Value::Integer(n)) => {
                u64::try_from(*n).map_err(|_| bad("must not be negative".into()))
            }
            Some(toml::Value::String(s)) => s
                .trim()
                .parse()
                .map_err(|_| bad(format!("must be an integer, got `{s}`"))),
            Some(other) => Err(bad(format!("must be an integer, got {}", other.type_str()))),
        }
    }

    /// Rejects zero limits, a bad telemetry endpoint and section windows that do not fit the
    /// hard deadline or the resolve stage.
    fn validate(&self) -> Result<(), ConfigError> {
        let a = &self.admission;
        for (name, value) in [
            ("admission.per_ip_per_minute", a.per_ip_per_minute),
            ("admission.per_ip_burst", a.per_ip_burst),
            ("admission.global_per_minute", a.global_per_minute),
            ("admission.global_burst", a.global_burst),
            ("admission.domain_run_slots", a.domain_run_slots),
        ] {
            reject_zero(name, u64::from(value))?;
        }
        reject_zero("badges.ttl_seconds", self.badges.ttl_seconds)?;
        netray_common::telemetry::validate(&self.telemetry).map_err(ConfigError::Message)?;

        let [dns, tls, http, email, ip] = MODULES.map(|m| self.deadline_ms(m));
        let (dns, tls, http, email, ip) = (dns?, tls?, http?, email?, ip?);
        let wave1_ms = dns.max(tls).max(http).max(email);
        let budget_ms = wave1_ms.saturating_add(ip);
        let hard_ms = lens::check::HARD_DEADLINE.as_millis() as u64;
        if budget_ms >= hard_ms {
            return Err(ConfigError::Message(format!(
                "invalid configuration: module deadline_ms (slowest wave-1 deadline_ms {wave1_ms} + modules.ip.deadline_ms {ip}) = {budget_ms} ms must stay below the {hard_ms} ms hard deadline"
            )));
        }
        let resolve = self.engine.resolve_timeout_ms;
        for (name, deadline) in [("ip", ip), ("tls", tls), ("http", http)] {
            if resolve >= deadline {
                return Err(ConfigError::Message(format!(
                    "invalid configuration: engine.resolve_timeout_ms {resolve} must stay below modules.{name}.deadline_ms {deadline}: the {name} section waits for the resolve inside its own window, both counted from the start of a check"
                )));
            }
        }
        Ok(())
    }

    /// The lens configuration `serve` starts lens's application with. `validate` has run.
    pub fn to_lens(&self) -> lens::config::Config {
        let window = |name: &str| lens::config::BackendConfig {
            url: None,
            timeout_ms: self.deadline_ms(name).expect("deadline validated at load"),
        };
        let mut modules = self.modules.clone();
        for table in modules.values_mut() {
            table.remove("deadline_ms");
        }
        modules.retain(|_, table| !table.is_empty());
        let a = &self.admission;
        lens::config::Config {
            server: lens::config::ServerConfig {
                bind: self.server.bind,
                metrics_bind: self.server.metrics_bind,
                trusted_proxies: self.server.trusted_proxies.clone(),
            },
            backends: lens::config::BackendsConfig {
                resolve_timeout_ms: self.engine.resolve_timeout_ms,
                dns: window("dns"),
                tls: window("tls"),
                ip: window("ip"),
                http: Some(window("http")),
                email: Some(window("email")),
            },
            ecosystem: Default::default(),
            cache: lens::config::CacheConfig {
                enabled: a.cache_enabled,
                ttl_seconds: a.cache_window_seconds,
            },
            rate_limit: lens::config::RateLimitConfig {
                per_ip_per_minute: a.per_ip_per_minute,
                per_ip_burst: a.per_ip_burst,
                global_per_minute: a.global_per_minute,
                global_burst: a.global_burst,
            },
            admission: lens::config::AdmissionConfig {
                client_budget_per_hour: a.client_budget_per_hour,
                domain_run_slots: a.domain_run_slots,
            },
            scoring: lens::config::ScoringConfig {
                profile_path: self.engine.profile_path.clone(),
            },
            site: Default::default(),
            badges: self.badges.clone(),
            og_cards: self.og_cards.clone(),
            api: Default::default(),
            inspect: Default::default(),
            snapshots: lens::config::SnapshotsConfig {
                enabled: false,
                ..Default::default()
            },
            history: lens::config::HistoryConfig {
                enabled: self.store.enabled,
                db_path: self.store.db_path.clone(),
            },
            modules,
            telemetry: self.telemetry.clone(),
        }
    }
}

fn reject_zero(name: &str, value: u64) -> Result<(), ConfigError> {
    if value == 0 {
        return Err(ConfigError::Message(format!(
            "invalid configuration: {name} must not be zero"
        )));
    }
    Ok(())
}
