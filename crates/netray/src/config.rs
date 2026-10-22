//! `netray.toml`: the one configuration of `netray serve` (specs/features/v2-cutover, V5.2).

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::PathBuf;

use std::time::Duration;

use netray_common::config::ConfigError;
use netray_model::Protocol;
use netray_web::api::{AdmissionLimits, ApiConfig};
use netray_web::config::{ApexConfig, BadgesConfig, OgCardsConfig};
use netray_web::pages::InspectConfig;
use serde::Deserialize;

const MODULES: [&str; 5] = ["dns", "tls", "http", "email", "ip"];

/// The defaults lens gave the keys `netray.toml` took over (`crates/lens/src/config.rs`).
const DEFAULT_BIND: ([u8; 4], u16) = ([0, 0, 0, 0], 8082);
const DEFAULT_METRICS_BIND: ([u8; 4], u16) = ([127, 0, 0, 1], 9090);
const DEFAULT_RESOLVE_TIMEOUT_MS: u64 = 1500;
const DEFAULT_DEADLINE_MS: u64 = 2000;
const DEFAULT_DB_PATH: &str = "history.db";
const DEFAULT_CLIENT_BUDGET_PER_HOUR: u32 = 30;
const DEFAULT_DOMAIN_RUN_SLOTS: u32 = 8;
const DEFAULT_PER_IP_PER_MINUTE: u32 = 10;
const DEFAULT_PER_IP_BURST: u32 = 3;
const DEFAULT_GLOBAL_PER_MINUTE: u32 = 100;
const DEFAULT_GLOBAL_BURST: u32 = 20;
const DEFAULT_CACHE_WINDOW_SECONDS: u64 = 300;

/// The repository tooling's own `NETRAY_` variables, which reach every process its scripts start
/// and are no config keys: the live-test switch, a prebuilt binary (`tests/repo/lib/netray.sh`)
/// and `just design-check`'s canvas.
const TOOL_VARS: [&str; 3] = ["NETRAY_LIVE_TESTS", "NETRAY_BIN", "NETRAY_DESIGN_DIR"];

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
    pub badges: BadgesConfig,
    pub og_cards: OgCardsConfig,
    /// `[api]` with its `[api.<inspector>]` tables.
    pub api: ApiConfig,
    /// `[inspect.<tool>]`: the inspector pages' budgets.
    pub inspect: InspectConfig,
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
        Self {
            bind: DEFAULT_BIND.into(),
            metrics_bind: DEFAULT_METRICS_BIND.into(),
            trusted_proxies: Vec::new(),
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
            resolve_timeout_ms: DEFAULT_RESOLVE_TIMEOUT_MS,
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
        Self {
            enabled: true,
            db_path: PathBuf::from(DEFAULT_DB_PATH),
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
        Self {
            client_budget_per_hour: DEFAULT_CLIENT_BUDGET_PER_HOUR,
            domain_run_slots: DEFAULT_DOMAIN_RUN_SLOTS,
            per_ip_per_minute: DEFAULT_PER_IP_PER_MINUTE,
            per_ip_burst: DEFAULT_PER_IP_BURST,
            global_per_minute: DEFAULT_GLOBAL_PER_MINUTE,
            global_burst: DEFAULT_GLOBAL_BURST,
            cache_enabled: true,
            cache_window_seconds: DEFAULT_CACHE_WINDOW_SECONDS,
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
        let env = env
            .into_iter()
            .filter(|(k, _)| !TOOL_VARS.iter().any(|t| k == t));
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
            None => Ok(DEFAULT_DEADLINE_MS),
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
        self.api.validate().map_err(ConfigError::Message)?;
        self.inspect.validate().map_err(ConfigError::Message)?;
        netray_common::telemetry::validate(&self.telemetry).map_err(ConfigError::Message)?;

        let [dns, tls, http, email, ip] = MODULES.map(|m| self.deadline_ms(m));
        let (dns, tls, http, email, ip) = (dns?, tls?, http?, email?, ip?);
        let wave1_ms = dns.max(tls).max(http).max(email);
        let budget_ms = wave1_ms.saturating_add(ip);
        let hard_ms = netray_engine::domain_run::HARD_DEADLINE.as_millis() as u64;
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

    /// The section window of `protocol` (`[modules.<m>] deadline_ms`). `validate` has run.
    pub fn section_window(&self, protocol: Protocol) -> Duration {
        Duration::from_millis(
            self.deadline_ms(protocol.as_str())
                .expect("deadline validated at load"),
        )
    }

    /// The budget of the resolve stage (`[engine] resolve_timeout_ms`).
    pub fn resolve_budget(&self) -> Duration {
        Duration::from_millis(self.engine.resolve_timeout_ms)
    }

    /// The module tables without the section windows, as the module registry reads them. A known
    /// module's table left empty goes (the module runs on its defaults); an unknown one stays, so
    /// the registry refuses it even when it held only its section window.
    pub fn module_tables(&self) -> BTreeMap<String, toml::Table> {
        let mut modules = self.modules.clone();
        for table in modules.values_mut() {
            table.remove("deadline_ms");
        }
        modules.retain(|name, table| !table.is_empty() || !MODULES.contains(&name.as_str()));
        modules
    }

    /// What the apex's routes read: proxies, limits, cache, badges, OG cards, `[api]`, `[inspect]`.
    pub fn apex_config(&self) -> ApexConfig {
        let a = &self.admission;
        ApexConfig {
            trusted_proxies: self.server.trusted_proxies.clone(),
            limits: AdmissionLimits {
                client_budget_per_hour: a.client_budget_per_hour,
                domain_run_slots: a.domain_run_slots,
                rate_limit_per_ip_per_minute: a.per_ip_per_minute,
                rate_limit_per_ip_burst: a.per_ip_burst,
                rate_limit_global_per_minute: a.global_per_minute,
                rate_limit_global_burst: a.global_burst,
            },
            cache_enabled: a.cache_enabled,
            cache_window_seconds: a.cache_window_seconds,
            badges: self.badges.clone(),
            og_cards: self.og_cards.clone(),
            api: self.api.clone(),
            inspect: self.inspect.clone(),
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
