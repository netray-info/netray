pub mod backend;
#[cfg(test)]
mod classify_tests;
pub mod config;
#[cfg(test)]
mod contract_golden;
pub mod enrichment;
pub mod extractors;
pub mod format;
pub mod handlers;
pub mod lookup;
pub mod module;
pub mod negotiate;
#[cfg(feature = "testing")]
pub mod testing;

pub use config::ModuleConfig;
pub use lookup::{IpLookup, IpRefusal, LookupOptions};
pub use module::{IpModule, enrichment_client, translate};

/// The attribution MaxMind's GeoLite2 licence requires next to data derived from the databases.
pub const GEOIP_ATTRIBUTION: &str =
    "This product includes GeoLite2 data created by MaxMind, available from https://www.maxmind.com.";
