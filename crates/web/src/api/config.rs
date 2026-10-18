//! The `[api]` config table (spec v2-api requirement 8).
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiDnsConfig {}

/// The limiters of `POST /tls/inspect`; the inspection itself runs on `[modules.tls]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiTlsConfig {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub per_target_per_minute: u32,
    pub per_target_burst: u32,
}

impl Default for ApiTlsConfig {
    fn default() -> Self {
        Self {
            per_ip_per_minute: 30,
            per_ip_burst: 10,
            per_target_per_minute: 60,
            per_target_burst: 20,
        }
    }
}

/// The limiters of `POST /http/inspect`; the inspection itself runs on `[modules.http]`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiHttpConfig {
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub per_target_per_minute: u32,
    pub per_target_burst: u32,
}

impl Default for ApiHttpConfig {
    fn default() -> Self {
        Self {
            per_ip_per_minute: 10,
            per_ip_burst: 5,
            per_target_per_minute: 30,
            per_target_burst: 10,
        }
    }
}

fn non_zero(table: &str, quotas: [(&str, u32); 4]) -> Result<(), String> {
    for (key, value) in quotas {
        if value == 0 {
            return Err(format!("api.{table}.{key} must be greater than 0"));
        }
    }
    Ok(())
}

impl ApiTlsConfig {
    pub fn validate(&self) -> Result<(), String> {
        non_zero(
            "tls",
            [
                ("per_ip_per_minute", self.per_ip_per_minute),
                ("per_ip_burst", self.per_ip_burst),
                ("per_target_per_minute", self.per_target_per_minute),
                ("per_target_burst", self.per_target_burst),
            ],
        )
    }
}

impl ApiHttpConfig {
    pub fn validate(&self) -> Result<(), String> {
        non_zero(
            "http",
            [
                ("per_ip_per_minute", self.per_ip_per_minute),
                ("per_ip_burst", self.per_ip_burst),
                ("per_target_per_minute", self.per_target_per_minute),
                ("per_target_burst", self.per_target_burst),
            ],
        )
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiEmailConfig {}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiIpConfig {
    pub internal_mode: bool,
    pub filtered_headers: Vec<String>,
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub per_target_per_minute: u32,
    pub per_target_burst: u32,
    pub batch_max_size: u32,
    pub batch_addresses_per_minute: u32,
    pub batch_addresses_burst: u32,
}

impl Default for ApiIpConfig {
    fn default() -> Self {
        Self {
            internal_mode: false,
            filtered_headers: Vec::new(),
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            per_target_per_minute: 120,
            per_target_burst: 20,
            batch_max_size: 100,
            batch_addresses_per_minute: 60,
            batch_addresses_burst: 100,
        }
    }
}

fn bounded(key: &str, value: u32, cap: u32) -> Result<(), String> {
    if value == 0 {
        return Err(format!("api.ip.{key} must be greater than 0"));
    }
    if value > cap {
        return Err(format!("api.ip.{key} ({value}) exceeds hard cap ({cap})"));
    }
    Ok(())
}

impl ApiIpConfig {
    pub fn validate(&self) -> Result<(), String> {
        use netray_ip::config::{
            HARD_CAP_BATCH_SIZE, HARD_CAP_RATE_LIMIT_BURST, HARD_CAP_RATE_LIMIT_PER_MINUTE,
        };
        let per_minute = HARD_CAP_RATE_LIMIT_PER_MINUTE;
        let burst = HARD_CAP_RATE_LIMIT_BURST;
        let batch = HARD_CAP_BATCH_SIZE as u32;
        bounded("per_ip_per_minute", self.per_ip_per_minute, per_minute)?;
        bounded("per_ip_burst", self.per_ip_burst, burst)?;
        if self.per_ip_burst < 2 {
            return Err(format!(
                "api.ip.per_ip_burst ({}) is below the cost of a diff (2)",
                self.per_ip_burst
            ));
        }
        bounded(
            "per_target_per_minute",
            self.per_target_per_minute,
            per_minute,
        )?;
        bounded("per_target_burst", self.per_target_burst, burst)?;
        bounded("batch_max_size", self.batch_max_size, batch)?;
        bounded(
            "batch_addresses_per_minute",
            self.batch_addresses_per_minute,
            per_minute,
        )?;
        bounded("batch_addresses_burst", self.batch_addresses_burst, burst)?;
        if self.batch_addresses_burst < self.batch_max_size {
            return Err(format!(
                "api.ip.batch_addresses_burst ({}) is below api.ip.batch_max_size ({})",
                self.batch_addresses_burst, self.batch_max_size
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiConfig {
    pub enabled: bool,
    pub per_ip_per_minute: u32,
    pub per_ip_burst: u32,
    pub dns: ApiDnsConfig,
    pub tls: ApiTlsConfig,
    pub http: ApiHttpConfig,
    pub email: ApiEmailConfig,
    pub ip: ApiIpConfig,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            per_ip_per_minute: 60,
            per_ip_burst: 10,
            dns: ApiDnsConfig::default(),
            tls: ApiTlsConfig::default(),
            http: ApiHttpConfig::default(),
            email: ApiEmailConfig::default(),
            ip: ApiIpConfig::default(),
        }
    }
}

impl ApiConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.per_ip_per_minute == 0 {
            return Err("api.per_ip_per_minute must be greater than 0".into());
        }
        if self.per_ip_burst == 0 {
            return Err("api.per_ip_burst must be greater than 0".into());
        }
        self.ip.validate()?;
        self.tls.validate()?;
        self.http.validate()?;
        Ok(())
    }
}
