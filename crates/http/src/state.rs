use std::sync::Arc;
use std::time::Duration;

use crate::config::{Config, EnrichmentConfig};
use crate::inspect::request::Outbound;
use crate::security::{IpExtractor, RateLimitState};
use netray_common::enrichment::{EnrichmentClient, EnrichmentMode};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
    pub outbound: Outbound,
}

pub(crate) fn enrichment_client(config: &EnrichmentConfig) -> Option<Arc<EnrichmentClient>> {
    config.ip_url.as_ref().map(|url| {
        Arc::new(EnrichmentClient::new(
            url,
            Duration::from_millis(config.timeout_ms),
            "spectra",
            None,
            EnrichmentMode::Plain,
        ))
    })
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        Self {
            ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
            rate_limiter: Arc::new(RateLimitState::new(&config.limits)),
            enrichment_client: enrichment_client(&config.enrichment),
            config: Arc::new(config.clone()),
            outbound: Outbound::system(),
        }
    }
}
