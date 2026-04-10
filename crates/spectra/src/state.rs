use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::security::{IpExtractor, RateLimitState};
use netray_common::enrichment::EnrichmentClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
    pub http_client: Arc<reqwest::Client>,
}

impl AppState {
    pub fn new(config: &Config) -> Self {
        let enrichment_client = config.enrichment.ip_url.as_ref().map(|url| {
            Arc::new(EnrichmentClient::new(
                url,
                Duration::from_millis(config.enrichment.timeout_ms),
                "spectra",
                None,
            ))
        });

        // Shared base client for all inspection probes. Per-request settings
        // (.resolve(), Origin header, redirect policy) are applied in execute_request.
        let http_client = Arc::new(
            reqwest::Client::builder()
                .build()
                .expect("failed to build shared HTTP client"),
        );

        Self {
            ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
            rate_limiter: Arc::new(RateLimitState::new(&config.limits)),
            enrichment_client,
            config: Arc::new(config.clone()),
            http_client,
        }
    }
}
