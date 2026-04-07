use std::collections::{BTreeMap, HashMap};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ProfileMeta {
    pub name: String,
    pub version: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SectionWeights {
    pub dns: u32,
    pub tls: u32,
    pub ip: u32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct HardFail {
    #[serde(default)]
    pub dns: Vec<String>,
    #[serde(default)]
    pub tls: Vec<String>,
    #[serde(default)]
    pub ip: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScoringProfile {
    pub meta: ProfileMeta,
    #[serde(default)]
    pub dns: HashMap<String, u32>,
    #[serde(default)]
    pub tls: HashMap<String, u32>,
    #[serde(default)]
    pub ip: HashMap<String, u32>,
    pub section_weights: SectionWeights,
    /// Ordered by value descending for grade lookup.
    pub thresholds: BTreeMap<String, u32>,
    #[serde(default)]
    pub hard_fail: HardFail,
}

impl ScoringProfile {
    pub fn from_toml(s: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(s)
    }

    pub fn embedded_default() -> Self {
        Self::from_toml(include_str!("../../profiles/default.toml"))
            .expect("embedded default profile is valid TOML")
    }
}
