use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

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
    pub inspect_semaphore: Arc<Semaphore>,
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self, crate::error::MailError> {
        let dns_resolver = DnsResolver::new(&config.dns.resolvers, config.dns.timeout_ms)
            .await
            .map_err(|e| crate::error::MailError::DnsError(e.to_string()))?;

        let dnsbl_resolver = DnsResolver::new(&config.dnsbl.resolvers, config.dnsbl.timeout_ms)
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
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 5 {
                    return attempt.error("too many redirects");
                }
                let url = attempt.url();
                if url.scheme() != "https" {
                    return attempt.error("redirect to non-HTTPS URL rejected");
                }
                attempt.follow()
            }))
            .user_agent(format!(
                "beacon/{} (netray.info)",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .expect("failed to build HTTP client (follow redirects)");

        let rate_limiter =
            RateLimitState::new(&config.rate_limit).map_err(crate::error::MailError::Config)?;

        Ok(Self {
            ip_extractor: Arc::new(IpExtractor::new(&config.server.trusted_proxies)),
            rate_limiter: Arc::new(rate_limiter),
            dns_resolver: Arc::new(dns_resolver),
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            http_client,
            http_client_follow,
            enrichment_client,
            inspect_semaphore: Arc::new(Semaphore::new(config.server.max_concurrent_inspections)),
            config: Arc::new(config.clone()),
        })
    }

    /// Test-only constructor that accepts pre-built resolvers and skips the
    /// network-side initialisation performed by [`AppState::new`].
    ///
    /// SDD §4 item I3. `AppState` holds a concrete [`DnsResolver`] because
    /// Axum state erasure makes generic state painful; check-level tests that
    /// need to avoid real DNS use [`crate::dns::DnsLookup`] directly via
    /// `run_all_checks::<TestDnsResolver>(...)` rather than going through
    /// [`AppState`].
    #[cfg(test)]
    pub fn with_overrides(
        config: Config,
        dns_resolver: DnsResolver,
        dnsbl_resolver: DnsResolver,
    ) -> Self {
        let rate_limiter = RateLimitState::new(&config.rate_limit)
            .expect("test config must have a valid rate-limit string");
        Self {
            ip_extractor: Arc::new(IpExtractor::new(&[])),
            rate_limiter: Arc::new(rate_limiter),
            dns_resolver: Arc::new(dns_resolver),
            dnsbl_resolver: Arc::new(dnsbl_resolver),
            http_client: reqwest::Client::new(),
            http_client_follow: reqwest::Client::new(),
            enrichment_client: None,
            inspect_semaphore: Arc::new(Semaphore::new(16)),
            config: Arc::new(config),
        }
    }
}
