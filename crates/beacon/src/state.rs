use std::sync::Arc;
use std::time::Duration;

use crate::config::Config;
use crate::dns::DnsResolver;
use crate::security::{IpExtractor, RateLimitState};
use netray_common::enrichment::EnrichmentClient;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub ip_extractor: Arc<IpExtractor>,
    pub rate_limiter: Arc<RateLimitState>,
    pub dns_resolver: Arc<DnsResolver>,
    pub dnsbl_resolver: Arc<DnsResolver>,
    pub http_client: reqwest::Client,
    pub http_client_follow: reqwest::Client,
    pub enrichment_client: Option<Arc<EnrichmentClient>>,
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self, crate::error::MailError> {
        let dns_resolver = DnsResolver::new(&config.dns.resolvers, config.dns.timeout_ms)
            .await
            .map_err(|e| crate::error::MailError::DnsError(e.to_string()))?;

        let dnsbl_resolver =
            DnsResolver::new(&config.dnsbl.resolvers, config.dnsbl.timeout_ms)
                .await
                .map_err(|e| crate::error::MailError::DnsError(e.to_string()))?;

        let enrichment_client = config.backends.ip.as_ref().and_then(|ip_cfg| {
            ip_cfg.url.as_ref().map(|url| {
                Arc::new(EnrichmentClient::new(
                    url,
                    Duration::from_millis(ip_cfg.timeout_ms),
                    "beacon",
                    None,
                ))
            })
        });

        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.http.timeout_ms))
            .redirect(reqwest::redirect::Policy::none())
            .user_agent(format!(
                "beacon/{} (netray.info)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("failed to build HTTP client");

        let http_client_follow = reqwest::Client::builder()
            .timeout(Duration::from_millis(config.http.timeout_ms))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(format!(
                "beacon/{} (netray.info)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("failed to build HTTP client (follow redirects)");

        Ok(Self {
            ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
            rate_limiter: Arc::new(RateLimitState::new(&config.rate_limit)),
            dns_resolver: Arc::new(dns_resolver),
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            http_client,
            http_client_follow,
            enrichment_client,
            config: Arc::new(config.clone()),
        })
    }
}
