//! The apex's configuration: the `[badges]` and `[og_cards]` tables, with the serde shape and
//! the defaults lens gave them, and [`ApexConfig`], what `netray serve` hands to
//! [`crate::host::apex::build`].

use serde::Deserialize;

use crate::api::{AdmissionLimits, ApiConfig};
use crate::pages::InspectConfig;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BadgesConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_badge_ttl")]
    pub ttl_seconds: u64,
    #[serde(default = "default_badge_label")]
    pub default_label: String,
    #[serde(default = "default_badge_max_label_len")]
    pub max_label_len: usize,
}

fn default_badge_ttl() -> u64 {
    3600
}

fn default_badge_label() -> String {
    "lens".into()
}

fn default_badge_max_label_len() -> usize {
    32
}

impl Default for BadgesConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ttl_seconds: default_badge_ttl(),
            default_label: default_badge_label(),
            max_label_len: default_badge_max_label_len(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OgCardsConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl Default for OgCardsConfig {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn default_true() -> bool {
    true
}

/// Everything the apex's routes read from the configuration.
#[derive(Debug, Clone)]
pub struct ApexConfig {
    /// CIDRs of the proxies whose forwarding headers name the client.
    pub trusted_proxies: Vec<String>,
    /// The per-client budget, the domain run slots and the per-IP and global limiters.
    pub limits: AdmissionLimits,
    /// Whether a head inside the cache window is served without a run.
    pub cache_enabled: bool,
    /// The cache window in seconds; also the window the result page calls a stored entry fresh.
    pub cache_window_seconds: u64,
    pub badges: BadgesConfig,
    pub og_cards: OgCardsConfig,
    pub api: ApiConfig,
    pub inspect: InspectConfig,
}

impl Default for ApexConfig {
    /// lens's defaults: budget 30 per hour, 8 slots, 10 per minute and a burst of 3 per IP, 100
    /// per minute and a burst of 20 globally, the cache on with a 300 s window.
    fn default() -> Self {
        Self {
            trusted_proxies: Vec::new(),
            limits: AdmissionLimits {
                client_budget_per_hour: 30,
                domain_run_slots: 8,
                rate_limit_per_ip_per_minute: 10,
                rate_limit_per_ip_burst: 3,
                rate_limit_global_per_minute: 100,
                rate_limit_global_burst: 20,
            },
            cache_enabled: true,
            cache_window_seconds: 300,
            badges: BadgesConfig::default(),
            og_cards: OgCardsConfig::default(),
            api: ApiConfig::default(),
            inspect: InspectConfig::default(),
        }
    }
}
