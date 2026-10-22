use serde::Deserialize;

/// The sections the email module reads from a `[modules.email]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    #[serde(default = "default_dns")]
    pub dns: DnsConfig,
    #[serde(default = "default_dnsbl")]
    pub dnsbl: DnsblConfig,
    #[serde(default = "default_http")]
    pub http: HttpConfig,
    #[serde(default = "default_dkim")]
    pub dkim: DkimConfig,
    #[serde(default)]
    pub inspections: InspectionsConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionsConfig {
    #[serde(
        default = "default_max_concurrent",
        deserialize_with = "deserialize_max_concurrent"
    )]
    pub max_concurrent: usize,
}

/// Refuses 0 (no inspection could ever run) and values tokio's semaphore cannot hold.
fn deserialize_max_concurrent<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    if value == 0 || value > tokio::sync::Semaphore::MAX_PERMITS {
        return Err(serde::de::Error::custom(format!(
            "max_concurrent must be between 1 and {}",
            tokio::sync::Semaphore::MAX_PERMITS
        )));
    }
    Ok(value)
}

impl Default for InspectionsConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
        }
    }
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
pub struct DkimConfig {
    #[serde(default = "default_max_user_selectors")]
    pub max_user_selectors: usize,
}

// Default functions

fn default_max_concurrent() -> usize {
    16
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

fn default_dkim() -> DkimConfig {
    DkimConfig {
        max_user_selectors: default_max_user_selectors(),
    }
}

fn default_max_user_selectors() -> usize {
    5
}
