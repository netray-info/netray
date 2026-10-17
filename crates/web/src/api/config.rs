//! The `[api]` config table (spec v2-api requirement 8).
use serde::Deserialize;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiDnsConfig {}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiTlsConfig {}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiHttpConfig {}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiEmailConfig {}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct ApiIpConfig {}

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
        Ok(())
    }
}
