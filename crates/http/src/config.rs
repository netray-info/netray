use std::num::NonZeroU32;

use serde::Deserialize;

/// The sections the HTTP module reads from a `[modules.http]` table.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    #[serde(default = "default_inspect")]
    pub inspect: InspectConfig,
    #[serde(default)]
    pub limits: ModuleLimits,
}

/// The per-target limits that apply to the module. A zero value is refused at deserialisation.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleLimits {
    #[serde(default = "default_per_target_per_minute")]
    pub per_target_per_minute: NonZeroU32,
    #[serde(default = "default_per_target_burst")]
    pub per_target_burst: NonZeroU32,
}

impl Default for ModuleLimits {
    fn default() -> Self {
        Self {
            per_target_per_minute: default_per_target_per_minute(),
            per_target_burst: default_per_target_burst(),
        }
    }
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

// --- Defaults ---

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

fn default_per_target_per_minute() -> NonZeroU32 {
    NonZeroU32::new(30).expect("non-zero default")
}
fn default_per_target_burst() -> NonZeroU32 {
    NonZeroU32::new(10).expect("non-zero default")
}
