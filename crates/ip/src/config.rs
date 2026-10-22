use serde::Deserialize;

/// Upper bounds for the per-client limits of `[api.ip]` (netray-web validates against them).
pub const HARD_CAP_RATE_LIMIT_PER_MINUTE: u32 = 600;
pub const HARD_CAP_RATE_LIMIT_BURST: u32 = 100;
pub const HARD_CAP_BATCH_SIZE: usize = 100;

/// The data sources the IP module reads from a `[modules.ip]` table: ifconfig-rs's data path
/// keys, same names and same load semantics.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleConfig {
    pub geoip_city_db: Option<String>,
    pub geoip_asn_db: Option<String>,
    pub user_agent_regexes: Option<String>,
    pub tor_exit_nodes: Option<String>,
    pub feodo_botnet_ips: Option<String>,
    pub cins_army_ips: Option<String>,
    pub cloud_provider_ranges: Option<String>,
    pub vpn_ranges: Option<String>,
    pub datacenter_ranges: Option<String>,
    pub bot_ranges: Option<String>,
    pub spamhaus_drop: Option<String>,
    pub asn_patterns: Option<String>,
    pub asn_info: Option<String>,
}
